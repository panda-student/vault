//! Concrete Windows bindings for the backend capability ports.
use super::*;
use crate::definitions::ports::system::{DesktopProcess, PauseLease, SystemPlatform};
use std::{path::Path, process::Child, sync::Arc};
impl PauseLease for crate::platform::runtime::Pause {
    fn token(&self) -> &str {
        &self.token
    }
    fn handoff(self: Box<Self>) {
        crate::platform::runtime::Pause::handoff(*self)
    }
}
impl DesktopProcess for processes::Desktop {
    fn directory(&self) -> &Path {
        &self.directory
    }
    fn exited(&self) -> bool {
        processes::Desktop::exited(self)
    }
    fn close(&self) -> Result<()> {
        processes::close(std::slice::from_ref(self))
    }
}
impl SystemPlatform for WindowsService {
    fn capture_target(&self, r: CaptureRequest) -> Result<FillStatus> {
        WindowsService::capture_target(self, r)
    }
    fn fill_target(
        &self,
        t: &str,
        r: Box<dyn FnOnce() -> Result<String> + '_>,
    ) -> Result<FillStatus> {
        WindowsService::fill_target(self, t, r)
    }
    fn replace_file(&self, d: &Path, r: &Path) -> Result<()> {
        WindowsService::replace_file(self, d, r)
    }
    fn bind_clipboard(&self, p: Arc<dyn ClipboardPort>) -> Result<()> {
        WindowsService::bind_clipboard(self, p)
    }
    fn copy_text(&self, t: String) -> Result<()> {
        WindowsService::copy_text(self, t)
    }
    fn open_url(&self, u: &str) -> Result<()> {
        WindowsService::open_url(self, u)
    }
    fn startup_enabled(&self, e: &Path, d: &Path) -> Result<bool> {
        WindowsService::startup_enabled(self, e, d)
    }
    fn set_startup(&self, e: &Path, d: &Path, b: bool) -> Result<bool> {
        WindowsService::set_startup(self, e, d, b)
    }
    fn confirm_purge(&self, o: isize, c: usize) -> Result<bool> {
        WindowsService::confirm_purge(self, o, c)
    }
    fn launch_update(&self, l: UpdateLaunch) -> Result<()> {
        WindowsService::launch_update(self, l)
    }
    fn begin_pause(&self, e: &Path) -> Result<Box<dyn PauseLease>> {
        Ok(Box::new(crate::platform::runtime::Pause::begin(e)?))
    }
    fn adopt_pause(&self, e: &Path, t: &str) -> Result<Box<dyn PauseLease>> {
        Ok(Box::new(crate::platform::runtime::Pause::adopt(e, t)?))
    }
    fn wait_for_process(&self, id: u32) -> Result<()> {
        super::wait_for_process(id)
    }
    fn start_updated(&self, e: &Path, d: &Path, h: Option<(&Path, &str)>) -> Result<Child> {
        super::start_updated(e, d, h)
    }
    fn desktops(&self, e: &Path) -> Result<Vec<Box<dyn DesktopProcess>>> {
        Ok(processes::desktops(e)?
            .into_iter()
            .map(|v| Box::new(v) as Box<dyn DesktopProcess>)
            .collect())
    }
}
