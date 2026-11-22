//! Redacts protected values and their common encodings. No protocol or core dependency.
use base64::Engine;
use serde_json::Value;
use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};
use vault_contracts::errors::{AppError, Result};
use zeroize::Zeroizing;
/// A protected value; zeroized on drop and hashed by its text.
struct Protected(Zeroizing<String>);
impl PartialEq for Protected {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}
impl Eq for Protected {}
impl Hash for Protected {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state)
    }
}
impl Borrow<str> for Protected {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}
#[derive(Default)]
pub struct OutputGuard {
    values: HashSet<Protected>,
}
pub type SharedGuard = Arc<RwLock<OutputGuard>>;
impl OutputGuard {
    pub fn add(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        let encoded = serde_json::to_string(value).unwrap_or_default();
        let escaped = encoded
            .get(1..encoded.len().saturating_sub(1))
            .unwrap_or("");
        for item in [
            value.to_owned(),
            base64::engine::general_purpose::STANDARD.encode(value),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value),
            percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
                .to_string(),
            escaped.to_owned(),
        ] {
            if !item.is_empty() && !self.values.contains(item.as_str()) {
                self.values.insert(Protected(Zeroizing::new(item)));
            }
        }
    }
    fn contains(&self, text: &str) -> bool {
        self.values
            .iter()
            .any(|value| text.contains(value.0.as_str()))
    }
    /// Replaces every protected value inside `text` with `***`, longest first,
    /// so text that merely contains one keeps the rest readable.
    pub fn mask_text(&self, text: &str) -> String {
        let mut values: Vec<&str> = self
            .values
            .iter()
            .map(|value| value.0.as_str())
            .filter(|value| text.contains(*value))
            .collect();
        if values.is_empty() {
            return text.to_owned();
        }
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        let mut masked = text.to_owned();
        for value in values {
            masked = masked.replace(value, "***");
        }
        masked
    }
    /// What a program vault ran printed is masked in place, not dropped: one
    /// line that happens to contain an unrelated stored value must not hide
    /// the whole output. Other fields still go through `scrub`.
    pub fn mask_program_output(&self, value: &mut Value) {
        let Value::Object(values) = value else {
            return;
        };
        if values.get("status").and_then(Value::as_str) != Some("command_finished") {
            return;
        }
        for name in ["stdout", "stderr"] {
            if let Some(Value::String(text)) = values.get_mut(name) {
                *text = self.mask_text(text);
            }
        }
    }
    pub fn scrub(&self, value: &mut Value) {
        match value {
            Value::String(text) if self.contains(text) => *value = Value::Null,
            Value::Array(values) => {
                for value in values {
                    self.scrub(value);
                }
            }
            Value::Object(values) => {
                values.retain(|key, _| !self.contains(key));
                for value in values.values_mut() {
                    self.scrub(value);
                }
            }
            _ => {}
        }
    }
}
pub fn scrub(guard: &SharedGuard, mut value: Value) -> Result<Value> {
    guard
        .read()
        .map_err(|_| AppError::storage())?
        .scrub(&mut value);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_metadata_and_common_encodings_are_suppressed() {
        let mut guard = OutputGuard::default();
        guard.add("synthetic-protected-API_123");
        let mut value = serde_json::json!({"title":"prefix synthetic-protected-API_123 suffix","nested":[base64::engine::general_purpose::STANDARD.encode("synthetic-protected-API_123")],"safe":"service label"});
        guard.scrub(&mut value);
        assert!(value["title"].is_null());
        assert!(value["nested"][0].is_null());
        assert_eq!(value["safe"], "service label");
    }
    #[test]
    fn program_output_is_masked_in_place() {
        let mut guard = OutputGuard::default();
        guard.add("1234");
        guard.add("synthetic-protected-API_123");
        let mut value = serde_json::json!({"status":"command_finished","stdout":"account 991234 ok\nmail list","stderr":"key synthetic-protected-API_123","exitCode":0});
        guard.mask_program_output(&mut value);
        guard.scrub(&mut value);
        assert_eq!(value["stdout"], "account 99*** ok\nmail list");
        assert_eq!(value["stderr"], "key ***");
        let mut other = serde_json::json!({"status":"other","stdout":"991234"});
        guard.mask_program_output(&mut other);
        guard.scrub(&mut other);
        assert!(other["stdout"].is_null());
    }
}
