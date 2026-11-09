mod arguments;
pub mod bridge;
mod writer;
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
use vault_contracts::errors::{AppError, Result};
use vault_core::api::CoreSession;
use writer::SafeWriter;
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
        "export_entries" => "transfer.mcpExport",
        "get_settings" => "settings.mcpGet",
        "update_settings" => "settings.mcpUpdate",
        "sync_now" => "sync.mcpRun",
        _ => return Err(AppError::new("UNKNOWN_TOOL", "该工具不存在。")),
    };
    core.invoke(method, value)
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
    /// Every outcome of a tool call, including an unknown tool name or malformed
    /// arguments, is a tool result with `isError`, so the calling model sees the
    /// reason and can correct itself. Protocol errors are what clients surface to
    /// the user as pop-ups instead.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let Some(tool) = tools().into_iter().find(|tool| tool.name == request.name) else {
            let names: Vec<String> = tools().iter().map(|t| t.name.to_string()).collect();
            return Ok(CallToolResult::structured_error(json!({
                "code": "UNKNOWN_TOOL",
                "message": "该工具不存在。",
                "details": [{"field": "name", "reason": format!("可用工具：{}", names.join(", "))}]
            }))
            .into());
        };
        let core = self.core.clone();
        let name = request.name.to_string();
        let schema = Value::Object((*tool.input_schema).clone());
        let value = Value::Object(request.arguments.unwrap_or_default());
        let outcome = tokio::task::spawn_blocking(move || {
            let value = arguments::repair_arguments(&schema, value);
            core.protect_input(&value)?;
            let mut result = execute(&core, &name, value).map_err(|error| json!(error));
            // Redact before the result text is rendered, so a protected value can
            // never turn the whole response into an invalid frame downstream. The
            // guard is current: execute refreshed it. If it is unavailable the
            // SafeWriter refuses the frame, so nothing unredacted leaves.
            if let Ok(guard) = core.output_guard().read() {
                match &mut result {
                    Ok(value) | Err(value) => guard.scrub(value),
                }
            }
            Ok::<_, AppError>(result)
        })
        .await;
        let result = match outcome {
            Ok(Ok(Ok(value))) => CallToolResult::structured(value),
            Ok(Ok(Err(error))) => CallToolResult::structured_error(error),
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
