//! Remote stores. Each one reads and writes a single configured file and never
//! lists, changes or removes anything else at the remote.
mod folder;
mod github;
mod http;
mod webdav;

pub use folder::FolderStore;
pub use github::GithubStore;
pub use webdav::WebdavStore;

use vault_contracts::{
    errors::{AppError, Result},
    ports::sync::{RemoteStore, RemoteStoreFactory},
    sync::ResolvedTarget,
};

/// Production factory injected by bootstrap.
#[derive(Default)]
pub struct Stores;

impl RemoteStoreFactory for Stores {
    fn open(&self, target: &ResolvedTarget) -> Result<Box<dyn RemoteStore>> {
        Ok(match target {
            ResolvedTarget::Folder { directory, path } => {
                Box::new(FolderStore::new(directory, path)?)
            }
            ResolvedTarget::Webdav {
                url,
                path,
                username,
                password,
            } => Box::new(WebdavStore::new(url, path, username, password)?),
            ResolvedTarget::Github {
                owner,
                repo,
                branch,
                path,
                token,
            } => Box::new(GithubStore::new(
                owner,
                repo,
                branch.as_deref(),
                path,
                token,
            )?),
        })
    }
}

/// Split a relative file path into safe segments: no absolute paths, no `..`.
pub(crate) fn segments(path: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = path
        .split(['/', '\\'])
        .map(str::trim)
        .map(String::from)
        .collect();
    let invalid = parts.is_empty()
        || parts
            .iter()
            .any(|p| p.is_empty() || p == "." || p == ".." || p.contains(':'));
    if invalid || path.len() > 512 {
        return Err(AppError::invalid().with(
            "path",
            "同步文件路径须是相对路径，例如 vault/vault-sync.vaultsync。",
        ));
    }
    Ok(parts)
}

/// Larger remote files are refused before reading: the sync file itself is
/// capped at 32 MB, so anything this big is not ours.
pub(crate) const MAX_REMOTE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn too_large() -> AppError {
    AppError::new(
        "SYNC_TOO_LARGE",
        "同步路径上的文件过大，不是本程序的同步文件，未做任何改动。",
    )
}

pub(crate) fn remote_changed() -> AppError {
    AppError::new(
        "SYNC_REMOTE_CHANGED",
        "远端同步文件刚被其他设备修改，请重新同步。",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_plain_relative_paths_are_accepted() {
        assert_eq!(
            segments("vault/a.vaultsync").unwrap(),
            ["vault", "a.vaultsync"]
        );
        for bad in ["", "/abs", "a//b", "../x", "a/./b", "C:/x", "a/.."] {
            assert!(segments(bad).is_err(), "{bad}");
        }
    }
}
