

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_config;
mod hotkey_config;
mod permissions;
mod i18n;
#[cfg(test)]
mod tests;

/// 单条剪贴板记录的最大大小（10MB）
const MAX_SINGLE_ITEM_SIZE: usize = 10 * 1024 * 1024;
/// 剪贴板历史记录的最大总内存（50MB）
const MAX_TOTAL_MEMORY: usize = 50 * 1024 * 1024;

use app_config::{AppConfig, CloseAction, TypingFormat};
use arboard::Clipboard;
use chrono::Local;
use eframe::egui;
use enigo::{Enigo, Keyboard, Settings};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use hotkey_config::{HotkeyConfig, KeyCode};
use i18n::I18n;
use log::{debug, error, info, warn};
use permissions::{check_permissions, get_permission_fix_instructions, PermissionStatus};
use rand::Rng;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard,
};
use std::thread;
use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// 主面板预览最多渲染的字符数（大文本不整段排版，避免卡死 UI）
const PREVIEW_MAX_CHARS: usize = 2000;
/// 历史记录列表每项预览的字符数
const HISTORY_PREVIEW_CHARS: usize = 300;
/// 日志文件超过该大小时轮转为 .old（2MB）
const LOG_FILE_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// 托盘菜单项 ID
const MENU_SHOW: &str = "show";
const MENU_TOGGLE: &str = "toggle";
const MENU_EXIT: &str = "exit";

/// 获取 Mutex 锁；锁被毒化（持有者 panic）时恢复数据而不是级联 panic。
/// 本程序锁内均为纯数据，恢复访问比重启整个进程更安全。
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
struct HistoryItem {
    text: String,
    /// 列表展示用的截断预览，入栈时算好，避免每帧对全量文本排版
    preview: String,
    copied_at: String,
}

impl HistoryItem {
    fn new(text: String) -> Self {
        Self {
            preview: char_preview(&text, HISTORY_PREVIEW_CHARS),
            text,
            copied_at: format_history_timestamp(),
        }
    }
}

/// 共享应用状态
#[derive(Clone)]
struct SharedState {
    /// 当前保存的剪贴板文本（Arc<str>：UI 每帧取用时只增加引用计数，不复制内容）
    clipboard_text: Arc<Mutex<Arc<str>>>,
    /// 上一次的剪贴板文本（用于检测变化；Windows 上主要靠剪贴板序号检测）
    last_clipboard_text: Arc<Mutex<String>>,
    /// 剪贴板历史记录（队首为最旧记录）
    clipboard_history: Arc<Mutex<VecDeque<HistoryItem>>>,
    /// 剪贴板历史记录占用的总内存（字节）
    history_memory_used: Arc<Mutex<usize>>,
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
    /// 模拟输入时的延迟 (毫秒)
    typing_delay: Arc<Mutex<u64>>,
    /// 模拟输入时的随机偏差 (毫秒)，为 0 时不抖动
    typing_variance: Arc<Mutex<u64>>,
    /// 模拟输入时对文本格式的处理方式
    typing_format: Arc<Mutex<TypingFormat>>,
    /// 输入是否暂停
    typing_paused: Arc<Mutex<bool>>,
    /// 请求取消当前输入
    typing_cancelled: Arc<AtomicBool>,
    /// 输入进度（已完成字符数, 总字符数）
    typing_progress: Arc<Mutex<(usize, usize)>>,
    /// 最近一次快捷键触发时间
    last_hotkey_trigger: Arc<Mutex<Option<Instant>>>,
    /// 当前快捷键 ID
    hotkey_id: Arc<Mutex<Option<u32>>>,
    /// 后台线程状态变化时用于唤醒 UI 重绘的上下文
    repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    /// 语言资源
    i18n: I18n,
}

impl SharedState {
    fn new(i18n: I18n) -> Self {
        let ready = i18n.t("status.ready");
        Self {
            clipboard_text: Arc::new(Mutex::new(Arc::from(""))),
            last_clipboard_text: Arc::new(Mutex::new(String::new())),
            clipboard_history: Arc::new(Mutex::new(VecDeque::new())),
            history_memory_used: Arc::new(Mutex::new(0)),
            history_enabled: Arc::new(Mutex::new(false)),
            history_max_items: Arc::new(Mutex::new(0)),
            is_typing: Arc::new(Mutex::new(false)),
            enabled: Arc::new(Mutex::new(true)),
            status_message: Arc::new(Mutex::new(ready)),
            request_exit: Arc::new(AtomicBool::new(false)),
            typing_delay: Arc::new(Mutex::new(0)),
            typing_variance: Arc::new(Mutex::new(0)),
            typing_format: Arc::new(Mutex::new(TypingFormat::Raw)),
            typing_paused: Arc::new(Mutex::new(false)),
            typing_cancelled: Arc::new(AtomicBool::new(false)),
            typing_progress: Arc::new(Mutex::new((0, 0))),
            last_hotkey_trigger: Arc::new(Mutex::new(None)),
            hotkey_id: Arc::new(Mutex::new(None)),
            repaint_ctx: Arc::new(Mutex::new(None)),
            i18n,
        }
    }

    /// 记录 egui 上下文，使后台线程的状态变更能立即触发重绘，
    /// 代替原先固定的 50ms 轮询重绘。
    fn set_repaint_ctx(&self, ctx: egui::Context) {
        *lock(&self.repaint_ctx) = Some(ctx);
    }

    fn request_repaint(&self) {
        if let Some(ctx) = lock(&self.repaint_ctx).as_ref() {
            ctx.request_repaint();
        }
    }

