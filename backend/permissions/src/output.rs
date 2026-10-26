//! Redacts protected values and their common encodings. No protocol or core dependency.
use base64::Engine;
use serde_json::Value;
use std::sync::{Arc, RwLock};
use vault_contracts::errors::{AppError, Result};
use zeroize::Zeroizing;
#[derive(Default)]
pub struct OutputGuard {
    values: Vec<Zeroizing<String>>,
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
            if !item.is_empty() && !self.values.iter().any(|existing| existing.as_str() == item) {
                self.values.push(Zeroizing::new(item));
            }
        }
    }
    fn contains(&self, text: &str) -> bool {
        self.values
            .iter()
            .any(|value| text.contains(value.as_str()))
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
}
