pub mod bridge;
use crate::services::permissions::output::SafeWriter;
use crate::{
    api::CoreSession,
    definitions::errors::{AppError, Result},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
};
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone)]
struct VaultServer {
    core: Arc<CoreSession>,
}
mod catalog;
use catalog::tools;
/// MCP retains standard tools/call framing and maps tools to shared RPC methods.
fn execute(core: &CoreSession, name: &str, value: Value) -> Result<Value> {
    let method = match name {
        "get_mcp_config" => "mcp.configuration",
        "search_entries" => "credentials.search",
        "get_entries" => "credentials.get",
        "create_entries" => "credentials.create",
        "update_entries" => "credentials.update",
        "delete_entries" => "credentials.delete",
        "restore_entries" => "trash.restore",
        "pin_entries" => "credentials.pin",
        "organize" => "tags.organize",
        "write_credential" => "filling.write",
        "capture_fill_target" => "filling.capture",
        _ => return Err(AppError::new("UNKNOWN_TOOL", "该工具不存在。")),
    };
    crate::api::rpc::dispatch(core, method, value)
}

impl ServerHandler for VaultServer {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(include_str!("instructions.txt"));
        config.server_info = Implementation::new("vault", env!("CARGO_PKG_VERSION"));
        config
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools()
                .into_iter()
                .filter(|tool| self.core.tool_allowed(tool.name.as_ref()).unwrap_or(false))
                .collect(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if !tools().iter().any(|tool| tool.name == request.name) {
            return Err(ErrorData::invalid_params("Unknown tool", None));
        }
        let core = self.core.clone();
        let name = request.name.to_string();
        let value = Value::Object(request.arguments.unwrap_or_default());
        let outcome = tokio::task::spawn_blocking(move || {
            core.protect_input(&value)?;
            execute(&core, &name, value)
        })
        .await;
        let result = match outcome {
            Ok(Ok(value)) => CallToolResult::structured(value),
            Ok(Err(error)) => CallToolResult::structured_error(json!(error)),
            Err(_) => CallToolResult::structured_error(
                json!({"code":"INTERNAL_ERROR","message":"Local operation failed"}),
            ),
        };
        Ok(result.into())
    }
}
pub async fn serve_connection<R, W>(
    core: Arc<CoreSession>,
    input: R,
    output: W,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let guard = core.output_guard();
    let writer = SafeWriter::new(output, guard.clone()).with_core(core.clone());
    let service = VaultServer { core }.serve((input, writer)).await?;
    service.waiting().await?;
    Ok(())
}
