//! JSON-RPC 2.0 wire definitions; business errors are carried in error.data.
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const VERSION: &str = "2.0";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default = "empty_params")]
    pub params: Value,
    #[serde(rename = "id")]
    pub _id: Option<Value>,
}
fn empty_params() -> Value {
    serde_json::json!({})
}
#[derive(Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
#[derive(Serialize)]
pub struct Success {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub result: Value,
}
#[derive(Serialize)]
pub struct Failure {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub error: RpcError,
}