    fn set_status(&self, msg: &str) {
        *lock(&self.status_message) = msg.to_string();
        self.request_repaint();
    }

    fn get_status(&self) -> String {
        lock(&self.status_message).clone()
    }

    fn is_enabled(&self) -> bool {
        *lock(&self.enabled)
    }

    fn set_enabled(&self, enabled: bool) {
        *lock(&self.enabled) = enabled;
        if !enabled {
            // 禁用程序即停止正在进行的输入
            self.typing_cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// 返回暂存文本的引用（零拷贝，供 UI 每帧调用）
    fn get_clipboard_text(&self) -> Arc<str> {
        lock(&self.clipboard_text).clone()
    }

    fn is_typing(&self) -> bool {
        *lock(&self.is_typing)
    }

    fn cancel_typing(&self) {
        self.typing_cancelled.store(true, Ordering::SeqCst);
        self.request_repaint();
    }

    fn toggle_typing_pause(&self) -> bool {
        let mut paused = lock(&self.typing_paused);
        *paused = !*paused;
        *paused
    }

    /// 暂停期间自旋等待；返回 false 表示期间收到取消请求，调用方应中止输入。
    fn wait_if_paused(&self) -> bool {
        while *lock(&self.typing_paused) {
            if self.typing_cancelled.load(Ordering::SeqCst) {
                return false;
            }
            thread::sleep(Duration::from_millis(50));
        }
        !self.typing_cancelled.load(Ordering::SeqCst)
    }

    fn should_handle_hotkey(&self) -> bool {
        let mut last = lock(&self.last_hotkey_trigger);
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

    fn tr(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.i18n.tr(key, args)
    }

    /// 将新文本设为待输入内容并写入历史记录（监控线程与输入线程共用入口）
    fn stage_text(&self, text: String) {
        let len_str = text.len().to_string();
        info!(
            "{}",
            self.tr("log.clipboard_changed", &[("len", len_str.as_str())])
        );

        // truncate_text 已按字符边界安全截断并转义换行
        let preview = truncate_text(&text, 50);
        debug!(
            "{}",
            self.tr("log.clipboard_preview", &[("preview", preview.as_str())])
        );

        *lock(&self.clipboard_text) = Arc::from(text.as_str());
        *lock(&self.last_clipboard_text) = text.clone();
        self.record_history(text);
        self.request_repaint();
    }

    /// 从历史记录载入为待输入文本（不重复写入历史）
    fn stage_from_history(&self, text: String) {
        *lock(&self.clipboard_text) = Arc::from(text.as_str());
        self.request_repaint();
    }

    fn record_history(&self, text: String) {
        if !*lock(&self.history_enabled) {
            return;
        }
        let max_items = *lock(&self.history_max_items);
        if max_items == 0 {
            return;
        }

        // 计算文本大小（字节）
        let text_size = text.len();

        // 如果单条文本超过10MB，则不存储
        if text_size > MAX_SINGLE_ITEM_SIZE {
            warn!(
                "{}",
                self.tr(
                    "log.item_too_large",
                    &[
                        ("size", &format!("{:.2}MB", text_size as f64 / 1024.0 / 1024.0)),
                        ("max", &format!("{:.2}MB", MAX_SINGLE_ITEM_SIZE as f64 / 1024.0 / 1024.0))
                    ]
                )
            );
            return;
        }

        let mut history = lock(&self.clipboard_history);
        let mut memory_used = lock(&self.history_memory_used);

        // 连续重复复制相同内容不重复入栈，仅刷新最新记录的时间戳
        if let Some(back) = history.back_mut() {
            if back.text == text {
                back.copied_at = format_history_timestamp();
                return;
            }
        }

        // 如果新增后总内存超过50MB，删除最旧的记录直到能够放下
        while *memory_used + text_size > MAX_TOTAL_MEMORY && !history.is_empty() {
            if let Some(removed) = history.pop_front() {
                let removed_size = removed.text.len();
                *memory_used = memory_used.saturating_sub(removed_size);
                debug!(
                    "{}",
                    self.tr(
                        "log.removed_old_item",
                        &[
                            ("size", &format!("{:.2}KB", removed_size as f64 / 1024.0)),
                            ("remaining", &format!("{:.2}MB", *memory_used as f64 / 1024.0 / 1024.0))
                        ]
                    )
                );
            }
        }

        // 添加新记录
        history.push_back(HistoryItem::new(text));
        *memory_used += text_size;

        // 检查是否超出条数限制
        while history.len() > max_items as usize {
            if let Some(item) = history.pop_front() {
                *memory_used = memory_used.saturating_sub(item.text.len());
            }
        }

        debug!(
            "{}",
            self.tr(
                "log.history_stats",
                &[
                    ("count", &history.len().to_string()),
                    ("memory", &format!("{:.2}MB", *memory_used as f64 / 1024.0 / 1024.0))
                ]
            )
        );

        #[cfg(debug_assertions)]
        Self::assert_history_memory_sync(&history, *memory_used);
    }

    fn clear_history(&self) {
        let mut history = lock(&self.clipboard_history);
        let mut memory_used = lock(&self.history_memory_used);
        history.clear();
        *memory_used = 0;

        #[cfg(debug_assertions)]
        Self::assert_history_memory_sync(&history, *memory_used);
    }

    fn trim_history(&self) {
        let max_items = *lock(&self.history_max_items);
        if max_items == 0 {
            self.clear_history();
            return;
        }
        let mut history = lock(&self.clipboard_history);
        let mut memory_used = lock(&self.history_memory_used);
        while history.len() > max_items as usize {
            if let Some(item) = history.pop_front() {
                *memory_used = memory_used.saturating_sub(item.text.len());
            }
        }

        #[cfg(debug_assertions)]
        Self::assert_history_memory_sync(&history, *memory_used);
    }

    #[cfg(debug_assertions)]
    fn assert_history_memory_sync(history: &VecDeque<HistoryItem>, memory_used: usize) {
        let computed: usize = history.iter().map(|item| item.text.len()).sum();
        debug_assert_eq!(
            memory_used,
            computed,
            "history_memory_used out of sync: tracked={}, actual={}",
            memory_used,
            computed
        );
    }

    /// 执行模拟输入逻辑
    fn execute_typing(&self) {
        if !self.is_enabled() {
            warn!("{}", self.t("log.request_ignored_disabled"));
            return;
        }

        // 检查是否正在输入
        {
            let mut typing = lock(&self.is_typing);
            if *typing {
                warn!("{}", self.t("log.request_ignored_typing"));
                return;
            }
            *typing = true;
        }

        *lock(&self.typing_paused) = false;
        self.typing_cancelled.store(false, Ordering::SeqCst);
        self.set_status(&self.t("status.typing"));
        let state = self.clone();
        let delay = *lock(&self.typing_delay);
        let variance = *lock(&self.typing_variance);
        let format = *lock(&self.typing_format);

        thread::spawn(move || {
            // 无论线程如何退出（含 panic），都复位输入相关标志，
            // 避免 is_typing 卡死导致重启前无法再输入。
            let _guard = TypingGuard::new(state.clone());

            // 延迟输入，防止还未松开快捷键
            thread::sleep(Duration::from_millis(250));

            // 优先实时读取系统剪贴板：若禁用期间复制了新内容、
            // 或监控线程还没来得及轮询，也能输入最新文本。
            // 读取失败/为空则回退到监控线程暂存的内容。
            let raw = match Clipboard::new().and_then(|mut cb| cb.get_text()) {
                Ok(t) if !t.is_empty() => {
                    // 同步暂存状态与历史，保证预览与实际输入一致
                    state.stage_text(t.clone());
                    t
                }
                _ => state.get_clipboard_text().to_string(),
            };

            // 按所选格式模式预处理后再输入
            let text = format.apply(&raw);

            if text.is_empty() {
                warn!("{}", state.t("log.clipboard_empty"));
                state.set_status(&state.t("status.clipboard_empty"));
                return;
            }

            let total = text.chars().count();
            *lock(&state.typing_progress) = (0, total);
            state.request_repaint();

            let len_str = text.len().to_string();
            let delay_str = delay.to_string();
            let variance_str = variance.to_string();

            info!(
                "{}",
                state.tr(
                    "log.input_start",
                    &[
                        ("len", len_str.as_str()),
                        ("delay", delay_str.as_str()),
                        ("variance", variance_str.as_str())
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
                    return;
                }
            };

            let mut result = Ok(());
            let mut rng = rand::thread_rng();
            // 复用缓冲区，避免每个字符都分配 String
            let mut buf = [0u8; 4];
            let mut done = 0usize;

            'typing: for c in text.chars() {
                if state.typing_cancelled.load(Ordering::SeqCst) || !state.wait_if_paused() {
                    break;
                }
                if let Err(e) = enigo.text(c.encode_utf8(&mut buf)) {
                    result = Err(e);
                    break;
                }
                done += 1;
                *lock(&state.typing_progress) = (done, total);

                // 计算实际延迟：在 [delay, delay + variance] 之间随机
                let mut actual_delay = delay;
                if variance > 0 {
                    actual_delay += rng.gen_range(0..=variance);
                }

                let mut remaining = actual_delay;
                while remaining > 0 {
                    if state.typing_cancelled.load(Ordering::SeqCst) {
                        break 'typing;
                    }
                    if !state.wait_if_paused() {
                        break 'typing;
                    }
                    let step = remaining.min(50);
                    thread::sleep(Duration::from_millis(step));
                    remaining -= step;
                }
            }

            if state.typing_cancelled.load(Ordering::SeqCst) {
                let done_str = done.to_string();
                let total_str = total.to_string();
                info!(
                    "{}",
                    state.tr(
                        "log.input_cancelled",
                        &[("done", done_str.as_str()), ("total", total_str.as_str())]
                    )
                );
                state.set_status(&state.t("status.typing_cancelled"));
            } else if let Err(e) = result {
                let err = e.to_string();
                error!("{}", state.tr("log.input_error", &[("err", err.as_str())]));
                state.set_status(&state.tr("status.input_error", &[("err", err.as_str())]));
            } else {
                info!("{}", state.t("log.input_complete"));
                state.set_status(&state.t("status.input_complete"));
            }
        });
    }
}

/// 输入线程作用域守卫：Drop 时复位所有输入标志，保证线程以任何方式
/// （正常结束/提前 return/panic）退出后状态一致，不会永久卡在“输入中”。
struct TypingGuard {
    state: SharedState,
}

impl TypingGuard {
    fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl Drop for TypingGuard {
    fn drop(&mut self) {
        *lock(&self.state.typing_paused) = false;
        *lock(&self.state.is_typing) = false;
        self.state.typing_cancelled.store(false, Ordering::SeqCst);
        *lock(&self.state.typing_progress) = (0, 0);
        self.state.request_repaint();
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
    /// 快捷键注册错误信息
    hotkey_register_error: Option<String>,
    /// 显示启动时快捷键错误弹窗
    show_startup_hotkey_error: bool,
    /// 启动时快捷键错误信息
    startup_hotkey_error: Option<String>,
    /// 权限状态
    permission_status: PermissionStatus,
    /// 系统托盘上下文，必须保持活跃
    tray_context: Option<TrayContext>,
    /// 托盘是否实际可用（Linux 上由 GTK 线程异步置位）。
    /// 用于关闭/最小化时避免“窗口隐藏后没有托盘可恢复”。
    tray_available: Arc<AtomicBool>,
    /// 启动时最小化的待处理标记（Linux：等待托盘就绪后再隐藏）
    #[cfg(target_os = "linux")]
    pending_minimize: bool,
    /// 启动最小化的等待帧数预算（Linux）
    #[cfg(target_os = "linux")]
    startup_minimize_frames: u32,
    /// 预览缓存对应的暂存文本（Arc 指针判等），内容/格式变化时重建缓存
    preview_source: Option<Arc<str>>,
    /// 预览缓存对应的格式模式
    preview_format: TypingFormat,
    /// 截断后的预览文本（经 typing_format 变换，与实际输入一致）
    preview_text: String,
    /// 待输入文本统计（字符数, 行数），随预览缓存一起更新
    preview_stats: (usize, usize),
}

/// 保持托盘及其菜单项存活的结构体
struct TrayContext {
    tray: TrayIcon,
    show_item: MenuItem,
    toggle_item: MenuItem,
    exit_item: MenuItem,
    #[allow(dead_code)]
    separator: PredefinedMenuItem,
}

impl CopyTypeApp {
    fn new(cc: &eframe::CreationContext<'_>, icon: Option<tray_icon::Icon>) -> Self {
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
        state.set_repaint_ctx(cc.egui_ctx.clone());
        // 初始化 state 中的配置值
        *lock(&state.typing_delay) = app_config.typing_delay;
        *lock(&state.typing_variance) = app_config.typing_variance;
        *lock(&state.typing_format) = app_config.typing_format;
        *lock(&state.history_enabled) = app_config.history_enabled;
        *lock(&state.history_max_items) = app_config.history_max_items;

        // 根据配置显示/隐藏控制台
        #[cfg(target_os = "windows")]
        {
            if app_config.show_console {
                show_console_window();
            } else {
                hide_console_window();
            }
        }

        // 托盘是否可用（关闭/最小化逻辑依赖它，避免窗口隐藏后无法恢复）
        let tray_available = Arc::new(AtomicBool::new(false));

        // 创建系统托盘
        // Windows/macOS：在主线程创建（主线程已有 win32 / NSApp 事件循环）。
        #[cfg(not(target_os = "linux"))]
        let tray_context = if let Some(icon) = icon {
            let tray_ctx = create_tray_context(&i18n, icon);
            tray_available.store(tray_ctx.is_some(), Ordering::SeqCst);
            tray_ctx
        } else {
            warn!("Tray icon unavailable; skipping tray menu.");
            None
        };

        // Linux：tray-icon/muda 基于 GTK，必须在“已初始化 GTK 且有 GTK 事件循环”的
        // 线程上创建并运行；而 eframe 用的是 winit（无 GTK 循环），因此这里专门起一个
        // GTK 线程承载托盘（见 spawn_linux_tray）。
        #[cfg(target_os = "linux")]
        let tray_context: Option<TrayContext> = {
            let _ = icon; // 托盘图标在 GTK 线程内自行构建
            spawn_linux_tray(i18n.clone(), tray_available.clone());
            None
        };
        
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
                    let current_id = *lock(&hotkey_state.hotkey_id);
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
            current_hotkey: None,
            hotkey_config: hotkey_config.clone(),
            temp_hotkey_config: hotkey_config,
            app_config: app_config.clone(),
            temp_app_config: app_config.clone(),
            show_hotkey_settings: false,
            show_app_settings: false,
            show_permission_warning,
            hotkey_register_error: None,
            show_startup_hotkey_error: false,
            startup_hotkey_error: None,
            permission_status,
            tray_context,
            tray_available,
            #[cfg(target_os = "linux")]
            pending_minimize: false,
            #[cfg(target_os = "linux")]
            startup_minimize_frames: 0,
            preview_source: None,
            preview_format: TypingFormat::Raw,
            preview_text: String::new(),
            preview_stats: (0, 0),
        };

        // 初始化快捷键
        app.init_hotkey();

        // 启动剪贴板监控
        app.start_clipboard_monitor();

        // 启动时最小化：仅当托盘可用时才隐藏窗口，否则会“隐藏后无法恢复”。
        if app_config.start_minimized {
            #[cfg(not(target_os = "linux"))]
            {
                if app.tray_available.load(Ordering::SeqCst) {
                    cc.egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::Visible(false));
                } else {
                    warn!("Tray unavailable; ignoring start-minimized to keep the window reachable.");
                }
            }
            // Linux：托盘由 GTK 线程异步创建，推迟到 update() 确认托盘就绪后再隐藏。
            #[cfg(target_os = "linux")]
            {
                app.pending_minimize = true;
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
                            self.current_hotkey = Some(hotkey);
                            *lock(&self.state.hotkey_id) = Some(hotkey.id());
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
                            // 保存用户友好的错误信息
                            let friendly_error = if err.contains("already register") {
                                self.i18n.t("ui.error_hotkey_already_registered")
                            } else {
                                self.i18n.tr("ui.error_hotkey_register_failed", &[("error", err.as_str())])
                            };
                            self.startup_hotkey_error =
                                Some(format!("{} - {}", self.hotkey_config.display(), friendly_error));
                            self.show_startup_hotkey_error = true;
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
                // 保存用户友好的错误信息
                self.startup_hotkey_error = Some(self.i18n.t("ui.error_hotkey_manager_init_failed"));
                self.show_startup_hotkey_error = true;
            }
        }
    }

    /// 更新快捷键
    fn update_hotkey(&mut self) {
        // 先尝试注册新的快捷键（不注销旧的）
        if let Some(manager) = &self.hotkey_manager {
            if let Some(new_hotkey) = self.temp_hotkey_config.to_global_hotkey() {
                if let Some(current_hotkey) = self.current_hotkey {
                    if current_hotkey == new_hotkey {
                        self.hotkey_register_error = None;
                        return;
                    }
                }
                match manager.register(new_hotkey) {
                    Ok(()) => {
                        // 注册成功，现在注销旧的快捷键
                        if let Some(old_hotkey) = self.current_hotkey {
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
                        }

                        // 更新配置
                        self.hotkey_config = self.temp_hotkey_config.clone();
                        self.current_hotkey = Some(new_hotkey);
                        *lock(&self.state.hotkey_id) = Some(new_hotkey.id());
                        
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

                        // 清除错误信息
                        self.hotkey_register_error = None;
                    }
                    Err(e) => {
                        // 注册失败，保存错误信息
                        let err = e.to_string();
                        error!(
                            "{}",
                            self.i18n
                                .tr("log.hotkey_register_fail", &[("err", err.as_str())])
                        );
                        // 保存用户友好的错误信息
                        let friendly_error = if err.contains("already register") {
                            self.i18n.t("ui.error_hotkey_already_registered")
                        } else {
                            err
                        };
                        self.hotkey_register_error = Some(friendly_error);
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

            // Windows：剪贴板序号是系统级计数器，每次剪贴板变化（包括
            // 重复复制相同内容）都会递增，比逐次取文本比对更省也更准确。
            #[cfg(target_os = "windows")]
            let mut last_seq = clipboard_sequence_number();

            loop {
                // 只在启用时监控
                if state.is_enabled() {
                    #[cfg(target_os = "windows")]
                    {
                        let seq = clipboard_sequence_number();
                        if seq != last_seq {
                            match clipboard.get_text() {
                                Ok(text) if !text.is_empty() => {
                                    last_seq = seq;
                                    state.stage_text(text);
                                }
                                // 读取失败（如被其他程序占用）或当前不是文本：
                                // 不消费序号，下个周期重试
                                _ => {}
                            }
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        if let Ok(text) = clipboard.get_text() {
                            let last = lock(&state.last_clipboard_text).clone();
                            if text != last && !text.is_empty() {
                                state.stage_text(text);
                            }
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

}

impl eframe::App for CopyTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let i18n = self.i18n.clone();

        // 重绘策略：不做固定频率轮询。后台线程（剪贴板监控/输入/托盘事件）
        // 在状态变化时通过 SharedState::request_repaint 唤醒 UI；
        // 输入中的 spinner 动画也会自行请求重绘。

        // Linux：等待托盘就绪后再执行“启动最小化”，避免托盘还没建好就把窗口藏了导致无法恢复。
        #[cfg(target_os = "linux")]
        if self.pending_minimize {
            // 仅在等待托盘期间才周期重绘
            ctx.request_repaint_after(Duration::from_millis(50));
            self.startup_minimize_frames += 1;
            if self.tray_available.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.pending_minimize = false;
            } else if self.startup_minimize_frames > 40 {
                warn!("Tray did not become available; keeping the window visible.");
                self.pending_minimize = false;
            }
        }

        // 待输入文本预览缓存：仅在文本或格式模式变化时重建，
        // 避免每帧对（可能达 10MB 的）全文做格式化和排版。
        let clipboard_text = self.state.get_clipboard_text();
        let typing_format = *lock(&self.state.typing_format);
        let preview_stale = match &self.preview_source {
            Some(src) => !Arc::ptr_eq(src, &clipboard_text),
            None => true,
        } || self.preview_format != typing_format;
        if preview_stale {
            let formatted = typing_format.apply(&clipboard_text);
            self.preview_stats = (formatted.chars().count(), formatted.lines().count());
            self.preview_text = char_preview(&formatted, PREVIEW_MAX_CHARS);
            self.preview_source = Some(clipboard_text.clone());
            self.preview_format = typing_format;
        }

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

        // 启动时快捷键错误警告窗口
        if self.show_startup_hotkey_error {
            egui::Window::new(i18n.t("ui.title_hotkey_error"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(i18n.t("ui.label_hotkey_conflict_startup"));
                    ui.add_space(10.0);

                    if let Some(error) = &self.startup_hotkey_error {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            error
                        );
                    }

                    ui.add_space(10.0);
                    ui.label(i18n.t("ui.label_hotkey_conflict_suggestion"));
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button(i18n.t("ui.button_open_settings")).clicked() {
                            self.show_startup_hotkey_error = false;
                            self.show_hotkey_settings = true;
                            self.temp_hotkey_config = self.hotkey_config.clone();
                        }
                        if ui.button(i18n.t("ui.button_acknowledge")).clicked() {
                            self.show_startup_hotkey_error = false;
                        }
                    });
                });
        }

        // 顶部菜单栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(i18n.t("ui.menu_file"), |ui| {
                    if ui.button(i18n.t("ui.menu_minimize_to_tray")).clicked() {
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
                        let (done, total) = *lock(&self.state.typing_progress);
                        if total > 0 {
                            let done_str = done.to_string();
                            let total_str = total.to_string();
                            ui.label(i18n.tr(
                                "status.typing_progress",
                                &[("done", done_str.as_str()), ("total", total_str.as_str())],
                            ));
                        }
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

            // 剪贴板内容预览（clipboard_text/preview_text 均来自上方缓存逻辑）
            let history_enabled = *lock(&self.state.history_enabled);

            if history_enabled {
                ui.label(i18n.t("ui.label_history_list"));
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        let history = lock(&self.state.clipboard_history);
                        if history.is_empty() {
                            ui.label(egui::RichText::new(i18n.t("ui.label_empty")).italics().weak());
                        } else {
                            let history_len = history.len();
                            let mut load_item: Option<String> = None;
                            for (index, item) in history.iter().rev().enumerate() {
                                egui::Frame::none()
                                    .fill(ui.style().visuals.extreme_bg_color)
                                    .inner_margin(8.0)
                                    .rounding(4.0)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        let time_label = i18n.tr(
                                            "ui.label_copied_time",
                                            &[("time", item.copied_at.as_str())],
                                        );
                                        ui.label(egui::RichText::new(time_label).small().weak());
                                        let resp = ui
                                            .add(
                                                egui::Label::new(&item.preview)
                                                    .sense(egui::Sense::click()),
                                            )
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .on_hover_text(i18n.t("ui.history_click_hint"));
                                        if resp.clicked() {
                                            load_item = Some(item.text.clone());
                                        }
                                    });
                                if index + 1 < history_len {
                                    ui.add_space(6.0);
                                }
                            }
                            if let Some(text) = load_item {
                                self.state.stage_from_history(text);
                                self.state.set_status(&i18n.t("status.loaded_from_history"));
                            }
                        }
                    });
            } else {
                ui.label(i18n.t("ui.label_waiting_text"));
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
                                    ui.label(&self.preview_text);
                                }
                            });
                    });
            }

