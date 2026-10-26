//! Cross-module fixture: delegates to real credentials and storage APIs.
use super::*;
use std::ops::Deref;
use vault_credentials::CredentialService;
pub struct TestVault(CredentialService);
impl TestVault {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self(CredentialService::new(Arc::new(
            vault_storage::Vault::open(path)?,
        ))))
    }
    pub fn connection(&self) -> Result<rusqlite::Connection> {
        rusqlite::Connection::open(self.directory().join("vault.db"))
            .map_err(|_| AppError::storage())
    }
    pub fn create(&self, value: EntryInput) -> Result<EntrySummary> {
        Ok(self.create_many(vec![value])?.remove(0))
    }
    pub fn create_many(&self, values: Vec<EntryInput>) -> Result<Vec<EntrySummary>> {
        self.0.create_many(
            values,
            &permissions::ChangePolicy {
                caller: permissions::Caller::Desktop,
                access: permissions::Access::Create,
            },
        )
    }
    pub fn update(&self, value: EntryPatch) -> Result<EntrySummary> {
        Ok(self.update_many(vec![value])?.remove(0))
    }
    pub fn update_many(&self, values: Vec<EntryPatch>) -> Result<Vec<EntrySummary>> {
        self.0.update_many(
            values,
            &permissions::ChangePolicy {
                caller: permissions::Caller::Desktop,
                access: permissions::Access::Update,
            },
        )
    }
    pub fn delete(&self, id: &str, revision: i64) -> Result<()> {
        self.delete_many(vec![EntryVersion {
            id: id.into(),
            expected_revision: revision,
        }])?;
        Ok(())
    }
    pub fn delete_many(&self, values: Vec<EntryVersion>) -> Result<Vec<String>> {
        self.0.delete_many(
            values,
            &permissions::ChangePolicy {
                caller: permissions::Caller::Desktop,
                access: permissions::Access::Delete,
            },
        )
    }
}
impl Deref for TestVault {
    type Target = CredentialService;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl vault_contracts::ports::repository::CredentialReader for TestVault {
    fn get(&self, id: &str) -> Result<EntryDetail> {
        vault_contracts::ports::repository::CredentialReader::get(&self.0, id)
    }
}
