//! Unit tests for application configuration.

use super::{AppConfig, CloseAction, TypingFormat};
use crate::hotkey_config::{HotkeyConfig, KeyCode};

#[test]
fn defaults_are_sane() {
    let c = AppConfig::default();
    assert_eq!(c.close_action, CloseAction::MinimizeToTray);
    assert!(!c.start_minimized);
    assert!(!c.show_console);
    assert_eq!(c.typing_delay, 20);
    assert_eq!(c.typing_variance, 0);
    assert_eq!(c.typing_format, TypingFormat::Raw);
    assert!(!c.history_enabled);
    assert_eq!(c.history_max_items, 20);
    assert_eq!(c.language, "zh-CN");
    assert!(c.hotkey.conflicts_with(&HotkeyConfig::default()));
    assert_eq!(c.hotkey.key, KeyCode::V);
}

#[test]
fn close_action_default_is_minimize() {
    assert_eq!(CloseAction::default(), CloseAction::MinimizeToTray);
}

#[test]
fn typing_format_default_is_raw() {
    assert_eq!(TypingFormat::default(), TypingFormat::Raw);
}

#[test]
fn typing_format_raw_keeps_text_unchanged() {
    let input = "  fn main() {\n    println!();\n}\n";
    assert_eq!(TypingFormat::Raw.apply(input), input);
}

#[test]
fn typing_format_strip_indent_removes_leading_whitespace_per_line() {
    let input = "fn main() {\n    let x = 1;\n\tlet y = 2;\n}";
    let expected = "fn main() {\nlet x = 1;\nlet y = 2;\n}";
    assert_eq!(TypingFormat::StripIndent.apply(input), expected);
}

#[test]
fn typing_format_single_line_joins_with_spaces_and_drops_blank_lines() {
    let input = "  line one  \n\n   line two";
    assert_eq!(TypingFormat::SingleLine.apply(input), "line one line two");
}

#[test]
fn typing_format_handles_crlf_line_endings() {
    let input = "a\r\n  b";
    assert_eq!(TypingFormat::StripIndent.apply(input), "a\nb");
    assert_eq!(TypingFormat::SingleLine.apply(input), "a b");
}

#[test]
fn normalize_clamps_history_max_items() {
    let mut c = AppConfig {
        history_max_items: 0,
        ..AppConfig::default()
    };
    c.normalize();
    assert_eq!(c.history_max_items, 20);

    c.history_max_items = 150;
    c.normalize();
    assert_eq!(c.history_max_items, 100);

    c.history_max_items = 50;
    c.normalize();
    assert_eq!(c.history_max_items, 50);
}

#[test]
fn deserialize_applies_serde_defaults() {
    let json = r#"{"close_action":"ExitApp","start_minimized":true}"#;
    let c: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.close_action, CloseAction::ExitApp);
    assert!(c.start_minimized);
    assert_eq!(c.typing_delay, 20);
    assert_eq!(c.typing_variance, 0);
    assert_eq!(c.typing_format, TypingFormat::Raw);
    assert_eq!(c.history_max_items, 20);
    assert_eq!(c.language, "zh-CN");
}

#[test]
fn deserialize_ignores_removed_legacy_fields() {
    // Configs written by older versions contain auto_start / autostart_asked /
    // typing_variance_enabled. They must still load without error so that an
    // upgrade does not reset the user's settings.
    let json = r#"{
        "close_action": "MinimizeToTray",
        "auto_start": true,
        "start_minimized": false,
        "autostart_asked": true,
        "show_console": false,
        "typing_delay": 33,
        "typing_variance": 7,
        "typing_variance_enabled": true,
        "history_enabled": true,
        "history_max_items": 9,
        "language": "en"
    }"#;
    let c: AppConfig = serde_json::from_str(json).expect("legacy config should still parse");
    assert_eq!(c.typing_delay, 33);
    assert_eq!(c.typing_variance, 7);
    assert!(c.history_enabled);
    assert_eq!(c.history_max_items, 9);
    assert_eq!(c.language, "en");
}

#[test]
fn serde_round_trip_preserves_values() {
    let c = AppConfig::default();
    let json = serde_json::to_string_pretty(&c).unwrap();
    let back: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.close_action, c.close_action);
    assert_eq!(back.typing_delay, c.typing_delay);
    assert_eq!(back.typing_variance, c.typing_variance);
    assert_eq!(back.history_max_items, c.history_max_items);
    assert_eq!(back.language, c.language);
}