            ui.add_space(10.0);

            // 文本信息（统计的是经格式处理后的待输入文本）
            if !clipboard_text.is_empty() {
                ui.horizontal(|ui| {
                    let char_count = self.preview_stats.0.to_string();
                    let line_count = self.preview_stats.1.to_string();
                    ui.label(i18n.tr("ui.label_char_count", &[("count", char_count.as_str())]));
                    ui.label(i18n.tr("ui.label_line_count", &[("count", line_count.as_str())]));
                });
            }

            ui.add_space(10.0);

            // 手动触发 / 停止 / 清空按钮
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

                if typing && ui.button(i18n.t("ui.button_stop")).clicked() {
                    self.state.cancel_typing();
                }

                if ui.button(i18n.t("ui.button_clear")).clicked() {
                    *lock(&self.state.clipboard_text) = Arc::from("");
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

                    // 验证快捷键
                    let is_valid = self.temp_hotkey_config.is_valid();
                    let is_same = self.temp_hotkey_config.conflicts_with(&self.hotkey_config);
                    let can_save = is_valid && !is_same;

                    // 显示警告
                    if !is_valid {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            format!("⚠ {}", i18n.t("ui.error_no_modifier_key"))
                        );
                        ui.add_space(10.0);
                    } else if is_same {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 165, 0),
                            format!("⚠ {}", i18n.t("ui.warning_same_hotkey"))
                        );
                        ui.add_space(10.0);
                    }

                    // 显示注册错误（如果有）
                    if let Some(error) = &self.hotkey_register_error {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            format!("⚠ {}: {}", i18n.t("ui.error_hotkey_conflict"), error)
                        );
                        ui.add_space(10.0);
                    }

                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        // 如果无效或相同，禁用保存按钮
                        ui.add_enabled_ui(can_save, |ui| {
                            if ui.button(i18n.t("ui.button_save")).clicked() {
                                self.update_hotkey();
                                // 只有在没有错误时才关闭窗口
                                if self.hotkey_register_error.is_none() {
                                    self.show_hotkey_settings = false;
                                }
                            }
                        });
                        if ui.button(i18n.t("ui.button_cancel")).clicked() {
                            self.hotkey_register_error = None;
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

                        ui.horizontal(|ui| {
                            ui.label(i18n.t("ui.app.label_typing_format"));
                            let selected_text = match self.temp_app_config.typing_format {
                                TypingFormat::Raw => i18n.t("ui.app.format_raw"),
                                TypingFormat::StripIndent => i18n.t("ui.app.format_strip_indent"),
                                TypingFormat::SingleLine => i18n.t("ui.app.format_single_line"),
                            };
                            egui::ComboBox::from_id_salt("typing_format")
                                .selected_text(selected_text)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.temp_app_config.typing_format,
                                        TypingFormat::Raw,
                                        i18n.t("ui.app.format_raw"),
                                    );
                                    ui.selectable_value(
                                        &mut self.temp_app_config.typing_format,
                                        TypingFormat::StripIndent,
                                        i18n.t("ui.app.format_strip_indent"),
                                    );
                                    ui.selectable_value(
                                        &mut self.temp_app_config.typing_format,
                                        TypingFormat::SingleLine,
                                        i18n.t("ui.app.format_single_line"),
                                    );
                                });
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
                            *lock(&self.state.typing_delay) = self.app_config.typing_delay;
                            *lock(&self.state.typing_variance) = self.app_config.typing_variance;
                            *lock(&self.state.typing_format) = self.app_config.typing_format;
                            *lock(&self.state.history_enabled) = self.app_config.history_enabled;
                            *lock(&self.state.history_max_items) = self.app_config.history_max_items;
                            if self.app_config.history_enabled {
                                self.state.trim_history();
                            } else {
                                self.state.clear_history();
                            }
                            self.i18n.set_language(&self.app_config.language);

                            // 托盘菜单文本随界面语言刷新（Linux 上托盘在 GTK
                            // 线程内独立持有，tray_context 为 None，自动跳过）
                            if let Some(tray) = &self.tray_context {
                                tray.show_item.set_text(self.i18n.t("tray.menu_show"));
                                tray.toggle_item.set_text(self.i18n.t("tray.menu_toggle"));
                                tray.exit_item.set_text(self.i18n.t("tray.menu_exit"));
                                let _ = tray.tray.set_tooltip(Some(self.i18n.t("tray.tooltip")));
                            }

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
        if ctx.input(|i| i.viewport().close_requested())
            && !self.state.request_exit.load(Ordering::SeqCst)
        {
            // 仅当托盘可用时才“最小化到托盘”，否则直接退出，
            // 避免在托盘不可用（如 Linux 无 GTK 托盘）时窗口隐藏后无法恢复。
            let minimize = matches!(self.app_config.close_action, CloseAction::MinimizeToTray)
                && self.tray_available.load(Ordering::SeqCst);
            if minimize {
                // 取消关闭，改为隐藏
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                info!("{}", i18n.t("log.window_minimized_to_tray"));
            } else {
                // 允许关闭（用户选择退出，或托盘不可用时的安全回退）
                info!("{}", i18n.t("log.app_exit"));
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

    // 在 Linux 上尝试一系列常见的 CJK 字体路径（不同发行版位置不同）。
    // egui 内置字体不含 CJK 字形，默认语言又是中文，找不到字体会显示成“豆腐块”。
    #[cfg(target_os = "linux")]
    {
        let font_paths = [
            // Debian / Ubuntu
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
            // Arch / 通用
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJKsc-Regular.otf",
            // Fedora
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJKsc-Regular.otf",
            // Adobe Source Han Sans
            "/usr/share/fonts/adobe-source-han-sans/SourceHanSansSC-Regular.otf",
            "/usr/share/fonts/source-han-sans/SourceHanSansSC-Regular.otf",
            // 文泉驿
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            // Android / Droid fallback
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ];

        let mut loaded = false;
        for path in &font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "cjk".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(font_data)),
                );

                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "cjk".to_owned());

                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "cjk".to_owned());

                loaded = true;
                break;
            }
        }

        if !loaded {
            warn!(
                "No CJK font found; non-Latin (e.g. Chinese) text may render as boxes. \
                 Install Noto Sans CJK / Source Han Sans / WenQuanYi, or switch the UI language to English."
            );
        }
    }

    ctx.set_fonts(fonts);
}

