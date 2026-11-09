use super::super::*;
use vault_contracts::ports::repository::{CredentialReader, VaultRepository};
impl CredentialReader for Vault {
    fn get(&self, id: &str) -> Result<EntryDetail> {
        Vault::get(self, id)
    }
}
impl VaultRepository for Vault {
    fn directory(&self) -> &Path {
        Vault::directory(self)
    }
    fn list(&self, q: &str, k: Option<Kind>) -> Result<Vec<EntrySummary>> {
        Vault::list(self, q, k)
    }
    fn create_many(&self, v: Vec<EntryInput>, p: &dyn MutationPolicy) -> Result<Vec<EntrySummary>> {
        self.create_many_checked(v, p)
    }
    fn export_entries(&self, include_trash: bool) -> Result<Vec<EntryInput>> {
        Vault::export_entries(self, include_trash)
    }
    fn import_many(&self, v: Vec<EntryInput>, p: &dyn MutationPolicy) -> Result<Vec<EntrySummary>> {
        Vault::import_many(self, v, p)
    }
    fn update_many(&self, v: Vec<EntryPatch>, p: &dyn MutationPolicy) -> Result<Vec<EntrySummary>> {
        self.update_many_checked(v, p)
    }
    fn delete_many(&self, v: Vec<EntryVersion>, p: &dyn MutationPolicy) -> Result<Vec<String>> {
        self.delete_many_checked(v, p)
    }
    fn pin(&self, id: &str, r: i64, p: bool) -> Result<EntrySummary> {
        Vault::pin(self, id, r, p)
    }
    fn organization(&self) -> Result<Organization> {
        Vault::organization(self)
    }
    fn organize(&self, k: &str, n: &str, p: Option<&str>) -> Result<()> {
        Vault::organize(self, k, n, p)
    }
    fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        Vault::list_trash(self)
    }
    fn restore(&self, id: &str, r: i64) -> Result<EntrySummary> {
        Vault::restore(self, id, r)
    }
    fn empty_trash(&self, e: Vec<EntryVersion>) -> Result<usize> {
        Vault::empty_trash(self, e)
    }
    fn settings(&self) -> Result<Settings> {
        Vault::settings(self)
    }
    fn save_settings(&self, s: &Settings) -> Result<()> {
        Vault::save_settings(self, s)
    }
    fn set_close_to_tray(&self, value: bool) -> Result<bool> {
        Vault::set_close_to_tray(self, value)
    }
    fn protected_values(&self) -> Result<Zeroizing<Vec<String>>> {
        Vault::protected_values(self)
    }
    fn sync_snapshot(&self) -> Result<vault_contracts::sync::SyncSnapshot> {
        Vault::sync_snapshot(self)
    }
    fn apply_sync(
        &self,
        changes: Vec<vault_contracts::sync::LocalChange>,
        expected: &std::collections::BTreeMap<String, vault_contracts::sync::SyncSlot>,
    ) -> Result<Vec<String>> {
        Vault::apply_sync(self, changes, expected)
    }
    fn sync_blob(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        Vault::sync_blob(self, name)
    }
    fn store_sync_blob(&self, name: &str, bytes: Option<&[u8]>) -> Result<()> {
        Vault::store_sync_blob(self, name, bytes)
    }
}
