//! Transport adapter only. Caller identity is already bound to AppState.core.
use crate::app::{AppState, window};
use tauri::Manager;
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

/// Brings the desktop window forward for an AI export request that needs a passphrase.
#[tauri::command]
pub(crate) fn export_attention(
    app: tauri::AppHandle,
    active: bool,
) -> std::result::Result<(), String> {
    let Some(main) = app.get_webview_window("main") else {
        return Ok(());
    };
    if active {
        window::show_main(&app);
        main.set_always_on_top(true)
            .map_err(|error| error.to_string())?;
        main.request_user_attention(Some(tauri::UserAttentionType::Critical))
            .map_err(|error| error.to_string())?;
    } else {
        main.set_always_on_top(false)
            .map_err(|error| error.to_string())?;
        main.request_user_attention(None)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
