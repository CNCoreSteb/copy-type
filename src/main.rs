

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_config;
mod hotkey_config;
mod permissions;
mod i18n;

use app_config::{AppConfig, CloseAction};
use arboard::Clipboard;
use eframe::egui;
use enigo::{Enigo, Keyboard, Settings};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use hotkey_config::{HotkeyConfig, KeyCode};
use i18n::I18n;
use log::{debug, error, info, warn};
use permissions::{check_permissions, get_permission_fix_instructions, PermissionStatus};
use rand::Rng;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// 托盘菜单项 ID
const MENU_SHOW: &str = "show";
const MENU_TOGGLE: &str = "toggle";
const MENU_EXIT: &str = "exit";

/// 共享应用状态
#[derive(Clone)]
struct SharedState {
    /// 当前保存的剪贴板文本
    clipboard_text: Arc<Mutex<String>>,
    /// 上一次的剪贴板文本（用于检测变化）
    last_clipboard_text: Arc<Mutex<String>>,
    /// 剪贴板历史记录
    clipboard_history: Arc<Mutex<Vec<String>>>,
    /// 是否保存剪贴板历史
    history_enabled: Arc<Mutex<bool>>,
    /// 剪贴板历史最多保存条数
    history_max_items: Arc<Mutex<u32>>,
    /// 是否正在输入中（防止重复触发）
    is_typing: Arc<Mutex<bool>>,
    /// 程序是否启用
    enabled: Arc<Mutex<bool>>,
    /// 状态消息
    status_message: Arc<Mutex<String>>,
    /// 请求退出程序
    request_exit: Arc<AtomicBool>,
    /// 窗口是否可见
    #[allow(dead_code)]
    window_visible: Arc<AtomicBool>,
    /// 模拟输入时的延迟 (毫秒)
    typing_delay: Arc<Mutex<u64>>,
    /// 模拟输入时的随机偏差 (毫秒)
    typing_variance: Arc<Mutex<u64>>,
    /// 是否启用随机偏差
    typing_variance_enabled: Arc<Mutex<bool>>,
    /// 输入是否暂停
    typing_paused: Arc<Mutex<bool>>,
    /// 最近一次快捷键触发时间
    last_hotkey_trigger: Arc<Mutex<Option<Instant>>>,
    /// 当前快捷键 ID
    hotkey_id: Arc<Mutex<Option<u32>>>,
    /// 语言资源
    i18n: I18n,
}

impl SharedState {
    fn new(i18n: I18n) -> Self {
        let ready = i18n.t("status.ready");
        Self {
            clipboard_text: Arc::new(Mutex::new(String::new())),
            last_clipboard_text: Arc::new(Mutex::new(String::new())),
            clipboard_history: Arc::new(Mutex::new(Vec::new())),
            history_enabled: Arc::new(Mutex::new(false)),
            history_max_items: Arc::new(Mutex::new(0)),
            is_typing: Arc::new(Mutex::new(false)),
            enabled: Arc::new(Mutex::new(true)),
            status_message: Arc::new(Mutex::new(ready)),
            request_exit: Arc::new(AtomicBool::new(false)),
            window_visible: Arc::new(AtomicBool::new(true)),
            typing_delay: Arc::new(Mutex::new(0)),
            typing_variance: Arc::new(Mutex::new(0)),
            typing_variance_enabled: Arc::new(Mutex::new(false)),
            typing_paused: Arc::new(Mutex::new(false)),
            last_hotkey_trigger: Arc::new(Mutex::new(None)),
            hotkey_id: Arc::new(Mutex::new(None)),
            i18n,
        }
    }

    fn set_status(&self, msg: &str) {
        *self.status_message.lock().unwrap() = msg.to_string();
    }

    fn get_status(&self) -> String {
        self.status_message.lock().unwrap().clone()
    }

    fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }

    fn set_enabled(&self, enabled: bool) {
        *self.enabled.lock().unwrap() = enabled;
    }

    fn get_clipboard_text(&self) -> String {
        self.clipboard_text.lock().unwrap().clone()
    }

    fn is_typing(&self) -> bool {
        *self.is_typing.lock().unwrap()
    }

    fn toggle_typing_pause(&self) -> bool {
        let mut paused = self.typing_paused.lock().unwrap();
        *paused = !*paused;
        *paused
    }

    fn wait_if_paused(&self) {
        loop {
            if !*self.typing_paused.lock().unwrap() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn should_handle_hotkey(&self) -> bool {
        let mut last = self.last_hotkey_trigger.lock().unwrap();
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < Duration::from_millis(200) {
                return false;
            }
        }
        *last = Some(now);
        true
    }
    fn t(&self, key: &str) -> String {
        self.i18n.t(key)
    }

    fn tr<'a>(&self, key: &str, args: &[(&str, &'a str)]) -> String {
        self.i18n.tr(key, args)
    }

    fn record_history(&self, text: String) {
        if !*self.history_enabled.lock().unwrap() {
            return;
        }
        let max_items = *self.history_max_items.lock().unwrap();
        if max_items == 0 {
            return;
        }
        let mut history = self.clipboard_history.lock().unwrap();
        history.push(text);
        if history.len() > max_items as usize {
            let overflow = history.len() - max_items as usize;
            history.drain(0..overflow);
        }
    }

    fn clear_history(&self) {
        self.clipboard_history.lock().unwrap().clear();
    }

    fn trim_history(&self) {
        let max_items = *self.history_max_items.lock().unwrap();
        if max_items == 0 {
            self.clear_history();
            return;
        }
        let mut history = self.clipboard_history.lock().unwrap();
        if history.len() > max_items as usize {
            let overflow = history.len() - max_items as usize;
            history.drain(0..overflow);
        }
    }
    
    /// 执行模拟输入逻辑
    fn execute_typing(&self) {
        if !self.is_enabled() {
            warn!("{}", self.t("log.request_ignored_disabled"));
            return;
        }

        // 检查是否正在输入
        {
            let mut typing = self.is_typing.lock().unwrap();
            if *typing {
                warn!("{}", self.t("log.request_ignored_typing"));
                return;
            }
            *typing = true;
        }

        *self.typing_paused.lock().unwrap() = false;
        self.set_status(&self.t("status.typing"));
        let state = self.clone();
        let delay = *self.typing_delay.lock().unwrap();
        let variance = *self.typing_variance.lock().unwrap();
        let variance_enabled = *self.typing_variance_enabled.lock().unwrap();

        thread::spawn(move || {
            // 延迟输入，防止还未松开快捷键
            thread::sleep(Duration::from_millis(250));

            let text = state.clipboard_text.lock().unwrap().clone();

            if text.is_empty() {
                warn!("{}", state.t("log.clipboard_empty"));
                state.set_status(&state.t("status.clipboard_empty"));
                *state.typing_paused.lock().unwrap() = false;
                *state.is_typing.lock().unwrap() = false;
                return;
            }

            let len_str = text.len().to_string();
            let delay_str = delay.to_string();
            let variance_str = variance.to_string();
            let variance_enabled_str = variance_enabled.to_string();

            info!(
                "{}",
                state.tr(
                    "log.input_start",
                    &[
                        ("len", len_str.as_str()),
                        ("delay", delay_str.as_str()),
                        ("variance", variance_str.as_str()),
                        ("variance_enabled", variance_enabled_str.as_str())
                    ]
                )
            );

            let settings = Settings::default();
            let mut enigo = match Enigo::new(&settings) {
                Ok(e) => e,
                Err(e) => {
                    let err = e.to_string();
                    error!("{}", state.tr("log.input_init_error", &[("err", err.as_str())]));
                    state.set_status(&state.tr("status.input_init_error", &[("err", err.as_str())]));
                    *state.typing_paused.lock().unwrap() = false;
                    *state.is_typing.lock().unwrap() = false;
                    return;
                }
            };

            let mut result = Ok(());
            let mut rng = rand::thread_rng();

            for c in text.chars() {
                state.wait_if_paused();
                if let Err(e) = enigo.text(&c.to_string()) {
                    result = Err(e);
                    break;
                }

                 // 计算实际延迟
                let mut actual_delay = delay;
                if variance_enabled && variance > 0 {
                    // 在 [delay, delay + variance] 之间随机
                    let v = rng.gen_range(0..=variance);
                    actual_delay += v;
                }

                if actual_delay > 0 {
                    let mut remaining = actual_delay;
                    while remaining > 0 {
                        state.wait_if_paused();
                        let step = remaining.min(50);
                        thread::sleep(Duration::from_millis(step));
                        remaining -= step;
                    }
                }
            }

            if let Err(e) = result {
                let err = e.to_string();
                error!("{}", state.tr("log.input_error", &[("err", err.as_str())]));
                state.set_status(&state.tr("status.input_error", &[("err", err.as_str())]));
            } else {
                info!("{}", state.t("log.input_complete"));
                state.set_status(&state.t("status.input_complete"));
            }

            *state.typing_paused.lock().unwrap() = false;
            *state.is_typing.lock().unwrap() = false;
        });
    }
}

/// GUI 应用程序
struct CopyTypeApp {
    /// 共享状态
    state: SharedState,
    /// 国际化
    i18n: I18n,
    /// 快捷键管理器
    hotkey_manager: Option<GlobalHotKeyManager>,
    /// 当前快捷键 ID
    current_hotkey_id: Option<u32>,
    /// 当前已注册的快捷键
    current_hotkey: Option<HotKey>,
    /// 快捷键配置
    hotkey_config: HotkeyConfig,
    /// 临时快捷键配置（编辑中）
    temp_hotkey_config: HotkeyConfig,
    /// 应用程序配置
    app_config: AppConfig,
    /// 临时应用配置（编辑中）
    temp_app_config: AppConfig,
    /// 显示快捷键设置面板
    show_hotkey_settings: bool,
    /// 显示应用设置面板
    show_app_settings: bool,
    /// 显示权限警告
    show_permission_warning: bool,
    /// 权限状态
    permission_status: PermissionStatus,
    /// 系统托盘上下文，必须保持活跃
    #[allow(dead_code)]
    tray_context: Option<TrayContext>,
}

