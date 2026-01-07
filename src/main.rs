

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_config;
mod hotkey_config;
mod permissions;

use app_config::{AppConfig, CloseAction};
use arboard::Clipboard;
use eframe::egui;
use enigo::{Enigo, Keyboard, Settings};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use hotkey_config::{HotkeyConfig, KeyCode};
use log::{debug, error, info, warn};
use permissions::{check_permissions, get_permission_fix_instructions, PermissionStatus};
use rand::Rng;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

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
    /// 当前快捷键 ID
    hotkey_id: Arc<Mutex<Option<u32>>>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            clipboard_text: Arc::new(Mutex::new(String::new())),
            last_clipboard_text: Arc::new(Mutex::new(String::new())),
            is_typing: Arc::new(Mutex::new(false)),
            enabled: Arc::new(Mutex::new(true)),
            status_message: Arc::new(Mutex::new("就绪".to_string())),
            request_exit: Arc::new(AtomicBool::new(false)),
            window_visible: Arc::new(AtomicBool::new(true)),
            typing_delay: Arc::new(Mutex::new(0)),
            typing_variance: Arc::new(Mutex::new(0)),
            typing_variance_enabled: Arc::new(Mutex::new(false)),
            hotkey_id: Arc::new(Mutex::new(None)),
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
    
    /// 执行模拟输入逻辑
    fn execute_typing(&self) {
        if !self.is_enabled() {
            warn!("程序已禁用，忽略输入请求");
            return;
        }

        // 检查是否正在输入
        {
            let mut typing = self.is_typing.lock().unwrap();
            if *typing {
                warn!("正在输入中，忽略此次请求");
                return;
            }
            *typing = true;
        }

        self.set_status("正在输入...");
        let state = self.clone();
        let delay = *self.typing_delay.lock().unwrap();
        let variance = *self.typing_variance.lock().unwrap();
        let variance_enabled = *self.typing_variance_enabled.lock().unwrap();

        thread::spawn(move || {
            // 稍微延迟，让用户松开快捷键
            thread::sleep(Duration::from_millis(150));

            let text = state.clipboard_text.lock().unwrap().clone();

            if text.is_empty() {
                warn!("剪贴板为空，无法输入");
                state.set_status("剪贴板为空");
                *state.is_typing.lock().unwrap() = false;
                return;
            }

            info!(
                "开始模拟输入 ({} 字符, 延迟 {}ms, 偏差 {}ms, 启用偏差: {})",
                text.len(),
                delay,
                variance,
                variance_enabled
            );

            let settings = Settings::default();
            let mut enigo = match Enigo::new(&settings) {
                Ok(e) => e,
                Err(e) => {
                    error!("无法初始化键盘模拟: {}", e);
                    state.set_status(&format!("键盘模拟失败: {}", e));
                    *state.is_typing.lock().unwrap() = false;
                    return;
                }
            };

            let result = if delay > 0 || (variance_enabled && variance > 0) {
                let mut res = Ok(());
                let mut rng = rand::thread_rng();

                for c in text.chars() {
                    if let Err(e) = enigo.text(&c.to_string()) {
                        res = Err(e);
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
                        thread::sleep(Duration::from_millis(actual_delay));
                    }
                }
                res
            } else {
                enigo.text(&text)
            };

            if let Err(e) = result {
                error!("输入文本失败: {}", e);
                state.set_status(&format!("输入失败: {}", e));
            } else {
                info!("输入完成");
                state.set_status("输入完成");
            }

            *state.is_typing.lock().unwrap() = false;
        });
    }
}

/// GUI 应用程序
struct CopyTypeApp {
    /// 共享状态
    state: SharedState,
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

        // 检查权限
        let permission_status = check_permissions();
        let show_permission_warning = !permission_status.all_granted();

        if show_permission_warning {
            warn!("权限检查发现问题: {:?}", permission_status.issues);
        }

        // 加载配置（统一从 AppConfig 加载）
        let app_config = AppConfig::load();
        let hotkey_config = app_config.hotkey.clone();

        // 创建共享状态
        let state = SharedState::new();
        // 初始化 state 中的配置值
        *state.typing_delay.lock().unwrap() = app_config.typing_delay;
        *state.typing_variance.lock().unwrap() = app_config.typing_variance;
        *state.typing_variance_enabled.lock().unwrap() = app_config.typing_variance_enabled;

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
        let tray_context = create_tray_context();
        
        let ctx_clone = cc.egui_ctx.clone();
        let _state_enabled_clone = Arc::new(Mutex::new(app_config.auto_start)); // 这里只是暂时的占位，真正的状态在 SharedState::new 中

