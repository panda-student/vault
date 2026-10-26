//! Public protocol entry points. Business services never depend on the API layer.
use super::CoreSession;
use serde_json::Value;
impl CoreSession {
    /// Public wire entry point shared by all desktop business commands.
    pub fn handle_rpc(&self, request: Value) -> Option<Value> {
        crate::api::rpc::handle(self, request)
    }
    pub fn handle_json(&self, request: &str) -> Option<Value> {
        match serde_json::from_str(request) {
            Ok(value) => self.handle_rpc(value),
            Err(_) => Some(crate::api::rpc::failure(
                Value::Null,
                -32700,
                "Parse error",
                None,
            )),
        }
    }
}