/// 保持托盘及其菜单项存活的结构体
struct TrayContext {
    #[allow(dead_code)]
    tray: TrayIcon,
    #[allow(dead_code)]
    show_item: MenuItem,
    #[allow(dead_code)]
    toggle_item: MenuItem,
    #[allow(dead_code)]
    exit_item: MenuItem,
    #[allow(dead_code)]
    separator: PredefinedMenuItem,
}

impl CopyTypeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 设置中文字体
        setup_fonts(&cc.egui_ctx);

        // 加载配置（统一从 AppConfig 加载）
        let app_config = AppConfig::load();
        let hotkey_config = app_config.hotkey.clone();
        let i18n = I18n::new(&app_config.language);

        // 检查权限
        let permission_status = check_permissions(&i18n);
        let show_permission_warning = !permission_status.all_granted();

        if show_permission_warning {
            let issues = permission_status.issues.join(", ");
            warn!("{}", i18n.tr("log.permission_issue", &[("issues", issues.as_str())]));
        }

        // 创建共享状态
        let state = SharedState::new(i18n.clone());
        // 初始化 state 中的配置值
        *state.typing_delay.lock().unwrap() = app_config.typing_delay;
        *state.typing_variance.lock().unwrap() = app_config.typing_variance;
        *state.typing_variance_enabled.lock().unwrap() = app_config.typing_variance_enabled;
        *state.history_enabled.lock().unwrap() = app_config.history_enabled;
        *state.history_max_items.lock().unwrap() = app_config.history_max_items;

        // 根据配置显示/隐藏控制台
        #[cfg(target_os = "windows")]
        {
            if app_config.show_console {
                show_console_window();
            } else {
                hide_console_window();
            }
        }

        // 创建系统托盘，并保存上下文
        let tray_context = create_tray_context(&i18n);
        
        let window_hwnd = get_window_hwnd(cc);
        let ctx_clone = cc.egui_ctx.clone();
        let i18n_tray = i18n.clone();
        let tray_state = state.clone();

        // 启动独立的托盘事件监控线程
        // 这解决了主线程阻塞导致托盘事件无法及时处理的问题
        std::thread::spawn(move || {
             let receiver = MenuEvent::receiver();
             loop {
                 // 使用阻塞式 recv()，这样一有事件就会立即响应
                 if let Ok(event) = receiver.recv() {
                    let id_str = event.id.0.as_str();
                    info!("{}", i18n_tray.tr("log.tray_event", &[("id", id_str)]));
                    
                    match id_str {
                        MENU_EXIT => {
                            info!("{}", i18n_tray.t("log.tray_exec_exit"));
                            // 强制退出，不等待任何UI更新
                            std::process::exit(0);
                        }
                        MENU_SHOW => {
                            info!("{}", i18n_tray.t("log.tray_exec_show"));
                            tray_state.window_visible.store(true, Ordering::SeqCst);
                            show_main_window(&ctx_clone, window_hwnd);
                        }
                        MENU_TOGGLE => {
                            let enabled = !tray_state.is_enabled();
                            let state_text = if enabled {
                                i18n_tray.t("common.enabled")
                            } else {
                                i18n_tray.t("common.disabled")
                            };
                            info!(
                                "{}",
                                i18n_tray.tr("log.tray_exec_toggle", &[("state", state_text.as_str())])
                            );
                            tray_state.set_enabled(enabled);
                            let status = if enabled {
                                i18n_tray.t("status.enabled")
                            } else {
                                i18n_tray.t("status.disabled")
                            };
                            tray_state.set_status(&status);
                            ctx_clone.request_repaint();
                        }
                        _ => {
                            ctx_clone.request_repaint();
                        }
                    }
                 }
             }
        });

        // 启动独立的快捷键事件监控线程
        // 这解决了窗口隐藏/最小化时快捷键不响应的问题
        let hotkey_state = state.clone();
        let i18n_hotkey = i18n.clone();
        std::thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = receiver.recv() {
                    let current_id = *hotkey_state.hotkey_id.lock().unwrap();
                    if let Some(id) = current_id {
                        if event.id == id {
                            if !hotkey_state.should_handle_hotkey() {
                                continue;
                            }
                            info!("{}", i18n_hotkey.t("log.hotkey_triggered"));
                            if hotkey_state.is_typing() {
                                let paused = hotkey_state.toggle_typing_pause();
                                if paused {
                                    hotkey_state
                                        .set_status(&i18n_hotkey.t("status.typing_paused"));
                                } else {
                                    hotkey_state.set_status(&i18n_hotkey.t("status.typing"));
                                }
                            } else {
                                hotkey_state.execute_typing();
                            }
                        }
                    }
                }
            }
        });

        let mut app = Self {
            state,
            i18n: i18n.clone(),
            hotkey_manager: None,
            current_hotkey_id: None,
            current_hotkey: None,
            hotkey_config: hotkey_config.clone(),
            temp_hotkey_config: hotkey_config,
            app_config: app_config.clone(),
            temp_app_config: app_config.clone(),
            show_hotkey_settings: false,
            show_app_settings: false,
            show_permission_warning,
            permission_status,
            tray_context,
        };

        // 初始化快捷键
        app.init_hotkey();

        // 启动剪贴板监控
        app.start_clipboard_monitor();

        // 如果设置为启动时最小化，则隐藏窗口
        if app_config.start_minimized {
            app.state.window_visible.store(false, Ordering::SeqCst);
            if let Some(ctx) = cc.egui_ctx.clone().into() {
                let ctx: egui::Context = ctx;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        app
    }

    /// 初始化快捷键管理器
    fn init_hotkey(&mut self) {
        match GlobalHotKeyManager::new() {
            Ok(manager) => {
                if let Some(hotkey) = self.hotkey_config.to_global_hotkey() {
                    match manager.register(hotkey) {
                        Ok(()) => {
                            self.current_hotkey_id = Some(hotkey.id());
                            self.current_hotkey = Some(hotkey);
                            *self.state.hotkey_id.lock().unwrap() = Some(hotkey.id());
                            let display = self.hotkey_config.display();
                            info!(
                                "{}",
                                self.i18n
                                    .tr("log.hotkey_registered", &[("hotkey", display.as_str())])
                            );
                            self.state.set_status(
                                &self
                                    .i18n
                                    .tr("status.hotkey_registered", &[("hotkey", display.as_str())]),
                            );
                        }
                        Err(e) => {
                            let err = e.to_string();
                            error!(
                                "{}",
                                self.i18n
                                    .tr("log.hotkey_register_fail", &[("err", err.as_str())])
                            );
                            self.state.set_status(
                                &self
                                    .i18n
                                    .tr("status.hotkey_register_fail", &[("err", err.as_str())]),
                            );
                        }
                    }
                }
                self.hotkey_manager = Some(manager);
            }
            Err(e) => {
                let err = e.to_string();
                error!(
                    "{}",
                    self.i18n
                        .tr("log.hotkey_manager_fail", &[("err", err.as_str())])
                );
                self.state
                    .set_status(
                        &self
                            .i18n
                            .tr("status.hotkey_manager_fail", &[("err", err.as_str())]),
                    );
            }
        }
    }

    /// 更新快捷键
    fn update_hotkey(&mut self) {
        // 先注销旧的快捷键
        if let (Some(manager), Some(old_hotkey)) = (&self.hotkey_manager, self.current_hotkey) {
            if let Err(e) = manager.unregister(old_hotkey) {
                let err = e.to_string();
                warn!(
                    "{}",
                    self.i18n
                        .tr("log.hotkey_unregister_fail", &[("err", err.as_str())])
                );
            } else {
                info!("{}", self.i18n.t("log.hotkey_unregistered"));
            }
            self.current_hotkey_id = None;
            self.current_hotkey = None;
            *self.state.hotkey_id.lock().unwrap() = None;
        }

        // 更新配置
        self.hotkey_config = self.temp_hotkey_config.clone();

        // 注册新的快捷键
        if let Some(manager) = &self.hotkey_manager {
            if let Some(new_hotkey) = self.hotkey_config.to_global_hotkey() {
                match manager.register(new_hotkey) {
                    Ok(()) => {
                        self.current_hotkey_id = Some(new_hotkey.id());
                        self.current_hotkey = Some(new_hotkey);
                        *self.state.hotkey_id.lock().unwrap() = Some(new_hotkey.id());
                        let display = self.hotkey_config.display();
                        info!(
                            "{}",
                            self.i18n
                                .tr("log.hotkey_updated", &[("hotkey", display.as_str())])
                        );
                        self.state.set_status(
                            &self
                                .i18n
                                .tr("status.hotkey_updated", &[("hotkey", display.as_str())]),
                        );

                        // 保存配置（更新 app_config.hotkey 并保存）
                        self.app_config.hotkey = self.hotkey_config.clone();
                        if let Err(e) = self.app_config.save() {
                            let err = e.to_string();
                            error!(
                                "{}",
                                self.i18n
                                    .tr("log.save_config_fail", &[("err", err.as_str())])
                            );
                        }
                    }
                    Err(e) => {
                        let err = e.to_string();
                        error!(
                            "{}",
                            self.i18n
                                .tr("log.hotkey_register_fail", &[("err", err.as_str())])
                        );
                        self.state.set_status(
                            &self
                                .i18n
                                .tr("status.hotkey_register_fail", &[("err", err.as_str())]),
                        );
                    }
                }
            }
        }
    }

    /// 启动剪贴板监控线程
    fn start_clipboard_monitor(&self) {
        let state = self.state.clone();

        thread::spawn(move || {
            let mut clipboard = match Clipboard::new() {
                Ok(cb) => cb,
                Err(e) => {
                    let err = e.to_string();
                    error!("{}", state.tr("log.clipboard_init_fail", &[("err", err.as_str())]));
                    state.set_status(&state.tr("status.clipboard_init_fail", &[("err", err.as_str())]));
                    return;
                }
            };

            info!("{}", state.t("log.clipboard_monitor_started"));

            loop {
                // 只在启用时监控
                if state.is_enabled() {
                    if let Ok(text) = clipboard.get_text() {
                        let last = state.last_clipboard_text.lock().unwrap().clone();

                        if text != last && !text.is_empty() {
                            let len_str = text.len().to_string();
                            info!(
                                "{}",
                                state.tr("log.clipboard_changed", &[("len", len_str.as_str())])
                            );
                            
                            // 安全地生成预览，如果 truncate_text panic 就用简单方式
                            let preview = std::panic::catch_unwind(|| truncate_text(&text, 50))
                                .unwrap_or_else(|_| {
                                    error!("truncate_text 发生错误，使用简单截断");
                                    text.chars().take(50).collect::<String>() + "..."
                                });
                            debug!("{}", state.tr("log.clipboard_preview", &[("preview", preview.as_str())]));

                            *state.clipboard_text.lock().unwrap() = text.clone();
                            *state.last_clipboard_text.lock().unwrap() = text.clone();
                            state.record_history(text);
                        }
                    }
                }

                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    /// 模拟键盘输入文本
    fn type_text(&self) {
        self.state.execute_typing();
    }

    /// 处理快捷键事件
    fn handle_hotkey_events(&self) {
        // 快捷键事件现在由后台线程处理
    }

}

impl eframe::App for CopyTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let i18n = self.i18n.clone();
        // 处理快捷键事件
        self.handle_hotkey_events();

        // 请求持续重绘以处理事件
        ctx.request_repaint_after(Duration::from_millis(50));

        // 权限警告窗口
        if self.show_permission_warning {
            egui::Window::new(i18n.t("ui.title_permission_warning"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(i18n.t("ui.label_permission_issues"));
                    ui.add_space(10.0);

                    if let Some(msg) = self.permission_status.get_warning_message(&i18n) {
                        ui.label(msg);
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.collapsing(i18n.t("ui.label_fix_suggestions"), |ui| {
                        ui.label(get_permission_fix_instructions(&i18n));
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button(i18n.t("ui.button_acknowledge")).clicked() {
                            self.show_permission_warning = false;
                        }
                        if ui.button(i18n.t("ui.button_exit")).clicked() {
                            self.state.request_exit.store(true, Ordering::SeqCst);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        }

        // 顶部菜单栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(i18n.t("ui.menu_file"), |ui| {
                    if ui.button(i18n.t("ui.menu_minimize_to_tray")).clicked() {
                        self.state.window_visible.store(false, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(i18n.t("ui.menu_exit")).clicked() {
                        self.state.request_exit.store(true, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(i18n.t("ui.menu_settings"), |ui| {
                    if ui.button(i18n.t("ui.menu_hotkey_settings")).clicked() {
                        self.show_hotkey_settings = true;
                        self.temp_hotkey_config = self.hotkey_config.clone();
                        ui.close_menu();
                    }
                    if ui.button(i18n.t("ui.menu_app_settings")).clicked() {
                        self.show_app_settings = true;
                        self.temp_app_config = self.app_config.clone();
                        ui.close_menu();
                    }
                });
                ui.menu_button(i18n.t("ui.menu_help"), |ui| {
                    if ui.button(i18n.t("ui.menu_check_permissions")).clicked() {
                        self.permission_status = check_permissions(&i18n);
                        self.show_permission_warning = !self.permission_status.all_granted();
                        if self.permission_status.all_granted() {
                            self.state.set_status(&i18n.t("status.permissions_ok"));
                        }
                        ui.close_menu();
                    }
                });
            });
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status = self.state.get_status();
                ui.label(i18n.tr("ui.label_status", &[("status", status.as_str())]));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.state.is_typing() {
                        ui.spinner();
                    }
                    // 权限状态指示
                    if !self.permission_status.all_granted() {
                        ui.label(
                            egui::RichText::new(i18n.t("ui.label_permission_problem"))
                                .color(egui::Color32::YELLOW),
                        );
                    }
                });
            });
        });

        // 主面板
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(i18n.t("ui.title_main"));
            ui.add_space(10.0);

            // 启用/禁用开关
            ui.horizontal(|ui| {
                ui.label(i18n.t("ui.label_app_status"));
                let mut enabled = self.state.is_enabled();
                let label = if enabled {
                    i18n.t("ui.label_enabled")
                } else {
                    i18n.t("ui.label_disabled")
                };
                if ui.toggle_value(&mut enabled, label).changed() {
                    self.state.set_enabled(enabled);
                    let status = if enabled {
                        i18n.t("status.enabled")
                    } else {
                        i18n.t("status.disabled")
                    };
                    self.state.set_status(&status);
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 快捷键显示
            ui.horizontal(|ui| {
                ui.label(i18n.t("ui.label_current_hotkey"));
                ui.code(self.hotkey_config.display());
                if ui.button(i18n.t("ui.button_modify")).clicked() {
                    self.show_hotkey_settings = true;
                    self.temp_hotkey_config = self.hotkey_config.clone();
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 剪贴板内容预览
            ui.label(i18n.t("ui.label_waiting_text"));
            let clipboard_text = self.state.get_clipboard_text();

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    egui::Frame::none()
                        .fill(ui.style().visuals.extreme_bg_color)
                        .inner_margin(8.0)
                        .rounding(4.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            if clipboard_text.is_empty() {
                                ui.label(egui::RichText::new(i18n.t("ui.label_empty")).italics().weak());
                            } else {
                                ui.label(&clipboard_text);
                            }
                        });
                });

            ui.add_space(10.0);

            // 文本信息
            if !clipboard_text.is_empty() {
                ui.horizontal(|ui| {
                    let char_count = clipboard_text.chars().count().to_string();
                    let line_count = clipboard_text.lines().count().to_string();
                    ui.label(i18n.tr("ui.label_char_count", &[("count", char_count.as_str())]));
                    ui.label(i18n.tr("ui.label_line_count", &[("count", line_count.as_str())]));
                });
            }

            ui.add_space(10.0);

            // 手动触发按钮
            ui.horizontal(|ui| {
                let typing = self.state.is_typing();
                let enabled = self.state.is_enabled();

                if ui
                    .add_enabled(
                        enabled && !typing && !clipboard_text.is_empty(),
                        egui::Button::new(i18n.t("ui.button_manual_type")),
                    )
                    .clicked()
                {
                    self.type_text();
                }

                if ui.button(i18n.t("ui.button_clear")).clicked() {
                    *self.state.clipboard_text.lock().unwrap() = String::new();
                    self.state.set_status(&i18n.t("status.cleared"));
                }
            });
        });

        // 快捷键设置窗口
        if self.show_hotkey_settings {
            egui::Window::new(i18n.t("ui.window_hotkey_settings"))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(i18n.t("ui.label_modifiers"));

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.temp_hotkey_config.ctrl, "Ctrl");
                        ui.checkbox(&mut self.temp_hotkey_config.shift, "Shift");
                        ui.checkbox(&mut self.temp_hotkey_config.alt, "Alt");
                        #[cfg(target_os = "macos")]
                        ui.checkbox(&mut self.temp_hotkey_config.meta, "Cmd");
                        #[cfg(not(target_os = "macos"))]
                        ui.checkbox(&mut self.temp_hotkey_config.meta, "Win");
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label(i18n.t("ui.label_keys"));
                        egui::ComboBox::from_label("")
                            .selected_text(self.temp_hotkey_config.key.display())
                            .show_ui(ui, |ui| {
                                for key in KeyCode::all() {
                                    ui.selectable_value(
                                        &mut self.temp_hotkey_config.key,
                                        key.clone(),
                                        key.display(),
                                    );
                                }
                            });
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label(i18n.t("ui.label_preview"));
                        ui.code(self.temp_hotkey_config.display());
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button(i18n.t("ui.button_save")).clicked() {
                            self.update_hotkey();
                            self.show_hotkey_settings = false;
                        }
                        if ui.button(i18n.t("ui.button_cancel")).clicked() {
                            self.show_hotkey_settings = false;
                        }
                    });
                });
        }

        // 应用设置窗口
        if self.show_app_settings {
            egui::Window::new(i18n.t("ui.window_app_settings"))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(i18n.t("ui.app.label_close_window_action"));

                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.temp_app_config.close_action,
                            CloseAction::MinimizeToTray,
                            i18n.t("ui.app.close_action_minimize_to_tray"),
                        );
                        ui.radio_value(
                            &mut self.temp_app_config.close_action,
                            CloseAction::ExitApp,
                            i18n.t("ui.app.close_action_exit"),
                        );
                    });

                    ui.add_space(10.0);

                    ui.checkbox(
                        &mut self.temp_app_config.start_minimized,
                        i18n.t("ui.app.checkbox_start_minimized"),
                    );

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label(i18n.t("ui.app.label_language"));
                        let selected_label = i18n
                            .available_languages()
                            .iter()
                            .find(|(code, _)| *code == self.temp_app_config.language.as_str())
                            .map(|(_, name)| (*name).to_string())
                            .unwrap_or_else(|| self.temp_app_config.language.clone());

                        egui::ComboBox::from_id_salt("language_select")
                            .selected_text(selected_label)
                            .show_ui(ui, |ui| {
                                for (code, name) in i18n.available_languages() {
                                    ui.selectable_value(
                                        &mut self.temp_app_config.language,
                                        code.to_string(),
                                        format!("{} ({})", name, code),
                                    );
                                }
                            });
                    });

                    ui.add_space(10.0);

                    ui.label(i18n.t("ui.app.group_typing_settings"));
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(i18n.t("ui.app.label_base_delay_ms"));
                            ui.add(egui::Slider::new(&mut self.temp_app_config.typing_delay, 0..=2000).text("ms"));
                            
                            // 计算并显示字每分钟
                            let chars_per_minute = if self.temp_app_config.typing_delay > 0 {
                                let avg_delay = self.temp_app_config.typing_delay as f64 
                                    + (self.temp_app_config.typing_variance as f64 / 2.0);
                                (60000.0 / avg_delay) as u32
                            } else {
                                9999 // 极速模式显示为 9999+
                            };
                            
                            let speed_text = if self.temp_app_config.typing_delay == 0 {
                                i18n.t("ui.app.typing_speed_infinite")
                            } else {
                                let cpm = chars_per_minute.to_string();
                                i18n.tr("ui.app.typing_speed", &[("cpm", cpm.as_str())])
                            };
                            
                            ui.label(egui::RichText::new(speed_text).weak());
                        });

                        ui.horizontal(|ui| {
                            ui.label(i18n.t("ui.app.label_variance_ms"));
                            ui.add(egui::Slider::new(&mut self.temp_app_config.typing_variance, 0..=1000).text("ms"));
                        });

                         ui.horizontal(|ui| {
                            ui.label(i18n.t("ui.app.label_presets"));
                             if ui.button(i18n.t("ui.app.preset_ultra")).clicked() {
                                self.temp_app_config.typing_delay = 0;
                                self.temp_app_config.typing_variance = 0;
                            }
                            if ui.button(i18n.t("ui.app.preset_fast")).clicked() {
                                self.temp_app_config.typing_delay = 10;
                                self.temp_app_config.typing_variance = 5;
                            }
                            if ui.button(i18n.t("ui.app.preset_normal")).clicked() {
                                self.temp_app_config.typing_delay = 50;
                                self.temp_app_config.typing_variance = 30;
                            }
                             if ui.button(i18n.t("ui.app.preset_slow")).clicked() {
                                self.temp_app_config.typing_delay = 150;
                                self.temp_app_config.typing_variance = 50;
                            }
                        });


                        ui.label(egui::RichText::new(i18n.t("ui.app.typing_tip")).small().weak());
                    });

                    ui.add_space(10.0);
                    ui.label(i18n.t("ui.app.group_history_settings"));
                    ui.group(|ui| {
                        ui.checkbox(
                            &mut self.temp_app_config.history_enabled,
                            i18n.t("ui.app.checkbox_history_enabled"),
                        );
                        ui.horizontal(|ui| {
                            ui.label(i18n.t("ui.app.label_history_max_items"));
                            ui.add_enabled(
                                self.temp_app_config.history_enabled,
                                egui::Slider::new(&mut self.temp_app_config.history_max_items, 1..=100)
                                    .text(i18n.t("ui.app.history_item_unit")),
                            );
                        });
                    });
                    
                    #[cfg(target_os = "windows")]
                    {
                        ui.add_space(5.0);
                        ui.checkbox(
                            &mut self.temp_app_config.show_console,
                            i18n.t("ui.app.checkbox_show_console"),
                        );
                        ui.label(egui::RichText::new(i18n.t("ui.app.label_restart_required")).small().weak());
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button(i18n.t("ui.button_save")).clicked() {
                            #[cfg(target_os = "windows")]
                            {
                                let console_changed = self.app_config.show_console != self.temp_app_config.show_console;
                                if console_changed {
                                    if self.temp_app_config.show_console {
                                        show_console_window();
                                    } else {
                                        hide_console_window();
                                    }
                                }
                            }

                            self.temp_app_config.history_max_items =
                                self.temp_app_config.history_max_items.clamp(1, 100);
                            
                            self.app_config = self.temp_app_config.clone();
                            // 更新 state 中的配置
                            *self.state.typing_delay.lock().unwrap() = self.app_config.typing_delay;
                            *self.state.typing_variance.lock().unwrap() = self.app_config.typing_variance;
                            *self.state.typing_variance_enabled.lock().unwrap() = self.app_config.typing_variance_enabled;
                            *self.state.history_enabled.lock().unwrap() = self.app_config.history_enabled;
                            *self.state.history_max_items.lock().unwrap() = self.app_config.history_max_items;
                            if self.app_config.history_enabled {
                                self.state.trim_history();
                            } else {
                                self.state.clear_history();
                            }
                            self.i18n.set_language(&self.app_config.language);
                            
                            // 保存时包含当前的快捷键配置
                            self.app_config.hotkey = self.hotkey_config.clone();
                            if let Err(e) = self.app_config.save() {
                                let err = e.to_string();
                                error!(
                                    "{}",
                                    i18n.tr("log.save_app_config_fail", &[("err", err.as_str())])
                                );
                            } else {
                                self.state.set_status(&i18n.t("status.app_settings_saved"));
                            }
                            self.show_app_settings = false;
                        }
                        if ui.button(i18n.t("ui.button_cancel")).clicked() {
                            self.show_app_settings = false;
                        }
                    });
                });
        }

        // 检查关闭请求
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.state.request_exit.load(Ordering::SeqCst) {
                match self.app_config.close_action {
                    CloseAction::MinimizeToTray => {
                        // 取消关闭，改为隐藏
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        self.state.window_visible.store(false, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        info!("{}", i18n.t("log.window_minimized_to_tray"));
                    }
                    CloseAction::ExitApp => {
                        // 允许关闭
                        info!("{}", i18n.t("log.app_exit"));
                    }
                }
            }
        }
    }
}

