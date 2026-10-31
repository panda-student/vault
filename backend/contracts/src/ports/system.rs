//! Capabilities injected by bootstrap. Business services do not import native implementations.
use crate::{
    errors::Result,
    filling::{CaptureRequest, FillStatus},
    operations::ClipboardPort,
    updates::UpdateLaunch,
};
use std::{path::Path, process::Child, sync::Arc};
pub trait PauseLease: Send {
    fn token(&self) -> &str;
    fn handoff(self: Box<Self>);
}
pub trait DesktopProcess {
    fn directory(&self) -> &Path;
    fn exited(&self) -> bool;
    fn close(&self) -> Result<()>;
}
pub trait SystemPlatform: Send + Sync {
    fn capture_target(&self, request: CaptureRequest) -> Result<FillStatus>;
    fn fill_target(
        &self,
        target: &str,
        resolve: Box<dyn FnOnce() -> Result<String> + '_>,
    ) -> Result<FillStatus>;
    fn replace_file(&self, destination: &Path, replacement: &Path) -> Result<()>;
    fn bind_clipboard(&self, port: Arc<dyn ClipboardPort>) -> Result<()>;
    fn copy_text(&self, text: String) -> Result<()>;
    fn open_url(&self, url: &str) -> Result<()>;
    fn startup_enabled(&self, executable: &Path, directory: &Path) -> Result<bool>;
    fn set_startup(&self, executable: &Path, directory: &Path, enabled: bool) -> Result<bool>;
    fn confirm_purge(&self, owner: isize, count: usize) -> Result<bool>;
    fn launch_update(&self, launch: UpdateLaunch) -> Result<()>;
    fn begin_pause(&self, executable: &Path) -> Result<Box<dyn PauseLease>>;
    fn wait_for_process(&self, id: u32) -> Result<()>;
    fn start_updated(
        &self,
        executable: &Path,
        directory: &Path,
        health: Option<(&Path, &str)>,
    ) -> Result<Child>;
    fn desktops(&self, executable: &Path) -> Result<Vec<Box<dyn DesktopProcess>>>;
}
