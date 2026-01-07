use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Simple i18n helper that loads translations from embedded TOML files.
#[derive(Clone)]
pub struct I18n {
    current_lang: Arc<RwLock<String>>,
    store: Arc<HashMap<String, HashMap<String, String>>>,
}

impl I18n {
    /// Create a new i18n handle with the given language code. Falls back to `zh-CN` if unknown.
    pub fn new(lang: &str) -> Self {
        let store = load_store();
        let default_lang = "zh-CN".to_string();
        let initial = if store.contains_key(lang) {
            lang.to_string()
        } else {
            default_lang.clone()
        };

        Self {
            current_lang: Arc::new(RwLock::new(initial)),
            store: Arc::new(store),
        }
    }

    /// Get the current language code.
    pub fn current_language(&self) -> String {
        self.current_lang.read().unwrap().clone()
    }

    /// Set the current language code if it exists; otherwise keep the previous value.
    pub fn set_language(&self, lang: &str) {
        if self.store.contains_key(lang) {
            *self.current_lang.write().unwrap() = lang.to_string();
        }
    }

    /// Translate a key without parameters.
    pub fn t(&self, key: &str) -> String {
        self.tr(key, &[])
    }

    /// Translate a key with placeholder replacements (`%{name}`).
    pub fn tr<'a>(&self, key: &str, args: &[(&str, &'a str)]) -> String {
        let lang = self.current_language();
        let text = self
            .lookup(&lang, key)
            .or_else(|| self.lookup("zh-CN", key))
            .unwrap_or_else(|| key.to_string());

        args.iter().fold(text, |acc, (k, v)| {
            acc.replace(&format!("%{{{}}}", k), v)
        })
    }

    /// List available languages `(code, display_name)`.
    pub fn available_languages(&self) -> Vec<(&'static str, &'static str)> {
        vec![("zh-CN", "简体中文"), ("en", "English")]
    }

    fn lookup(&self, lang: &str, key: &str) -> Option<String> {
        self.store.get(lang).and_then(|m| m.get(key).cloned())
    }
}

fn load_store() -> HashMap<String, HashMap<String, String>> {
    let mut store = HashMap::new();
    store.insert(
        "zh-CN".to_string(),
        parse_lang(include_str!("../i18n/zh-CN.toml")),
    );
    store.insert("en".to_string(), parse_lang(include_str!("../i18n/en.toml")));
    store
}

fn parse_lang(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    match content.parse::<toml::Value>() {
        Ok(value) => flatten("", &value, &mut map),
        Err(err) => {
            // parsing errors should not crash the app; leave map empty to fall back to keys
            log::warn!("Failed to parse i18n file: {}", err);
        }
    }
    map
}

fn flatten(prefix: &str, value: &toml::Value, out: &mut HashMap<String, String>) {
    match value {
        toml::Value::Table(table) => {
            for (k, v) in table {
                let next_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten(&next_prefix, v, out);
            }
        }
        toml::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => { /* ignore non-string values */ }
    }
}
