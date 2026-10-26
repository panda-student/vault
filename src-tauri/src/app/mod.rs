//! Tauri-only composition: windows, tray and process lifecycle.
mod clipboard;
mod errors;
pub(crate) mod host;
mod launch;
mod runtime;
mod startup;
mod tray;
pub(crate) mod window;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
use vault_backend::api::CoreSession;
pub(crate) struct AppState {
    pub(crate) core: Arc<CoreSession>,
    pub(crate) smoke_report: Option<PathBuf>,
    pub(crate) smoke_finished: AtomicBool,
    pub(crate) update_health: Option<(PathBuf, String)>,
    pub(crate) backend: host::Host,
}

pub(crate) fn run() {
    startup::main_entry();
}
