//! 快捷键配置

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};

/// 支持的按键列表
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Space,
    Enter,
    Tab,
    Backquote,
}

impl KeyCode {
    /// 获取所有可用的按键
    pub fn all() -> Vec<KeyCode> {
        vec![
            KeyCode::A,
            KeyCode::B,
            KeyCode::C,
            KeyCode::D,
            KeyCode::E,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::I,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::M,
            KeyCode::N,
            KeyCode::O,
            KeyCode::P,
            KeyCode::Q,
            KeyCode::R,
            KeyCode::S,
            KeyCode::T,
            KeyCode::U,
            KeyCode::V,
            KeyCode::W,
            KeyCode::X,
            KeyCode::Y,
            KeyCode::Z,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::Key0,
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
            KeyCode::Key7,
            KeyCode::Key8,
            KeyCode::Key9,
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Backquote,
        ]
    }

    /// 显示名称
    pub fn display(&self) -> &'static str {
        match self {
            KeyCode::A => "A",
            KeyCode::B => "B",
            KeyCode::C => "C",
            KeyCode::D => "D",
            KeyCode::E => "E",
            KeyCode::F => "F",
            KeyCode::G => "G",
            KeyCode::H => "H",
            KeyCode::I => "I",
            KeyCode::J => "J",
            KeyCode::K => "K",
            KeyCode::L => "L",
            KeyCode::M => "M",
            KeyCode::N => "N",
            KeyCode::O => "O",
            KeyCode::P => "P",
            KeyCode::Q => "Q",
            KeyCode::R => "R",
            KeyCode::S => "S",
            KeyCode::T => "T",
            KeyCode::U => "U",
            KeyCode::V => "V",
            KeyCode::W => "W",
            KeyCode::X => "X",
            KeyCode::Y => "Y",
            KeyCode::Z => "Z",
            KeyCode::F1 => "F1",
            KeyCode::F2 => "F2",
            KeyCode::F3 => "F3",
            KeyCode::F4 => "F4",
            KeyCode::F5 => "F5",
            KeyCode::F6 => "F6",
            KeyCode::F7 => "F7",
            KeyCode::F8 => "F8",
            KeyCode::F9 => "F9",
            KeyCode::F10 => "F10",
            KeyCode::F11 => "F11",
            KeyCode::F12 => "F12",
            KeyCode::Key0 => "0",
            KeyCode::Key1 => "1",
            KeyCode::Key2 => "2",
            KeyCode::Key3 => "3",
            KeyCode::Key4 => "4",
            KeyCode::Key5 => "5",
            KeyCode::Key6 => "6",
            KeyCode::Key7 => "7",
            KeyCode::Key8 => "8",
            KeyCode::Key9 => "9",
            KeyCode::Space => "Space",
            KeyCode::Enter => "Enter",
            KeyCode::Tab => "Tab",
            KeyCode::Backquote => "`",
        }
    }

    /// 转换为 global_hotkey 的 Code
    pub fn to_code(&self) -> Code {
        match self {
            KeyCode::A => Code::KeyA,
            KeyCode::B => Code::KeyB,
            KeyCode::C => Code::KeyC,
            KeyCode::D => Code::KeyD,
            KeyCode::E => Code::KeyE,
            KeyCode::F => Code::KeyF,
            KeyCode::G => Code::KeyG,
            KeyCode::H => Code::KeyH,
            KeyCode::I => Code::KeyI,
            KeyCode::J => Code::KeyJ,
            KeyCode::K => Code::KeyK,
            KeyCode::L => Code::KeyL,
            KeyCode::M => Code::KeyM,
            KeyCode::N => Code::KeyN,
            KeyCode::O => Code::KeyO,
            KeyCode::P => Code::KeyP,
            KeyCode::Q => Code::KeyQ,
            KeyCode::R => Code::KeyR,
            KeyCode::S => Code::KeyS,
            KeyCode::T => Code::KeyT,
            KeyCode::U => Code::KeyU,
            KeyCode::V => Code::KeyV,
            KeyCode::W => Code::KeyW,
            KeyCode::X => Code::KeyX,
            KeyCode::Y => Code::KeyY,
            KeyCode::Z => Code::KeyZ,
            KeyCode::F1 => Code::F1,
            KeyCode::F2 => Code::F2,
            KeyCode::F3 => Code::F3,
            KeyCode::F4 => Code::F4,
            KeyCode::F5 => Code::F5,
            KeyCode::F6 => Code::F6,
            KeyCode::F7 => Code::F7,
            KeyCode::F8 => Code::F8,
            KeyCode::F9 => Code::F9,
            KeyCode::F10 => Code::F10,
            KeyCode::F11 => Code::F11,
            KeyCode::F12 => Code::F12,
            KeyCode::Key0 => Code::Digit0,
            KeyCode::Key1 => Code::Digit1,
            KeyCode::Key2 => Code::Digit2,
            KeyCode::Key3 => Code::Digit3,
            KeyCode::Key4 => Code::Digit4,
            KeyCode::Key5 => Code::Digit5,
            KeyCode::Key6 => Code::Digit6,
            KeyCode::Key7 => Code::Digit7,
            KeyCode::Key8 => Code::Digit8,
            KeyCode::Key9 => Code::Digit9,
            KeyCode::Space => Code::Space,
            KeyCode::Enter => Code::Enter,
            KeyCode::Tab => Code::Tab,
            KeyCode::Backquote => Code::Backquote,
        }
    }
}

impl Default for KeyCode {
    fn default() -> Self {
        KeyCode::V
    }
}

/// 快捷键配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub key: KeyCode,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
            key: KeyCode::V,
        }
    }
}

impl HotkeyConfig {
    
    /// Checks whether two hotkey configurations are identical.
    ///
    /// Identical hotkeys would conflict if both were registered at the same time.
    pub fn conflicts_with(&self, other: &HotkeyConfig) -> bool {
        self.ctrl == other.ctrl
            && self.shift == other.shift
            && self.alt == other.alt
            && self.meta == other.meta
            && self.key == other.key
    }

    /// 检查快捷键是否有效
    pub fn is_valid(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.meta
    }

    /// 显示快捷键组合
    pub fn display(&self) -> String {
        let mut parts = Vec::new();

        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.meta {
            #[cfg(target_os = "macos")]
            parts.push("Cmd");
            #[cfg(not(target_os = "macos"))]
            parts.push("Win");
        }

        parts.push(self.key.display());

        parts.join(" + ")
    }

    /// 转换为 global_hotkey 的 HotKey
    pub fn to_global_hotkey(&self) -> Option<HotKey> {
        let mut modifiers = Modifiers::empty();

        if self.ctrl {
            modifiers |= Modifiers::CONTROL;
        }
        if self.shift {
            modifiers |= Modifiers::SHIFT;
        }
        if self.alt {
            modifiers |= Modifiers::ALT;
        }
        if self.meta {
            modifiers |= Modifiers::META;
        }

        let mods = if modifiers.is_empty() {
            None
        } else {
            Some(modifiers)
        };

        Some(HotKey::new(mods, self.key.to_code()))
    }
}
