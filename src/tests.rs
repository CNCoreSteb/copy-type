//! Unit tests for top-level helpers and the shared clipboard-history state.

use super::{
    format_history_timestamp, truncate_text, SharedState, MAX_SINGLE_ITEM_SIZE, MAX_TOTAL_MEMORY,
};
use crate::i18n::I18n;

/// Build a `SharedState` with history enabled and the given item cap.
fn history_state(max_items: u32) -> SharedState {
    let state = SharedState::new(I18n::new("zh-CN"));
    *state.history_enabled.lock().unwrap() = true;
    *state.history_max_items.lock().unwrap() = max_items;
    state
}

fn texts(state: &SharedState) -> Vec<String> {
    state
        .clipboard_history
        .lock()
        .unwrap()
        .iter()
        .map(|item| item.text.clone())
        .collect()
}

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn memory(state: &SharedState) -> usize {
    *state.history_memory_used.lock().unwrap()
}

#[test]
fn truncate_text_shorter_than_limit_escapes_newlines() {
    assert_eq!(truncate_text("a\r\nb", 50), "a\\r\\nb");
}

#[test]
fn truncate_text_at_exact_limit() {
    assert_eq!(truncate_text("abcde", 5), "abcde");
}

#[test]
fn truncate_text_longer_than_limit_appends_ellipsis() {
    assert_eq!(truncate_text("abcdefghij", 5), "abcde...");
}

#[test]
fn truncate_text_respects_utf8_char_boundaries() {
    // "ä" is two bytes; a limit of 3 lands mid-character and must round down
    // to a valid boundary instead of panicking.
    assert_eq!(truncate_text("ääää", 3), "ää...");
}

#[test]
fn record_history_noop_when_disabled() {
    let state = SharedState::new(I18n::new("zh-CN")); // history disabled by default
    state.record_history("hello".to_string());
    assert!(texts(&state).is_empty());
}

#[test]
fn record_history_noop_when_max_items_zero() {
    let state = SharedState::new(I18n::new("zh-CN"));
    *state.history_enabled.lock().unwrap() = true; // max_items stays 0
    state.record_history("hello".to_string());
    assert!(texts(&state).is_empty());
}

#[test]
fn record_history_tracks_items_and_memory() {
    let state = history_state(10);
    state.record_history("abc".to_string());
    state.record_history("de".to_string());
    assert_eq!(texts(&state), owned(&["abc", "de"]));
    assert_eq!(memory(&state), 5);
}

#[test]
fn record_history_evicts_oldest_beyond_max_items() {
    let state = history_state(3);
    for t in ["a", "b", "c", "d", "e"] {
        state.record_history(t.to_string());
    }
    assert_eq!(texts(&state), owned(&["c", "d", "e"]));
    assert_eq!(memory(&state), 3);
}

#[test]
fn record_history_rejects_oversized_item() {
    let state = history_state(10);
    state.record_history("a".repeat(MAX_SINGLE_ITEM_SIZE + 1));
    assert!(texts(&state).is_empty());
    assert_eq!(memory(&state), 0);
}

#[test]
fn record_history_evicts_to_stay_under_total_memory() {
    let state = history_state(100);
    let nine_mb = "a".repeat(9 * 1024 * 1024);
    for _ in 0..6 {
        state.record_history(nine_mb.clone());
    }
    // 9MB * 6 = 54MB > 50MB cap, so the oldest item is evicted, leaving 5.
    assert_eq!(state.clipboard_history.lock().unwrap().len(), 5);
    assert!(memory(&state) <= MAX_TOTAL_MEMORY);
    assert_eq!(memory(&state), 5 * 9 * 1024 * 1024);
}

#[test]
fn clear_history_resets_items_and_memory() {
    let state = history_state(10);
    state.record_history("abc".to_string());
    state.clear_history();
    assert!(texts(&state).is_empty());
    assert_eq!(memory(&state), 0);
}

#[test]
fn trim_history_drops_oldest_over_limit() {
    let state = history_state(10);
    for t in ["a", "b", "c", "d"] {
        state.record_history(t.to_string());
    }
    *state.history_max_items.lock().unwrap() = 2;
    state.trim_history();
    assert_eq!(texts(&state), owned(&["c", "d"]));
    assert_eq!(memory(&state), 2);
}

#[test]
fn trim_history_with_zero_max_clears_all() {
    let state = history_state(10);
    state.record_history("abc".to_string());
    *state.history_max_items.lock().unwrap() = 0;
    state.trim_history();
    assert!(texts(&state).is_empty());
    assert_eq!(memory(&state), 0);
}

#[test]
fn format_history_timestamp_has_hh_mm_ss_shape() {
    let ts = format_history_timestamp();
    assert_eq!(ts.len(), 8);
    assert!(ts.chars().enumerate().all(|(i, c)| {
        if i == 2 || i == 5 {
            c == ':'
        } else {
            c.is_ascii_digit()
        }
    }));
}