        // 启动独立的托盘事件监控线程
        // 这解决了主线程阻塞导致托盘事件无法及时处理的问题
        std::thread::spawn(move || {
             let receiver = MenuEvent::receiver();
             loop {
                 // 使用阻塞式 recv()，这样一有事件就会立即响应
                 if let Ok(event) = receiver.recv() {
                    let id_str = event.id.0.as_str();
                    info!("后台线程: 收到托盘事件 {}", id_str);
                    
                    match id_str {
                        MENU_EXIT => {
                            info!("Backgrond: EXIT command received. Terminating process immediately.");
                            // 强制退出，不等待任何UI更新
                            std::process::exit(0);
                        }
                        MENU_SHOW => {
                            ctx_clone.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx_clone.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx_clone.request_repaint();
                        }
                        MENU_TOGGLE => {
                            // 切换逻辑比较复杂，我们还是让主线程处理
                            // 但我们需要确保主线程被唤醒
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
        std::thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = receiver.recv() {
                    let current_id = *hotkey_state.hotkey_id.lock().unwrap();
                    if let Some(id) = current_id {
                        if event.id == id {
                            info!("后台线程: 检测到快捷键触发");
                            hotkey_state.execute_typing();
                        }
                    }
                }
            }
        });

        let mut app = Self {
            state,
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
                            info!("已注册快捷键: {}", self.hotkey_config.display());
                            self.state.set_status(&format!(
                                "快捷键已注册: {}",
                                self.hotkey_config.display()
                            ));
                        }
                        Err(e) => {
                            error!("注册快捷键失败: {}", e);
                            self.state.set_status(&format!("快捷键注册失败: {}", e));
                        }
                    }
                }
                self.hotkey_manager = Some(manager);
            }
            Err(e) => {
                error!("初始化快捷键管理器失败: {}", e);
                self.state
                    .set_status(&format!("快捷键管理器初始化失败: {}", e));
            }
        }
    }

    /// 更新快捷键
    fn update_hotkey(&mut self) {
        // 先注销旧的快捷键
        if let (Some(manager), Some(old_hotkey)) = (&self.hotkey_manager, self.current_hotkey) {
            if let Err(e) = manager.unregister(old_hotkey) {
                warn!("注销旧快捷键失败: {}", e);
            } else {
                info!("已注销旧快捷键");
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
                        info!("已注册新快捷键: {}", self.hotkey_config.display());
                        self.state
                            .set_status(&format!("快捷键已更新: {}", self.hotkey_config.display()));

                        // 保存配置（更新 app_config.hotkey 并保存）
                        self.app_config.hotkey = self.hotkey_config.clone();
                        if let Err(e) = self.app_config.save() {
                            error!("保存配置失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("注册新快捷键失败: {}", e);
                        self.state.set_status(&format!("快捷键注册失败: {}", e));
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
                    error!("无法初始化剪贴板: {}", e);
                    state.set_status(&format!("剪贴板初始化失败: {}", e));
                    return;
                }
            };

            info!("剪贴板监控已启动");

            loop {
                // 只在启用时监控
                if state.is_enabled() {
                    if let Ok(text) = clipboard.get_text() {
                        let last = state.last_clipboard_text.lock().unwrap().clone();

                        if text != last && !text.is_empty() {
                            info!("检测到新的剪贴板内容 ({} 字符)", text.len());
                            debug!("内容预览: {}", truncate_text(&text, 50));

                            *state.clipboard_text.lock().unwrap() = text.clone();
                            *state.last_clipboard_text.lock().unwrap() = text;
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

    /// 处理托盘菜单事件
    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        // 处理所有待处理的托盘事件
        let receiver = MenuEvent::receiver();
        let mut event_count = 0;
        
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    event_count += 1;
                    info!("收到托盘菜单事件 #{}: id={}", event_count, event.id.0);
                    
                    let id_str = event.id.0.as_str();
                    info!("匹配菜单ID: '{}'", id_str);
                    
                    match id_str {
                        MENU_SHOW => {
                            info!("执行: 显示窗口");
                            self.state.window_visible.store(true, Ordering::SeqCst);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        }
                        MENU_TOGGLE => {
                            let enabled = !self.state.is_enabled();
                            info!("执行: 切换状态为 {}", if enabled { "启用" } else { "禁用" });
                            self.state.set_enabled(enabled);
                            self.state.set_status(if enabled { "程序已启用" } else { "程序已禁用" });
                        }
                        MENU_EXIT => {
                            info!("执行: 退出程序");
                            self.tray_context = None; // 清理托盘图标
                            std::process::exit(0); // 直接退出进程，避免延迟
                        }
                        _ => {
                            warn!("收到未知的托盘菜单ID: '{}'", id_str);
                        }
                    }
                }
                Err(_) => {
                    // 没有更多事件或通道已断开
                    if event_count > 0 {
                        info!("本轮处理了 {} 个托盘事件", event_count);
                    }
                    break;
                }
            }
        }
    }
}

impl eframe::App for CopyTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理快捷键事件
        self.handle_hotkey_events();

        // 处理托盘菜单事件
        self.handle_tray_events(ctx);

        // 请求持续重绘以处理事件
        ctx.request_repaint_after(Duration::from_millis(50));

        // 权限警告窗口
        if self.show_permission_warning {
            egui::Window::new("⚠️ 权限警告")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("检测到以下权限问题：");
                    ui.add_space(10.0);

                    if let Some(msg) = self.permission_status.get_warning_message() {
                        ui.label(msg);
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.collapsing("查看修复建议", |ui| {
                        ui.label(get_permission_fix_instructions());
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("我知道了，继续使用").clicked() {
                            self.show_permission_warning = false;
                        }
                        if ui.button("退出程序").clicked() {
                            self.state.request_exit.store(true, Ordering::SeqCst);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        }

        // 顶部菜单栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("文件", |ui| {
                    if ui.button("最小化到托盘").clicked() {
                        self.state.window_visible.store(false, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("退出").clicked() {
                        self.state.request_exit.store(true, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("设置", |ui| {
                    if ui.button("快捷键设置").clicked() {
                        self.show_hotkey_settings = true;
                        self.temp_hotkey_config = self.hotkey_config.clone();
                        ui.close_menu();
                    }
                    if ui.button("应用设置").clicked() {
                        self.show_app_settings = true;
                        self.temp_app_config = self.app_config.clone();
                        ui.close_menu();
                    }
                });
                ui.menu_button("帮助", |ui| {
                    if ui.button("检查权限").clicked() {
                        self.permission_status = check_permissions();
                        self.show_permission_warning = !self.permission_status.all_granted();
                        if self.permission_status.all_granted() {
                            self.state.set_status("权限检查通过");
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
                ui.label(format!("状态: {}", status));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.state.is_typing() {
                        ui.spinner();
                    }
                    // 权限状态指示
                    if !self.permission_status.all_granted() {
                        ui.label(egui::RichText::new("⚠️ 权限问题").color(egui::Color32::YELLOW));
                    }
                });
            });
        });

        // 主面板
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Copy-Type");
            ui.add_space(10.0);

            // 启用/禁用开关
            ui.horizontal(|ui| {
                ui.label("程序状态:");
                let mut enabled = self.state.is_enabled();
                let label = if enabled { "✅ 已启用" } else { "❌ 已禁用" };
                if ui.toggle_value(&mut enabled, label).changed() {
                    self.state.set_enabled(enabled);
                    self.state
                        .set_status(if enabled { "程序已启用" } else { "程序已禁用" });
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 快捷键显示
            ui.horizontal(|ui| {
                ui.label("当前快捷键:");
                ui.code(self.hotkey_config.display());
                if ui.button("修改").clicked() {
                    self.show_hotkey_settings = true;
                    self.temp_hotkey_config = self.hotkey_config.clone();
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 剪贴板内容预览
            ui.label("等待输入的文本:");
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
                                ui.label(egui::RichText::new("(空)").italics().weak());
                            } else {
                                ui.label(&clipboard_text);
                            }
                        });
                });

            ui.add_space(10.0);

            // 文本信息
            if !clipboard_text.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(format!("字符数: {}", clipboard_text.chars().count()));
                    ui.label(format!("行数: {}", clipboard_text.lines().count()));
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
                        egui::Button::new("▶ 手动输入"),
                    )
                    .clicked()
                {
                    self.type_text();
                }

                if ui.button("🗑 清空").clicked() {
                    *self.state.clipboard_text.lock().unwrap() = String::new();
                    self.state.set_status("已清空");
                }
            });
        });

        // 快捷键设置窗口
        if self.show_hotkey_settings {
            egui::Window::new("快捷键设置")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("修饰键:");

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
                        ui.label("按键:");
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
                        ui.label("预览:");
                        ui.code(self.temp_hotkey_config.display());
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("保存").clicked() {
                            self.update_hotkey();
                            self.show_hotkey_settings = false;
                        }
                        if ui.button("取消").clicked() {
                            self.show_hotkey_settings = false;
                        }
                    });
                });
        }

        // 应用设置窗口
        if self.show_app_settings {
            egui::Window::new("应用设置")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("关闭窗口时:");

                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.temp_app_config.close_action,
                            CloseAction::MinimizeToTray,
                            "最小化到托盘",
                        );
                        ui.radio_value(
                            &mut self.temp_app_config.close_action,
                            CloseAction::ExitApp,
                            "退出程序",
                        );
                    });

                    ui.add_space(10.0);

                    ui.checkbox(&mut self.temp_app_config.start_minimized, "启动时最小化到托盘");

                    ui.add_space(10.0);
                    
                    ui.label("模拟输入设置:");
                    ui.group(|ui| {
                        ui.label("模拟输入设置:");
                        
                        ui.horizontal(|ui| {
                            ui.label("基础延迟 (毫秒):");
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
                                "≈ 9999+ 字/分钟".to_string()
                            } else {
                                format!("≈ {} 字/分钟", chars_per_minute)
                            };
                            
                            ui.label(egui::RichText::new(speed_text).weak());
                        });

                        ui.horizontal(|ui| {
                            ui.label("随机偏差 (毫秒):");
                            ui.add(egui::Slider::new(&mut self.temp_app_config.typing_variance, 0..=1000).text("ms"));
                        });

                         ui.horizontal(|ui| {
                            ui.label("预设:");
                             if ui.button("极速").clicked() {
                                self.temp_app_config.typing_delay = 0;
                                self.temp_app_config.typing_variance = 0;
                            }
                            if ui.button("快速").clicked() {
                                self.temp_app_config.typing_delay = 10;
                                self.temp_app_config.typing_variance = 5;
                            }
                            if ui.button("正常").clicked() {
                                self.temp_app_config.typing_delay = 50;
                                self.temp_app_config.typing_variance = 30;
                            }
                             if ui.button("慢速").clicked() {
                                self.temp_app_config.typing_delay = 150;
                                self.temp_app_config.typing_variance = 50;
                            }
                        });


                        ui.label(egui::RichText::new("增加随机偏差可以让输入更像人类，避免被反作弊检测。").small().weak());
                    });
                    
                    #[cfg(target_os = "windows")]
                    {
                        ui.add_space(5.0);
                        ui.checkbox(&mut self.temp_app_config.show_console, "显示调试控制台");
                        ui.label(egui::RichText::new("需要重启程序生效").small().weak());
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("保存").clicked() {
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
                            
                            self.app_config = self.temp_app_config.clone();
                            // 更新 state 中的配置
                            *self.state.typing_delay.lock().unwrap() = self.app_config.typing_delay;
                            *self.state.typing_variance.lock().unwrap() = self.app_config.typing_variance;
                            *self.state.typing_variance_enabled.lock().unwrap() = self.app_config.typing_variance_enabled;
                            
                            // 保存时包含当前的快捷键配置
                            self.app_config.hotkey = self.hotkey_config.clone();
                            if let Err(e) = self.app_config.save() {
                                error!("保存应用配置失败: {}", e);
                            } else {
                                self.state.set_status("应用设置已保存");
                            }
                            self.show_app_settings = false;
                        }
                        if ui.button("取消").clicked() {
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
                        info!("窗口已最小化到托盘");
                    }
                    CloseAction::ExitApp => {
                        // 允许关闭
                        info!("程序退出");
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
            info!("控制台已显示");
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
fn create_tray_context() -> Option<TrayContext> {
    // 创建托盘菜单
    let menu = Menu::new();

    let show_item = MenuItem::with_id(MENU_SHOW, "显示窗口", true, None);
    let toggle_item = MenuItem::with_id(MENU_TOGGLE, "启用/禁用", true, None);
    let separator = PredefinedMenuItem::separator();
    let exit_item = MenuItem::with_id(MENU_EXIT, "退出", true, None);

    if let Err(e) = menu.append(&show_item) {
        error!("添加显示菜单项失败: {}", e);
    }
    if let Err(e) = menu.append(&toggle_item) {
        error!("添加切换菜单项失败: {}", e);
    }
    if let Err(e) = menu.append(&separator) {
        error!("添加分隔符失败: {}", e);
    }
    if let Err(e) = menu.append(&exit_item) {
        error!("添加退出菜单项失败: {}", e);
    }
    
    info!("托盘菜单已创建，包含 {} 个菜单项", 3);

    // 创建托盘图标（使用默认图标）
    let icon = create_default_icon();

    match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Copy-Type - 剪贴板模拟输入")
        .with_icon(icon)
        .build()
    {
        Ok(tray) => {
            info!("系统托盘已创建");
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
            error!("创建系统托盘失败: {}", e);
            None
        }
    }
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
        format!(
            "{}...",
            text[..max_len].replace('\n', "\\n").replace('\r', "\\r")
        )
    }
}

fn main() -> eframe::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    info!("=================================");
    info!("  Copy-Type 启动");
    info!("=================================");

    // 检查权限（启动时也检查一次用于日志记录）
    let perm = check_permissions();
    if !perm.all_granted() {
        warn!("权限检查发现问题，程序可能无法正常工作");
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 500.0])
            .with_min_inner_size([350.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Copy-Type",
        options,
        Box::new(|cc| Ok(Box::new(CopyTypeApp::new(cc)))),
    )
}
