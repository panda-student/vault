//! Transactional persistence port. Authorization is evaluated against transaction-local settings.
use crate::definitions::{credentials::*, errors::Result, organization::*};
use std::path::Path;
use zeroize::Zeroizing;
pub(crate) trait MutationPolicy {
    fn prepare(&self, value: &mut EntryInput) -> Result<()>;
    fn patch(&self, current: EntryInput, patch: EntryPatch) -> Result<EntryInput>;
    fn authorize(
        &self,
        settings: &Settings,
        old: Option<&EntrySummary>,
        new: &EntrySummary,
    ) -> Result<()>;
}
pub(crate) trait CredentialReader {
    fn get(&self, id: &str) -> Result<EntryDetail>;
}
pub(crate) trait VaultRepository: CredentialReader + Send + Sync {
    fn directory(&self) -> &Path;
    fn list(&self, query: &str, kind: Option<Kind>) -> Result<Vec<EntrySummary>>;
    fn create_many(
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
    fn protected_values(&self) -> Result<Zeroizing<Vec<String>>>;
}
