//! Remote sync data shapes shared by storage, the sync module and core.
use super::credentials::EntryInput;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Default location of this program's sync file inside the chosen remote.
/// The remote may hold other files; only this one path is ever read or written.
pub const DEFAULT_PATH: &str = "vault/vault-sync.vaultsync";
fn default_path() -> String {
    DEFAULT_PATH.into()
}
/// Minutes between automatic syncs. 0 syncs only on demand and after edits.
pub const DEFAULT_INTERVAL: u32 = 15;
pub const MAX_INTERVAL: u32 = 24 * 60;

/// A credential stored in the vault whose group supplies the remote's secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialRef {
    pub credential_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

/// Where the sync file lives. Secrets are referenced, never stored here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum SyncTarget {
    Folder {
        directory: String,
        #[serde(default = "default_path")]
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    Webdav {
        url: String,
        #[serde(default = "default_path")]
        path: String,
        credential: CredentialRef,
    },
    #[serde(rename_all = "camelCase")]
    Github {
        owner: String,
        repo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        #[serde(default = "default_path")]
        path: String,
        token: CredentialRef,
    },
}
impl SyncTarget {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Folder { .. } => "folder",
            Self::Webdav { .. } => "webdav",
            Self::Github { .. } => "github",
        }
    }
    pub fn credential(&self) -> Option<&CredentialRef> {
        match self {
            Self::Folder { .. } => None,
            Self::Webdav { credential, .. } => Some(credential),
            Self::Github { token, .. } => Some(token),
        }
    }
    /// Human-readable location without any secret.
    pub fn summary(&self) -> String {
        match self {
            Self::Folder { directory, path } => format!("{directory}\\{path}"),
            Self::Webdav { url, path, .. } => format!("{}/{path}", url.trim_end_matches('/')),
            Self::Github {
                owner, repo, path, ..
            } => format!("github.com/{owner}/{repo}/{path}"),
        }
    }
}

/// A target with its secrets resolved from the vault. Never serialized.
pub enum ResolvedTarget {
    Folder {
        directory: std::path::PathBuf,
        path: String,
    },
    Webdav {
        url: String,
        path: String,
        username: String,
        password: Zeroizing<String>,
    },
    Github {
        owner: String,
        repo: String,
        branch: Option<String>,
        path: String,
        token: Zeroizing<String>,
    },
}

/// One entry as carried in the encrypted remote snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRecord {
    pub id: String,
    pub value: EntryInput,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// Set while the entry is in the trash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    /// Device that last changed this record, shown when resolving conflicts.
    #[serde(default)]
    pub device: String,
}
impl SyncRecord {
    /// Latest moment this record changed, including moving it to the trash.
    pub fn changed_at(&self) -> i64 {
        self.updated_at.max(self.deleted_at.unwrap_or(0))
    }
}

/// One local write that makes this computer match the merged result.
#[derive(Debug, Clone)]
pub enum LocalChange {
    /// Insert or replace the local entry, including its trash state.
    Upsert(SyncRecord),
    /// Remove the entry for good.
    Purge(String),
}
impl LocalChange {
    pub fn id(&self) -> &str {
        match self {
            Self::Upsert(record) => &record.id,
            Self::Purge(id) => id,
        }
    }
}

/// Where a local entry lived when the snapshot was taken. A write is skipped
/// when the entry moved or changed since, so a concurrent local edit survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncSlot {
    pub revision: i64,
    pub trashed: bool,
}

pub struct SyncSnapshot {
    /// Device is left empty; the caller knows which computer this is.
    pub records: Vec<SyncRecord>,
    pub slots: std::collections::BTreeMap<String, SyncSlot>,
}

/// How to settle entries both sides changed, or a first join with data on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncChoice {
    /// The local version replaces the remote one.
    LocalWins,
    /// The remote version replaces the local one.
    RemoteWins,
    /// Fill in both sides; the newer version keeps the id, the other becomes a copy.
    TwoWay,
}

/// A conflict as shown to the user: metadata only, never secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub id: String,
    pub name: String,
    pub local_changed_at: Option<i64>,
    pub remote_changed_at: Option<i64>,
    pub local_device: String,
    pub remote_device: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    /// Records the remote received or changed.
    pub pushed: usize,
    /// Records added or changed locally.
    pub pulled: usize,
    /// Records removed for good on either side.
    pub deleted: usize,
    /// Conflicts still waiting for the user's choice.
    pub conflicts: usize,
}

/// Desktop request to set up or change sync. The passphrase is typed by the
/// user on the desktop; MCP never sends one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncSetup {
    pub target: SyncTarget,
    /// Required for a new target; omitted keeps the saved one.
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub interval_minutes: Option<u32>,
    #[serde(default)]
    pub device_name: Option<String>,
    /// Create the missing folder, collection or private repository.
    #[serde(default)]
    pub create: bool,
}

/// What an MCP caller may change. A new target waits for the desktop user to
/// confirm and type the passphrase; interval and device name apply directly.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncPatch {
    /// `false` stops syncing; no data is removed anywhere. Same as `sync: null`,
    /// for clients that drop null arguments.
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub target: Option<SyncTarget>,
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub interval_minutes: Option<u32>,
    #[serde(default)]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncError {
    pub code: String,
    pub message: String,
}

/// Sync state shown to the desktop and to MCP. Never holds a secret.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusView {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<SyncTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_name: Option<String>,
    pub interval_minutes: u32,
    pub device_name: String,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<i64>,
    pub last_outcome: SyncOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SyncError>,
    /// Entries waiting for the user's choice on the desktop.
    pub conflicts: Vec<SyncConflict>,
    /// The conflicts come from a first join: the choice applies to the whole vault.
    pub first_join: bool,
}

/// A target change asked for over MCP, waiting for the desktop user.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequestView {
    pub id: String,
    pub kind: &'static str,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_name: Option<String>,
    pub create: bool,
    pub requested_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRespondArgs {
    pub id: String,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub cancel: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncResolveArgs {
    pub choice: SyncChoice,
}
