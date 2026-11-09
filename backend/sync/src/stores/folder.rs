//! A local or cloud-drive folder (OneDrive, Dropbox, Nutstore client, NAS share).
use super::{MAX_REMOTE_BYTES, remote_changed, segments, too_large};
use crate::sha_hex;
use std::path::{Path, PathBuf};
use vault_contracts::{
    errors::{AppError, Result},
    ports::sync::{RemoteObject, RemoteState, RemoteStore},
};

pub struct FolderStore {
    file: PathBuf,
}

impl FolderStore {
    pub fn new(directory: &Path, path: &str) -> Result<Self> {
        if !directory.is_absolute() {
            return Err(AppError::invalid().with("directory", "请选择绝对路径的文件夹。"));
        }
        let mut file = directory.to_path_buf();
        file.extend(segments(path)?);
        Ok(Self { file })
    }
    fn current_version(&self) -> Result<Option<String>> {
        Ok(self.read()?.map(|object| object.version))
    }
    /// Write `bytes` to a durable temporary file next to the sync file.
    fn stage(&self, bytes: &[u8]) -> Result<PathBuf> {
        use std::io::Write;
        let parent = self.file.parent().ok_or_else(io_error)?;
        let temporary = parent.join(format!(".vault-sync-{}.tmp", uuid::Uuid::new_v4().simple()));
        let written = std::fs::File::create(&temporary).and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
            return Err(io_error());
        }
        Ok(temporary)
    }
    /// Put the staged file in place if the remote is still at `expected`.
    fn commit(&self, temporary: &Path, expected: Option<&str>) -> Result<()> {
        match expected {
            // Creating: a hard link fails if another device created the file meanwhile.
            None => match std::fs::hard_link(temporary, &self.file) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    Err(remote_changed())
                }
                // Some network drives do not support hard links.
                Err(_) if !self.file.exists() => {
                    std::fs::rename(temporary, &self.file).map_err(|_| io_error())
                }
                Err(_) => Err(remote_changed()),
            },
            // Best effort: the check and the rename are not atomic across processes.
            Some(version) => {
                if self.current_version()?.as_deref() != Some(version) {
                    return Err(remote_changed());
                }
                std::fs::rename(temporary, &self.file).map_err(|_| io_error())
            }
        }
    }
}

impl RemoteStore for FolderStore {
    fn read(&self) -> Result<Option<RemoteObject>> {
        match std::fs::metadata(&self.file) {
            Ok(meta) if meta.len() > MAX_REMOTE_BYTES => return Err(too_large()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(io_error()),
        }
        match std::fs::read(&self.file) {
            Ok(bytes) => Ok(Some(RemoteObject {
                version: sha_hex(&bytes),
                bytes,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(io_error()),
        }
    }

    /// A cloud-drive client may still create its own conflict copies.
    fn write(&self, bytes: &[u8], expected: Option<&str>) -> Result<String> {
        let temporary = self.stage(bytes)?;
        let committed = self.commit(&temporary, expected);
        // After a hard link or a failure the temporary file is left over.
        let _ = std::fs::remove_file(&temporary);
        committed.map(|()| sha_hex(bytes))
    }

    fn prepare(&self, create: bool) -> Result<RemoteState> {
        let parent = self.file.parent().ok_or_else(io_error)?;
        let mut created = false;
        if !parent.is_dir() {
            if !create {
                return Err(AppError::new(
                    "SYNC_NOT_FOUND",
                    "同步文件夹不存在，请检查路径。",
                ));
            }
            std::fs::create_dir_all(parent).map_err(|_| io_error())?;
            created = true;
        }
        Ok(RemoteState {
            exists: self.file.is_file(),
            created,
        })
    }
}

fn io_error() -> AppError {
    AppError::new("SYNC_IO", "无法读写同步文件夹，请检查路径和权限。")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_only_its_own_file_and_detects_concurrent_changes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other.txt"), b"keep me").unwrap();
        let store = FolderStore::new(dir.path(), "vault/s.vaultsync").unwrap();
        assert_eq!(store.prepare(false).unwrap_err().code, "SYNC_NOT_FOUND");
        assert_eq!(
            store.prepare(true).unwrap(),
            RemoteState {
                exists: false,
                created: true
            }
        );
        assert!(store.read().unwrap().is_none());
        let first = store.write(b"one", None).unwrap();
        assert_eq!(
            store.write(b"again", None).unwrap_err().code,
            "SYNC_REMOTE_CHANGED"
        );
        let second = store.write(b"two", Some(&first)).unwrap();
        assert_eq!(store.read().unwrap().unwrap().bytes, b"two");
        assert_eq!(
            store.write(b"three", Some(&first)).unwrap_err().code,
            "SYNC_REMOTE_CHANGED"
        );
        assert_eq!(store.read().unwrap().unwrap().version, second);
        assert!(store.prepare(false).unwrap().exists);
        // Nothing else in the folder was touched, and no temp files remain.
        assert_eq!(
            std::fs::read(dir.path().join("other.txt")).unwrap(),
            b"keep me"
        );
        let names: Vec<_> = std::fs::read_dir(dir.path().join("vault"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["s.vaultsync"]);
    }

    #[test]
    fn rejects_relative_directories_and_escaping_paths() {
        assert!(FolderStore::new(Path::new("relative"), "a.vaultsync").is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(FolderStore::new(dir.path(), "../a.vaultsync").is_err());
    }
}