/// 设置中文字体
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 在 Windows 上使用微软雅黑字体
    #[cfg(target_os = "windows")]
    {
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts.font_data.insert(
                "msyh".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "msyh".to_owned());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "msyh".to_owned());
        }
    }

    // 在 macOS 上使用苹方字体
    #[cfg(target_os = "macos")]
    {
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
            fonts.font_data.insert(
                "pingfang".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "pingfang".to_owned());
        }
    }

    // 在 Linux 上使用 Noto Sans CJK
    #[cfg(target_os = "linux")]
    {
        let font_paths = [
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ];

        for path in &font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "noto".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(font_data)),
                );

                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "noto".to_owned());
                break;
            }
        }
    }

    ctx.set_fonts(fonts);
}

/// Windows: 显示控制台窗口
#[cfg(target_os = "windows")]
fn show_console_window() {
    use windows::Win32::System::Console::{AllocConsole, GetConsoleWindow};
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};

    unsafe {
        let _ = AllocConsole();
        let console_window = GetConsoleWindow();
        if !console_window.is_invalid() {
            let _ = ShowWindow(console_window, SW_SHOW);
            info!("Console window shown");
        }
    }
}

/// Windows: 隐藏控制台窗口
#[cfg(target_os = "windows")]
fn hide_console_window() {
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        let console_window = GetConsoleWindow();
        if !console_window.is_invalid() {
            let _ = ShowWindow(console_window, SW_HIDE);
        }
    }
}

