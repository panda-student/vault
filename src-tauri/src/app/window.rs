use super::AppState;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use vault_core::api::DesktopMetadata;
pub(crate) fn open_main(
    app: &tauri::AppHandle,
    visible: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        if visible {
            window.show()?;
            window.unminimize()?;
            window.set_focus()?;
        }
        return Ok(());
    }
    let state = app.state::<AppState>();
    let is_smoke = state.smoke_report.is_some();
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("本地密码库")
        .window_classname("vault.desktop")
        .inner_size(1120.0, 740.0)
        .min_inner_size(900.0, 580.0)
        .focused(true)
        .visible(visible)
        .data_directory(state.core.directory().join("webview"));
    // A debug build can expose only its explicitly selected fixture window
    // for native UI inspection. Release builds do not include this hook.
    #[cfg(debug_assertions)]
    let builder = if let Ok(port) = std::env::var("VAULT_UI_DEBUG_PORT") {
        let port = port.parse::<u16>().map_err(|_| "Invalid UI debug port")?;
        builder.additional_browser_args(&format!("--remote-debugging-port={port}"))
    } else {
        builder
    };
    let window = builder.build()?;
    let owner = window.hwnd().map_err(|_| "No native window")?.0 as isize;
    state.core.bind_desktop_metadata(DesktopMetadata {
        webview_version: tauri::webview_version().unwrap_or_default(),
        smoke_test: is_smoke,
        owner_window: owner,
    })?;
    let observed = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event
            && observed
                .state::<AppState>()
                .core
                .handle_rpc(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"settings.get"}))
                .is_some_and(|reply| reply["result"]["closeToTray"].as_bool() == Some(true))
        {
            api.prevent_close();
            let _ = observed.hide();
        }
    });
    Ok(())
}
pub(crate) fn show_main(app: &tauri::AppHandle) {
    let _ = open_main(app, true);
}
