//! Unit tests for hotkey configuration.

use super::{HotkeyConfig, KeyCode};
use global_hotkey::hotkey::Code;
use std::collections::HashSet;

#[test]
fn default_is_ctrl_shift_v() {
    let c = HotkeyConfig::default();
    assert!(c.ctrl && c.shift && !c.alt && !c.meta);
    assert_eq!(c.key, KeyCode::V);
    assert!(c.is_valid());
}

#[test]
fn key_code_default_is_v() {
    assert_eq!(KeyCode::default(), KeyCode::V);
}

#[test]
fn is_valid_requires_at_least_one_modifier() {
    let no_mods = HotkeyConfig {
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
        key: KeyCode::A,
    };
    assert!(!no_mods.is_valid());
    let with_alt = HotkeyConfig { alt: true, ..no_mods.clone() };
    assert!(with_alt.is_valid());
}

#[test]
fn conflicts_with_detects_identical_configs() {
    let a = HotkeyConfig::default();
    assert!(a.conflicts_with(&HotkeyConfig::default()));

    let mut diff_key = a.clone();
    diff_key.key = KeyCode::A;
    assert!(!a.conflicts_with(&diff_key));

    let mut diff_mod = a.clone();
    diff_mod.alt = true;
    assert!(!a.conflicts_with(&diff_mod));
}

#[test]
fn display_formats_combo() {
    assert_eq!(HotkeyConfig::default().display(), "Ctrl + Shift + V");
    let c = HotkeyConfig {
        ctrl: true,
        shift: false,
        alt: true,
        meta: false,
        key: KeyCode::A,
    };
    assert_eq!(c.display(), "Ctrl + Alt + A");
}

#[test]
fn to_global_hotkey_equal_for_equal_configs() {
    let a = HotkeyConfig::default().to_global_hotkey().unwrap();
    let b = HotkeyConfig::default().to_global_hotkey().unwrap();
    assert_eq!(a, b);

    let other = HotkeyConfig {
        key: KeyCode::A,
        ..HotkeyConfig::default()
    };
    assert_ne!(a, other.to_global_hotkey().unwrap());
}

#[test]
fn key_code_to_code_mapping() {
    assert_eq!(KeyCode::A.to_code(), Code::KeyA);
    assert_eq!(KeyCode::Key0.to_code(), Code::Digit0);
    assert_eq!(KeyCode::F5.to_code(), Code::F5);
    assert_eq!(KeyCode::Backquote.to_code(), Code::Backquote);
}

#[test]
fn key_code_all_is_complete_and_unique() {
    let all = KeyCode::all();
    assert_eq!(all.len(), 52);

    let mut seen = HashSet::new();
    for key in &all {
        assert!(!key.display().is_empty());
        assert!(seen.insert(key.display()), "duplicate display: {}", key.display());
    }
    assert_eq!(seen.len(), all.len());
}

#[test]
fn serde_round_trip_preserves_config() {
    let cfg = HotkeyConfig {
        ctrl: true,
        shift: false,
        alt: true,
        meta: false,
        key: KeyCode::F5,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let back: HotkeyConfig = serde_json::from_str(&json).unwrap();
    assert!(cfg.conflicts_with(&back));
}