/// Windows: 显示控制台窗口
#[cfg(target_os = "windows")]
fn show_console_window() {
    use windows::core::w;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AllocConsole, GetConsoleWindow, SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};

    unsafe {
        // GUI 子系统程序启动时没有控制台，先分配一个
        let _ = AllocConsole();

        // 关键：把进程的标准输出/错误句柄重新指向新控制台，
        // 否则 env_logger 写入 stderr 的日志不会显示在新分配的控制台里。
        if let Ok(handle) = CreateFileW(
            w!("CONOUT$"),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        ) {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
            let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
        }

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
fn create_tray_context(i18n: &I18n, icon: tray_icon::Icon) -> Option<TrayContext> {
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

#[cfg(not(target_os = "linux"))]
fn build_icon_from_rgba(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Option<(tray_icon::Icon, egui::IconData)> {
    match tray_icon::Icon::from_rgba(rgba.clone(), width, height) {
        Ok(tray_icon) => Some((
            tray_icon,
            egui::IconData {
                rgba,
                width,
                height,
            },
        )),
        Err(e) => {
            warn!("Failed to create tray icon: {}", e);
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn fallback_icon() -> Option<(tray_icon::Icon, egui::IconData)> {
    const FALLBACK_ICON_SIZE: u32 = 32;
    let rgba = vec![0u8; (FALLBACK_ICON_SIZE * FALLBACK_ICON_SIZE * 4) as usize];
    build_icon_from_rgba(rgba, FALLBACK_ICON_SIZE, FALLBACK_ICON_SIZE)
}

/// 加载应用图标（Windows / macOS：同时构建托盘图标与窗口图标）
#[cfg(not(target_os = "linux"))]
fn load_icon() -> (Option<tray_icon::Icon>, Option<egui::IconData>) {
    let icon_data = include_bytes!("logo.png");

    let icons = match image::load_from_memory(icon_data) {
        Ok(image) => {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            build_icon_from_rgba(rgba, width, height).or_else(fallback_icon)
        }
        Err(e) => {
            warn!("Failed to load icon data: {}", e);
            fallback_icon()
        }
    };

    if icons.is_none() {
        warn!("Unable to create any icon data; continuing without icons.");
    }

    icons
        .map(|(tray_icon, window_icon)| (Some(tray_icon), Some(window_icon)))
        .unwrap_or((None, None))
}

/// Linux：解码内嵌 logo 为原始 RGBA 像素（纯解码，不依赖 GTK）
#[cfg(target_os = "linux")]
fn load_icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let icon_data = include_bytes!("logo.png");
    match image::load_from_memory(icon_data) {
        Ok(image) => {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            Some((image.into_raw(), width, height))
        }
        Err(e) => {
            warn!("Failed to load icon data: {}", e);
            None
        }
    }
}

/// Linux：仅构建窗口图标（托盘图标在 GTK 线程内单独构建）
#[cfg(target_os = "linux")]
fn load_window_icon() -> Option<egui::IconData> {
    load_icon_rgba().map(|(rgba, width, height)| egui::IconData {
        rgba,
        width,
        height,
    })
}

/// Linux：在专用 GTK 线程上创建并运行系统托盘。
///
/// tray-icon / muda 在 Linux 上基于 GTK，要求在“已初始化 GTK 且持续 pump 事件”的
/// 线程上创建托盘。eframe 使用 winit（无 GTK 循环），因此这里单开一个线程：
/// 先 `gtk::init()`，构建托盘，再用 `gtk::main()` 跑 GTK 主循环（永不返回，从而让
/// 托盘与菜单回调保持存活）。托盘菜单事件仍通过全局 `MenuEvent` 通道分发，由另一个
/// 监控线程处理。`gtk::init()` 失败时优雅跳过托盘（也避免主线程因 GTK 未初始化而崩溃）。
#[cfg(target_os = "linux")]
fn spawn_linux_tray(i18n: I18n, tray_available: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        if let Err(e) = gtk::init() {
            warn!("GTK init failed; system tray disabled this session: {}", e);
            return;
        }

        let icon = load_icon_rgba().and_then(|(rgba, w, h)| {
            match tray_icon::Icon::from_rgba(rgba, w, h) {
                Ok(icon) => Some(icon),
                Err(e) => {
                    warn!("Failed to build tray icon: {}", e);
                    None
                }
            }
        });

        let _tray_context = match icon.and_then(|icon| create_tray_context(&i18n, icon)) {
            Some(ctx) => ctx,
            None => {
                warn!("Failed to create system tray; tray disabled.");
                return;
            }
        };

        tray_available.store(true, Ordering::SeqCst);

        // 运行 GTK 主循环以分发托盘/菜单事件；此调用不会返回，
        // `_tray_context` 因此在该线程内保持存活。
        gtk::main();
    });
}


/// 截断文本用于日志显示（转义换行，按字符边界截断）
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

/// 按字符数截断文本用于 UI 预览（不转义，超出部分以省略号收尾）
fn char_preview(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", preview)
    } else {
        preview
    }
}

/// Windows: 读取系统剪贴板序号，剪贴板内容每次变化（含重复复制相同文本）都会递增
#[cfg(target_os = "windows")]
fn clipboard_sequence_number() -> u32 {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    unsafe { GetClipboardSequenceNumber() }
}

fn format_history_timestamp() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

/// 同时写 stderr 和日志文件的 Writer
struct TeeWriter {
    file: Option<std::fs::File>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write(buf);
        if let Some(f) = &mut self.file {
            let _ = f.write(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(f) = &mut self.file {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// 打开日志文件；超过 LOG_FILE_MAX_BYTES 时轮转为 copy-type.old.log
fn open_log_file() -> Option<std::fs::File> {
    let dir = dirs::config_dir()?.join("copy-type").join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let log_path = dir.join("copy-type.log");
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > LOG_FILE_MAX_BYTES {
            let old_path = dir.join("copy-type.old.log");
            let _ = std::fs::remove_file(&old_path);
            let _ = std::fs::rename(&log_path, &old_path);
        }
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok()
}

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .target(env_logger::Target::Pipe(Box::new(TeeWriter {
            file: open_log_file(),
        })))
        .init();
}

/// 单实例锁：配置目录下对 instance.lock 取 OS 级文件锁，
/// 进程退出（含崩溃）时锁自动释放。返回的 File 需保持存活。
fn acquire_single_instance_lock() -> Option<std::fs::File> {
    let dir = dirs::config_dir()?.join("copy-type");
    std::fs::create_dir_all(&dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))
        .ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(std::fs::TryLockError::WouldBlock) => None,
        // 锁机制不可用时放行，避免误伤正常启动
        Err(_) => Some(file),
    }
}

/// Windows: 弹系统消息框提示已有实例在运行（release 版无控制台，不能靠日志）
#[cfg(target_os = "windows")]
fn show_already_running_message(title: &str, body: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(body),
            &HSTRING::from(title),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn main() -> eframe::Result<()> {
    // 初始化日志（stderr + 文件，文件超过 2MB 自动轮转）
    init_logger();

    let startup_config = AppConfig::load();
    let startup_i18n = I18n::new(&startup_config.language);

    // 单实例检查：第二个实例提示后直接退出
    let _instance_lock = match acquire_single_instance_lock() {
        Some(f) => f,
        None => {
            warn!("{}", startup_i18n.t("log.instance_already_running"));
            #[cfg(target_os = "windows")]
            show_already_running_message(
                &startup_i18n.t("ui.title_main"),
                &startup_i18n.t("ui.instance_already_running"),
            );
            std::process::exit(1);
        }
    };

    info!("=================================");
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

    // 加载图标
    #[cfg(not(target_os = "linux"))]
    let (tray_icon, window_icon) = load_icon();
    // Linux：托盘图标在 GTK 线程里构建，这里只准备窗口图标，
    // 避免在没有 GTK 循环的主线程上触碰托盘图标。
    #[cfg(target_os = "linux")]
    let (tray_icon, window_icon): (Option<tray_icon::Icon>, Option<egui::IconData>) =
        (None, load_window_icon());

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([400.0, 500.0])
        .with_min_inner_size([350.0, 400.0]);
    if let Some(window_icon) = window_icon {
        viewport = viewport.with_icon(window_icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Copy&Type",
        options,
        Box::new(|cc| Ok(Box::new(CopyTypeApp::new(cc, tray_icon)))),
    )
}
