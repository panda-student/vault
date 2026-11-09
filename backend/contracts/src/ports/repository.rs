//! Transactional persistence port. Authorization is evaluated against transaction-local settings.
use crate::{credentials::*, errors::Result, organization::*};
use std::path::Path;
use zeroize::Zeroizing;
pub trait MutationPolicy {
    fn prepare(&self, value: &mut EntryInput) -> Result<()>;
    fn patch(&self, current: EntryInput, patch: EntryPatch) -> Result<EntryInput>;
    fn authorize(
        &self,
        settings: &Settings,
        old: Option<&EntrySummary>,
        new: &EntrySummary,
    ) -> Result<()>;
}
pub trait CredentialReader {
    fn get(&self, id: &str) -> Result<EntryDetail>;
}
pub trait VaultRepository: CredentialReader + Send + Sync {
    fn directory(&self) -> &Path;
    fn list(&self, query: &str, kind: Option<Kind>) -> Result<Vec<EntrySummary>>;
    fn create_many(
        &self,
        values: Vec<EntryInput>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>>;
    fn export_entries(&self, include_trash: bool) -> Result<Vec<EntryInput>>;
    fn import_many(
        &self,
        values: Vec<EntryInput>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>>;
    fn update_many(
        &self,
        values: Vec<EntryPatch>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>>;
    fn delete_many(
        &self,
        values: Vec<EntryVersion>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<String>>;
    fn pin(&self, id: &str, revision: i64, pinned: bool) -> Result<EntrySummary>;
    fn organization(&self) -> Result<Organization>;
    fn organize(&self, kind: &str, name: &str, previous: Option<&str>) -> Result<()>;
    fn list_trash(&self) -> Result<Vec<TrashEntry>>;
    fn restore(&self, id: &str, revision: i64) -> Result<EntrySummary>;
    fn empty_trash(&self, expected: Vec<EntryVersion>) -> Result<usize>;
    fn settings(&self) -> Result<Settings>;
    fn save_settings(&self, settings: &Settings) -> Result<()>;
    /// Atomically change only `close_to_tray`; returns the previous value.
    fn set_close_to_tray(&self, value: bool) -> Result<bool>;
    /// Every entry, live and trashed, with secrets, for the sync file.
    fn sync_snapshot(&self) -> Result<crate::sync::SyncSnapshot>;
    /// Apply merged changes in one transaction. A change whose entry no longer
    /// matches `expected` (edited, moved or created since) is skipped; the
    /// skipped ids are returned.
    fn apply_sync(
        &self,
        changes: Vec<crate::sync::LocalChange>,
        expected: &std::collections::BTreeMap<String, crate::sync::SyncSlot>,
    ) -> Result<Vec<String>>;
    /// Encrypted private state of the sync feature (config, base, status).
    fn sync_blob(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>>;
    /// `None` removes the blob.
    fn store_sync_blob(&self, name: &str, bytes: Option<&[u8]>) -> Result<()>;
    fn protected_values(&self) -> Result<Zeroizing<Vec<String>>>;
}

/// Metadata-only view consumed by authorization; never exposes a secret field.
pub trait CredentialCatalog {
    fn summary(&self, id: &str) -> Result<EntrySummary>;
    fn list(&self, query: &str, kind: Option<Kind>) -> Result<Vec<EntrySummary>>;
    fn list_trash(&self) -> Result<Vec<TrashEntry>>;
    fn organization(&self) -> Result<Organization>;
    fn settings(&self) -> Result<Settings>;
}
/// Bound by core, evaluated inside the repository's write transaction.
pub trait ChangeAuthorizer {
    fn authorize(
        &self,
        settings: &Settings,
        old: Option<&EntrySummary>,
        new: &EntrySummary,
    ) -> Result<()>;
}
