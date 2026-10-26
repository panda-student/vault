//! Tauri-only composition: windows, tray and process lifecycle.
#[cfg(feature = "test-support")]
use std::sync::atomic::AtomicBool;
mod clipboard;
mod errors;
pub(crate) mod host;
mod launch;
mod runtime;
mod startup;
mod tray;
pub(crate) mod window;
use std::{path::PathBuf, sync::Arc};
use vault_core::api::CoreSession;
pub(crate) struct AppState {
    pub(crate) core: Arc<CoreSession>,
    pub(crate) smoke_report: Option<PathBuf>,
    #[cfg(feature = "test-support")]
    pub(crate) smoke_finished: AtomicBool,
    pub(crate) update_health: Option<(PathBuf, String)>,
    pub(crate) backend: host::Host,
}

pub(crate) fn run() {
    startup::main_entry();
}
