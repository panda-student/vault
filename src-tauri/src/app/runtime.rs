use super::AppState;
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::Ordering;
use tauri::Manager;
use vault_core::api::DesktopMetadata;
pub(super) fn desktop(
    state: AppState,
    background: bool,
    headless: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if tauri::webview_version().is_err() {
        return Err("需要 Microsoft Edge WebView2 Runtime，请从 https://developer.microsoft.com/microsoft-edge/webview2/ 获取。".into());
    }
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state);
    #[cfg(feature = "test-support")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        crate::commands::rpc,
        crate::testing::desktop::smoke_clipboard,
        crate::testing::desktop::smoke_window_cycle,
        crate::testing::desktop::smoke_resize,
        crate::testing::desktop::finish_smoke
    ]);
    #[cfg(not(feature = "test-support"))]
    let builder = builder.invoke_handler(tauri::generate_handler![crate::commands::rpc]);
    builder
        .setup(move |app| {
            let result: std::result::Result<(), Box<dyn std::error::Error>> = (|| {
                let state = app.state::<AppState>();
                state
                    .core
                    .bind_clipboard(Arc::new(super::clipboard::DesktopClipboard(
                        app.handle().clone(),
                    )))?;
                let is_smoke = state.smoke_report.is_some();
                state.core.bind_desktop_metadata(DesktopMetadata {
                    webview_version: tauri::webview_version().unwrap_or_default(),
                    smoke_test: is_smoke,
                    owner_window: 0,
                })?;
                if !headless {
                    super::window::open_main(app.handle(), !background)?;
                }
                super::tray::create(app)?;
                // A normal/health startup creates its window before the pause-aware
                // listener starts. Otherwise an in-progress update could stop the
                // host as headless just before it writes the health marker.
                state
                    .backend
                    .start(app.handle().clone(), state.core.clone())?;
                if let Some((path, token)) = &state.update_health {
                    std::fs::write(path, token)?;
                }
                #[cfg(feature = "test-support")]
                if is_smoke {
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(180));
                        let state = handle.state::<AppState>();
                        if !state.smoke_finished.load(Ordering::SeqCst) {
                            if let Some(path) = &state.smoke_report {
                                let _ = std::fs::write(
                                    path,
                                    r#"{"passed":false,"error":"Desktop smoke test timed out"}"#,
                                );
                            }
                            handle.exit(1);
                        }
                    });
                }
                Ok(())
            })();
            // A failed update health probe is an expected rollback condition.
            // Returning Err here makes Tauri panic during RuntimeReady; on Windows
            // crash recovery can relaunch an untracked child with the health args.
            if result.is_err() && app.state::<AppState>().update_health.is_some() {
                app.handle().exit(1);
                return Ok(());
            }
            result
        })
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // Closing the desktop window does not interrupt a connected MCP client.
            // Explicit tray quit and update exits retain their explicit exit code.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                if app.state::<AppState>().backend.has_clients() {
                    api.prevent_exit();
                }
            }
        });
    Ok(())
}
