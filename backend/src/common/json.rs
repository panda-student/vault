//! Serialization helper shared by application operations.
use crate::definitions::errors::{AppError, Result};
use serde::Serialize;
use serde_json::Value;
pub(crate) fn encoded(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| AppError::storage())
}
