//! 应用程序配置模块

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::hotkey_config::HotkeyConfig;

#[cfg(test)]
mod tests;

/// 关闭窗口时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CloseAction {
    /// 最小化到系统托盘
    #[default]
    MinimizeToTray,
    /// 直接退出程序
    ExitApp,
}

/// 模拟输入时对文本格式的处理方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TypingFormat {
    /// 原样输入，保留换行与行首缩进
    #[default]
    Raw,
    /// 去除每行行首缩进，保留换行（适合有自动缩进的代码编辑器）
    StripIndent,
    /// 合并为单行，换行转空格（适合回车即发送的聊天框/搜索框）
    SingleLine,
}

impl TypingFormat {
    /// 按所选模式预处理待输入文本
    pub fn apply(&self, text: &str) -> String {
        match self {
            TypingFormat::Raw => text.to_string(),
            TypingFormat::StripIndent => text
                .lines()
                .map(|line| line.trim_start())
                .collect::<Vec<_>>()
                .join("\n"),
            TypingFormat::SingleLine => text
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

/// 应用程序配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 关闭窗口时的行为
    pub close_action: CloseAction,
    /// 是否启动时最小化
    pub start_minimized: bool,
    /// 是否显示调试控制台
    #[serde(default)]
    pub show_console: bool,
    /// 模拟输入时的按键延迟 (毫秒)
    #[serde(default = "default_typing_delay")]
    pub typing_delay: u64,
    /// 模拟输入时的随机偏差 (毫秒)，为 0 时不抖动
    #[serde(default = "default_typing_variance")]
    pub typing_variance: u64,
    /// 模拟输入时对文本格式的处理方式
    #[serde(default)]
    pub typing_format: TypingFormat,
    /// 是否保存剪贴板历史
    #[serde(default)]
    pub history_enabled: bool,
    /// 剪贴板历史最多保存条数
    #[serde(default = "default_history_max_items")]
    pub history_max_items: u32,
    /// 快捷键配置
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    /// 界面语言
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_typing_delay() -> u64 {
    20 // 默认稍微带点延迟，更像人
}

fn default_typing_variance() -> u64 {
    0
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_history_max_items() -> u32 {
    20
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            close_action: CloseAction::MinimizeToTray,
            start_minimized: false,
            show_console: false,
            typing_delay: default_typing_delay(),
            typing_variance: default_typing_variance(),
            typing_format: TypingFormat::Raw,
            history_enabled: false,
            history_max_items: default_history_max_items(),
            hotkey: HotkeyConfig::default(),
            language: default_language(),
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
        let mut config = Self::config_path()
            .and_then(|path| fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str::<Self>(&content).ok())
            .unwrap_or_default();
        config.normalize();
        config
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

    fn normalize(&mut self) {
        if self.history_max_items == 0 {
            self.history_max_items = default_history_max_items();
        } else if self.history_max_items > 100 {
            self.history_max_items = 100;
        }
    }
}
