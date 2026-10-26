use crate::app::AppState;
use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use vault_core::api::{AppError, CoreSession, Result};
fn call(core: &CoreSession, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let reply = core
        .handle_rpc(serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .ok_or_else(AppError::invalid)?;
    if reply.get("error").is_some() {
        return Err(AppError::invalid());
    }
    Ok(reply["result"].clone())
}
#[tauri::command]
pub(crate) fn smoke_clipboard(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Option<bool>> {
    if state.smoke_report.is_none() {
        return Err(AppError::invalid());
    }
    // Keep any existing text entirely in memory, never in test artifacts. If
    // the clipboard holds an unsupported format, leave it untouched.
    let original = match app.clipboard().read_text() {
        Ok(value) => Some(value),
        Err(_) if clipboard_is_empty() => None,
        Err(_) => return Ok(None),
    };
    let entry = call(&state.core, "credentials.get", serde_json::json!({"id":id}))?;
    let params = serde_json::json!({"id":id,"expectedRevision":entry["revision"],"field":"secret"});
    let secret = call(
        &state.core,
        "credentials.readSecret",
        serde_json::json!({"id":id,"expectedRevision":entry["revision"]}),
    )?;
    let secret = secret.as_str().ok_or_else(AppError::invalid)?;
    let written = call(&state.core, "clipboard.copyField", params);
    let matches = written.is_ok()
        && app
            .clipboard()
            .read_text()
            .is_ok_and(|value| value == secret);
    let restored = match original {
        Some(value) => app.clipboard().write_text(value),
        None => app.clipboard().clear(),
    };
    restored.map_err(|_| AppError::new("CLIPBOARD_ERROR", "剪贴板恢复失败。"))?;
    Ok(Some(matches))
}

fn clipboard_is_empty() -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(window: *mut std::ffi::c_void) -> i32;
        fn CountClipboardFormats() -> u32;
        fn CloseClipboard() -> i32;
    }
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        let empty = CountClipboardFormats() == 0;
        CloseClipboard();
        empty
    }
}

#[tauri::command]
pub(crate) async fn smoke_window_cycle(app: tauri::AppHandle) -> Result<bool> {
    if app.state::<AppState>().smoke_report.is_none() {
        return Err(AppError::invalid());
    }
    let window = app.get_webview_window("main").ok_or(AppError::invalid())?;
    window.close().map_err(|_| AppError::invalid())?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let hidden = !window.is_visible().map_err(|_| AppError::invalid())?;
    window.show().map_err(|_| AppError::invalid())?;
    Ok(hidden
        && window.is_visible().map_err(|_| AppError::invalid())?
        && app.tray_by_id("main").is_some())
}

#[tauri::command]
pub(crate) fn smoke_resize(app: tauri::AppHandle, width: u32, height: u32) -> Result<()> {
    if app.state::<AppState>().smoke_report.is_none()
        || !matches!((width, height), (1120, 740) | (900, 580))
    {
        return Err(AppError::invalid());
    }
    app.get_webview_window("main")
        .ok_or(AppError::invalid())?
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|_| AppError::invalid())
}

#[tauri::command]
pub(crate) fn finish_smoke(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    report: serde_json::Value,
) -> Result<()> {
    let destination = state.smoke_report.as_ref().ok_or(AppError::invalid())?;
    std::fs::write(
        destination,
        serde_json::to_vec_pretty(&report).map_err(|_| AppError::invalid())?,
    )
    .map_err(|_| AppError::storage())?;
    state.smoke_finished.store(true, Ordering::SeqCst);
    app.exit(
        if report.get("passed").and_then(|v| v.as_bool()) == Some(true) {
            0
        } else {
            1
        },
    );
    Ok(())
}
