//! Unit tests for the i18n module.

use super::{parse_lang, parse_lang_bytes, I18n};

#[test]
fn new_uses_requested_language() {
    let i18n = I18n::new("en");
    assert_eq!(i18n.current_language(), "en");
}

#[test]
fn new_falls_back_to_default_for_unknown_language() {
    let i18n = I18n::new("does-not-exist");
    assert_eq!(i18n.current_language(), "zh-CN");
}

#[test]
fn translate_returns_language_specific_value() {
    assert_eq!(I18n::new("en").t("status.ready"), "Ready");
    assert_eq!(I18n::new("zh-CN").t("status.ready"), "就绪");
}

#[test]
fn missing_key_returns_key_itself() {
    assert_eq!(I18n::new("en").t("no.such.key"), "no.such.key");
}

#[test]
fn placeholder_substitution() {
    let i18n = I18n::new("en");
    assert_eq!(i18n.tr("ui.label_status", &[("status", "OK")]), "Status: OK");
}

#[test]
fn set_language_keeps_previous_on_invalid() {
    let i18n = I18n::new("en");
    i18n.set_language("zh-CN");
    assert_eq!(i18n.current_language(), "zh-CN");
    i18n.set_language("invalid");
    assert_eq!(i18n.current_language(), "zh-CN");
}

#[test]
fn available_languages_contains_both() {
    let langs = I18n::new("en").available_languages();
    assert!(langs.iter().any(|(code, _)| *code == "en"));
    assert!(langs.iter().any(|(code, _)| *code == "zh-CN"));
}

#[test]
fn parse_lang_flattens_nested_tables() {
    let map = parse_lang("[a]\nb = \"c\"\n\n[a.d]\ne = \"f\"\n");
    assert_eq!(map.get("a.b").map(String::as_str), Some("c"));
    assert_eq!(map.get("a.d.e").map(String::as_str), Some("f"));
}

#[test]
fn parse_lang_ignores_non_string_values() {
    let map = parse_lang("[s]\nk = \"v\"\nn = 5\n");
    assert_eq!(map.get("s.k").map(String::as_str), Some("v"));
    assert!(!map.contains_key("s.n"));
}

#[test]
fn parse_lang_invalid_returns_empty_without_panic() {
    let map = parse_lang("= = not valid = =");
    assert!(map.is_empty());
}

#[test]
fn parse_lang_bytes_strips_utf8_bom() {
    let with_bom = b"\xEF\xBB\xBF[s]\nk = \"v\"\n";
    let map = parse_lang_bytes(with_bom);
    assert_eq!(map.get("s.k").map(String::as_str), Some("v"));
}