/// 创建系统托盘图标
fn create_tray_context(i18n: &I18n) -> Option<TrayContext> {
    // 创建托盘菜单
    let menu = Menu::new();

    let show_text = i18n.t("tray.menu_show");
    let toggle_text = i18n.t("tray.menu_toggle");
    let exit_text = i18n.t("tray.menu_exit");

    let show_item = MenuItem::with_id(MENU_SHOW, &show_text, true, None);
    let toggle_item = MenuItem::with_id(MENU_TOGGLE, &toggle_text, true, None);
    let separator = PredefinedMenuItem::separator();
    let exit_item = MenuItem::with_id(MENU_EXIT, &exit_text, true, None);

    if let Err(e) = menu.append(&show_item) {
        let err = e.to_string();
        error!("{}", i18n.tr("tray.log.add_show_fail", &[("err", err.as_str())]));
    }
    if let Err(e) = menu.append(&toggle_item) {
        let err = e.to_string();
        error!(
            "{}",
            i18n.tr("tray.log.add_toggle_fail", &[("err", err.as_str())])
        );
    }
    if let Err(e) = menu.append(&separator) {
        let err = e.to_string();
        error!("{}", i18n.tr("tray.log.add_sep_fail", &[("err", err.as_str())]));
    }
    if let Err(e) = menu.append(&exit_item) {
        let err = e.to_string();
        error!(
            "{}",
            i18n.tr("tray.log.add_exit_fail", &[("err", err.as_str())])
        );
    }
    
    info!(
        "{}",
        i18n.tr("tray.log.menu_created", &[("count", "3")])
    );

    // 创建托盘图标（使用默认图标）
    let icon = create_default_icon();
    let tooltip = i18n.t("tray.tooltip");

    match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(&tooltip)
        .with_icon(icon)
        .build()
    {
        Ok(tray) => {
            info!("{}", i18n.t("tray.log.created"));
            // 将所有相关对象包含在上下文中返回
            Some(TrayContext {
                tray,
                show_item,
                toggle_item,
                exit_item,
                separator
            })
        }
        Err(e) => {
            let err = e.to_string();
            error!(
                "{}",
                i18n.tr("tray.log.create_fail", &[("err", err.as_str())])
            );
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn get_window_hwnd(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    cc.window_handle().ok().and_then(|handle| match handle.as_raw() {
        RawWindowHandle::Win32(win) => Some(win.hwnd.get()),
        _ => None,
    })
}

#[cfg(not(target_os = "windows"))]
fn get_window_hwnd(_cc: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

fn show_main_window(ctx: &egui::Context, window_hwnd: Option<isize>) {
    #[cfg(target_os = "windows")]
    {
        if let Some(hwnd) = window_hwnd {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

            unsafe {
                let hwnd = HWND(hwnd as *mut std::ffi::c_void);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window_hwnd;
    }

    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

/// 创建默认托盘图标
fn create_default_icon() -> tray_icon::Icon {
    // 创建一个简单的 16x16 图标
    let size = 16u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            // 创建一个简单的渐变图标
            let r = ((x as f32 / size as f32) * 100.0 + 100.0) as u8;
            let g = ((y as f32 / size as f32) * 100.0 + 100.0) as u8;
            let b = 200u8;
            let a = 255u8;

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create icon")
}

/// 截断文本用于日志显示
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.replace('\n', "\\n").replace('\r', "\\r")
    } else {
        // 找到安全的字符边界进行截断
        let truncate_pos = text.char_indices()
            .take_while(|(idx, _)| *idx < max_len)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);
        
        format!(
            "{}...",
            text[..truncate_pos].replace('\n', "\\n").replace('\r', "\\r")
        )
    }
}

fn main() -> eframe::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    info!("=================================");
    let startup_config = AppConfig::load();
    let startup_i18n = I18n::new(&startup_config.language);
    info!("  {}", startup_i18n.t("ui.title_main"));
    info!("=================================");

    // 检查权限（启动时也检查一次用于日志记录）
    let perm = check_permissions(&startup_i18n);
    if !perm.all_granted() {
        let issues = perm.issues.join(", ");
        warn!(
            "{}",
            startup_i18n.tr("log.permission_issue", &[("issues", issues.as_str())])
        );
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 500.0])
            .with_min_inner_size([350.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Copy&Type",
        options,
        Box::new(|cc| Ok(Box::new(CopyTypeApp::new(cc)))),
    )
}
