use super::*;
/// Desktop and MCP workers hold a shared lease before opening the vault.
/// Connection bridges stay alive while their workers drain and release leases.
pub struct UpdateRuntime {
    pub(crate) executable: PathBuf,
    pub(crate) lease: Mutex<File>,
    pub(crate) cache: PathBuf,
    pub(crate) pause: Mutex<Option<String>>,
}
impl UpdateRuntime {
    pub fn acquire(executable: &Path) -> Result<Arc<Self>> {
        let (lock, cache) = runtime_paths(executable)?;
        fs::create_dir_all(lock.parent().ok_or_else(invalid_update)?)
            .map_err(|_| AppError::storage())?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)
            .map_err(|_| AppError::storage())?;
        FileExt::try_lock_shared(&file).map_err(|_| {
            failure(
                "UPDATE_IN_PROGRESS",
                "程序正在更新，请等待更新完成后再启动。",
            )
        })?;
        Ok(Arc::new(Self {
            executable: executable.to_owned(),
            lease: Mutex::new(file),
            cache,
            pause: Mutex::new(None),
        }))
    }
    pub(crate) fn claim(&self) -> Result<()> {
        let file = self.lease.lock().map_err(|_| busy())?;
        FileExt::unlock(&*file).map_err(|_| AppError::storage())?;
        if FileExt::try_lock_exclusive(&*file).is_err() {
            FileExt::try_lock_shared(&*file).map_err(|_| busy())?;
            return Err(failure(
                "UPDATE_CLIENTS_ACTIVE",
                "仍有其他桌面窗口或旧版 MCP 占用程序。请退出其他窗口；旧版 MCP 首次切换需重连一次，新版 MCP 会自动暂停和恢复，无需退出 AI 客户端。",
            ));
        }
        Ok(())
    }
    pub(crate) fn release_claim(&self) {
        if let Ok(file) = self.lease.lock() {
            let _ = FileExt::unlock(&*file);
            let _ = FileExt::try_lock_shared(&*file);
        }
    }
}
pub(crate) fn runtime_paths(executable: &Path) -> Result<(PathBuf, PathBuf)> {
    let base = crate::common::paths::coordination_directory(executable)?;
    Ok((base.join("sessions.lock"), base.join("cache")))
}
