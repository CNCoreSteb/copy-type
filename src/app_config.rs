//! 应用程序配置模块

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::hotkey_config::HotkeyConfig;

/// 关闭窗口时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseAction {
    /// 最小化到系统托盘
    MinimizeToTray,
    /// 直接退出程序
    ExitApp,
}

impl Default for CloseAction {
    fn default() -> Self {
        CloseAction::MinimizeToTray
    }
}

impl CloseAction {
    #[allow(dead_code)]
    pub fn display(&self) -> &'static str {
        match self {
            CloseAction::MinimizeToTray => "最小化到托盘",
            CloseAction::ExitApp => "退出程序",
        }
    }
}

/// 应用程序配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 关闭窗口时的行为
    pub close_action: CloseAction,
    /// 是否开机启动
    pub auto_start: bool,
    /// 是否启动时最小化
    pub start_minimized: bool,
    /// 是否显示调试控制台
    #[serde(default)]
    pub show_console: bool,
    /// 模拟输入时的按键延迟 (毫秒)
    #[serde(default = "default_typing_delay")]
    pub typing_delay: u64,
    /// 模拟输入时的随机偏差 (毫秒)
    #[serde(default = "default_typing_variance")]
    pub typing_variance: u64,
    /// 是否启用随机偏差
    #[serde(default)]
    pub typing_variance_enabled: bool,
    /// 快捷键配置
    #[serde(default)]
    pub hotkey: HotkeyConfig,
}

fn default_typing_delay() -> u64 {
    20 // 默认稍微带点延迟，更像人
}

fn default_typing_variance() -> u64 {
    0
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            close_action: CloseAction::MinimizeToTray,
            auto_start: false,
            start_minimized: false,
            show_console: false,
            typing_delay: default_typing_delay(),
            typing_variance: default_typing_variance(),
            typing_variance_enabled: false,
            hotkey: HotkeyConfig::default(),
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("copy-type").join("config.json"))
    }

    /// 从文件加载配置
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            fs::write(&path, content)?;
        }
        Ok(())
    }
}
