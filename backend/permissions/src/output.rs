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
