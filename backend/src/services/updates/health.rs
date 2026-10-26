//! Recovery must own a ready desktop before retained MCP bridges resume.
use crate::definitions::{
    errors::{AppError, Result},
    ports::system::SystemPlatform,
};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

/// Start the restored image and confirm its window and listener are initialized.
/// A fresh recovery directory cannot collide with the failed health marker.
pub(super) fn restart_restored(
    platform: &dyn SystemPlatform,
    executable: &Path,
    directory: &Path,
) -> Result<()> {
    let parent = executable.parent().ok_or_else(AppError::storage)?;
    let stage = parent.join(format!(".vault-update-recovery-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage).map_err(|_| AppError::storage())?;
    let health = stage.join("healthy");
    let token = uuid::Uuid::new_v4().to_string();
    let mut child = platform.start_updated(executable, directory, Some((&health, &token)))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if fs::read_to_string(&health).is_ok_and(|value| value == token) {
            let _ = fs::remove_file(&health);
            let _ = fs::remove_dir(&stage);
            return Ok(());
        }
        if child.try_wait().map_err(|_| AppError::storage())?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(AppError::new(
        "UPDATE_RECOVERY_START",
        "旧程序文件已恢复，但未能确认界面启动。请重新打开程序。",
    ))
}
