//! Remote storage for the encrypted sync file. One implementation per backend.
use crate::{errors::Result, sync::ResolvedTarget};

/// The sync file's bytes (always encrypted) and an opaque version used for
/// conditional writes.
#[derive(Debug)]
pub struct RemoteObject {
    pub bytes: Vec<u8>,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteState {
    /// The sync file already exists at the configured path.
    pub exists: bool,
    /// `prepare` created the container (folder, collection or repository).
    pub created: bool,
}

/// A store touches exactly one file; other files at the remote are never listed,
/// changed or removed.
pub trait RemoteStore: Send + Sync {
    /// `None` when the sync file does not exist yet.
    fn read(&self) -> Result<Option<RemoteObject>>;
    /// Write only if the remote is still at `expected` (`None`: must not exist).
    /// A mismatch fails with `SYNC_REMOTE_CHANGED`. Returns the new version.
    fn write(&self, bytes: &[u8], expected: Option<&str>) -> Result<String>;
    /// Check access; with `create`, create the missing container.
    fn prepare(&self, create: bool) -> Result<RemoteState>;
}

pub trait RemoteStoreFactory: Send + Sync {
    fn open(&self, target: &ResolvedTarget) -> Result<Box<dyn RemoteStore>>;
}
