//! One business dispatcher for desktop IPC and standard MCP tools.
use crate::context::CoreSession;
use serde_json::{Value, json};
use vault_contracts::{
    contracts::*,
    credentials::*,
    errors::{AppError, Result},
    operations::*,
    rpc::*,
};

/// Handle a parsed JSON-RPC message, including batches and notifications.
pub(crate) fn handle(session: &CoreSession, value: Value) -> Option<Value> {
    if let Value::Array(requests) = value {
        if requests.is_empty() {
            return Some(failure(Value::Null, -32600, "Invalid Request", None));
        }
        let replies: Vec<_> = requests
            .into_iter()
            .filter_map(|request| handle_one(session, request))
            .collect();
        return (!replies.is_empty()).then_some(Value::Array(replies));
    }
    handle_one(session, value)
}
fn handle_one(session: &CoreSession, value: Value) -> Option<Value> {
    let id = value.get("id").cloned();
    let valid_id = id
        .as_ref()
        .is_none_or(|v| v.is_null() || v.is_string() || v.is_number());
    let request = serde_json::from_value::<Request>(value);
    let request = match request {
        Ok(request)
            if request.jsonrpc == VERSION
                && valid_id
                && (request.params.is_object() || request.params.is_array()) =>
        {
            request
        }
        _ => return Some(failure(Value::Null, -32600, "Invalid Request", None)),
    };
    let result = dispatch(session, &request.method, request.params);
    let id = id?; // Notifications execute but never produce a response.
    Some(match result {
        Ok(result) => serde_json::to_value(Success {
            jsonrpc: VERSION,
            id,
            result,
        })
        .expect("JSON values are serializable"),
        Err(error) => {
            let code = match error.code {
                "METHOD_NOT_FOUND" => -32601,
                "INVALID_INPUT" => -32602,
                _ => -32000,
            };
            failure(id, code, error.message, serde_json::to_value(error).ok())
        }
    })
}
pub(crate) fn failure(id: Value, code: i32, message: &str, data: Option<Value>) -> Value {
    serde_json::to_value(Failure {
        jsonrpc: VERSION,
        id,
        error: RpcError {
            code,
            message: message.into(),
            data,
        },
    })
    .expect("JSON values are serializable")
}

fn one_or_many<T: serde::de::DeserializeOwned>(value: Value) -> Result<(Vec<T>, bool)> {
    if value.get("items").is_some() {
        let batch: Batch<T> = arguments(value)?;
        if batch.items.is_empty() || batch.items.len() > 100 {
            return Err(AppError::invalid().with("items", "批量数量必须在 1-100 之间。"));
        }
        Ok((batch.items, true))
    } else {
        Ok((vec![arguments(value)?], false))
    }
}
fn output(mut results: Vec<Value>, batch: bool) -> Value {
    if batch {
        json!({"results": results})
    } else {
        results.remove(0)
    }
}

fn arguments<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|error| AppError::invalid().with("arguments", &error.to_string()))
}

pub(crate) fn dispatch(core: &CoreSession, name: &str, value: Value) -> Result<Value> {
    match name {
        "mcp.configuration" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::McpConfig)
        }
        "credentials.search" => {
            let (items, batch) = one_or_many::<Search>(value)?;
            Ok(output(
                items
                    .into_iter()
                    .map(|args| core.execute(Operation::Search(args)))
                    .collect::<Result<Vec<_>>>()?,
                batch,
            ))
        }
        "credentials.get" => {
            let (items, batch) = one_or_many::<ById>(value)?;
            let results = core.call(Operation::Read(
                items.into_iter().map(|item| item.id).collect(),
            ))?;
            Ok(output(results, batch))
        }
        "credentials.create" => {
            let (items, batch) = one_or_many::<EntryInput>(value)?;
            Ok(output(core.call(Operation::Create(items))?, batch))
        }
        "credentials.update" => {
            let (items, batch) = one_or_many::<EntryPatch>(value)?;
            Ok(output(core.call(Operation::Update(items))?, batch))
        }
        "credentials.delete" => {
            let (items, batch) = one_or_many::<EntryVersion>(value)?;
            Ok(output(core.call(Operation::Delete(items))?, batch))
        }
        "trash.restore" => {
            let item: EntryVersion = arguments(value)?;
            core.execute(Operation::Restore {
                id: item.id,
                expected_revision: item.expected_revision,
            })
        }
        "credentials.pin" => {
            let item: PinArgs = arguments(value)?;
            core.execute(Operation::Pin {
                id: item.id,
                expected_revision: item.expected_revision,
                pinned: item.pinned,
            })
        }
        "tags.organize" => {
            let item: OrganizeArgs = arguments(value)?;
            core.execute(Operation::Organize {
                kind: item.kind,
                name: item.name,
                previous: item.previous,
            })
            .map(|_| json!({"status":"ok"}))
        }
        "filling.write" => core.execute(Operation::Write(arguments(value)?)),
        "filling.capture" => core.execute(Operation::CaptureTarget(arguments(value)?)),
        "application.info" => {
            let _: EmptyArgs = arguments(value)?;
            core.application_info()
        }
        "credentials.list" => {
            let args: Search = arguments(value)?;
            let value = core.execute(Operation::Search(args))?;
            Ok(value["entries"].clone())
        }
        "tags.list" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::Organization)
        }
        "trash.list" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::ListTrash)
        }
        "trash.empty" => {
            let args: PurgeArgs = arguments(value)?;
            core.empty_trash(args)
        }
        "credentials.readSecret" => {
            let args: SecretArgs = arguments(value)?;
            core.execute(Operation::ReadField(FieldRequest {
                id: args.id,
                expected_revision: args.expected_revision,
                field: "secret".into(),
                service_id: None,
            }))
        }
        "clipboard.copyField" => core.execute(Operation::CopyField(arguments(value)?)),
        "clipboard.copyText" => {
            let args: TextArgs = arguments(value)?;
            core.execute(Operation::CopyText(args.text))
        }
        "browser.open" => {
            let args: UrlArgs = arguments(value)?;
            core.execute(Operation::OpenUrl(args.url))
        }
        "settings.get" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::Settings)
        }
        "settings.save" => {
            let args: SettingsArgs = arguments(value)?;
            core.execute(Operation::SaveSettings(args.settings))
        }
        "permissions.preview" => {
            let args: SettingsArgs = arguments(value)?;
            core.execute(Operation::PermissionPreview(args.settings))
        }
        "startup.set" => {
            let args: StartupArgs = arguments(value)?;
            core.execute(Operation::SetStartup {
                executable: std::env::current_exe().map_err(|_| AppError::storage())?,
                enabled: args.enabled,
            })
        }
        "updates.status" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::UpdateStatus)
        }
        "updates.check" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::CheckUpdate)
        }
        "updates.download" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::DownloadUpdate)
        }
        "updates.cancel" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::CancelUpdate)
        }
        "updates.install" => {
            let _: EmptyArgs = arguments(value)?;
            core.execute(Operation::InstallUpdate)
        }
        _ => Err(AppError::new("METHOD_NOT_FOUND", "该方法不存在。")),
    }
}
