//! Transport adapter only. Caller identity is already bound to AppState.core.
use crate::app::AppState;
#[tauri::command]
pub(crate) async fn rpc(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: serde_json::Value,
) -> std::result::Result<Option<serde_json::Value>, String> {
    let core = state.core.clone();
    let (response, restart) = tauri::async_runtime::spawn_blocking(move || {
        let response = core.handle_rpc(request);
        (response, core.restart_requested())
    })
    .await
    .map_err(|_| "Local RPC worker failed".to_string())?;
    if restart {
        app.exit(0);
    }
    Ok(response)
}
