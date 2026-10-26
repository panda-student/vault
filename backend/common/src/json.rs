//! Serialization helper shared by application operations.
use serde::Serialize;
use serde_json::Value;
use vault_contracts::errors::{AppError, Result};
pub fn encoded(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| AppError::storage())
}
