//! Credential use cases. Repository ports keep SQL and encryption out of this service.
use crate::definitions::credentials::EntryDetail;
use crate::definitions::{
    credentials::{EntryInput, EntryPatch, EntrySummary, EntryVersion, Kind, Settings},
    errors::{AppError, Result},
    organization::{Organization, TrashEntry},
};
use crate::services::permissions;
use std::path::Path;
pub(crate) mod projection;
use crate::definitions::ports::repository::{CredentialReader, VaultRepository};
use std::sync::Arc;
pub(crate) mod policy;
pub(crate) mod validation;
use zeroize::Zeroizing;
pub struct CredentialService {
    repository: Arc<dyn VaultRepository>,
}
impl CredentialService {
    pub(crate) fn new(repository: Arc<dyn VaultRepository>) -> Self {
        Self { repository }
    }
    pub fn directory(&self) -> &Path {
        self.repository.directory()
    }

    pub fn list(&self, query: &str, kind: Option<Kind>) -> Result<Vec<EntrySummary>> {
        self.repository.list(query, kind)
    }
    pub fn summary(&self, id: &str) -> Result<EntrySummary> {
        Ok(self.repository.get(id)?.summary)
    }

    pub fn create_many(
        &self,
        input: Vec<EntryInput>,
        caller: permissions::Caller,
    ) -> Result<Vec<EntrySummary>> {
        self.repository.create_many(
            input,
            &policy::CredentialPolicy {
                caller,
                access: permissions::Access::Create,
            },
        )
    }

    pub fn update_many(
        &self,
        patches: Vec<EntryPatch>,
        caller: permissions::Caller,
    ) -> Result<Vec<EntrySummary>> {
        self.repository.update_many(
            patches,
            &policy::CredentialPolicy {
                caller,
                access: permissions::Access::Update,
            },
        )
    }

    pub fn delete_many(
        &self,
        entries: Vec<EntryVersion>,
        caller: permissions::Caller,
    ) -> Result<Vec<String>> {
        self.repository.delete_many(
            entries,
            &policy::CredentialPolicy {
                caller,
                access: permissions::Access::Delete,
            },
        )
    }
    pub fn pin(&self, id: &str, revision: i64, pinned: bool) -> Result<EntrySummary> {
        self.repository.pin(id, revision, pinned)
    }
    pub fn organization(&self) -> Result<Organization> {
        self.repository.organization()
    }
    pub fn organize(&self, kind: &str, name: &str, previous: Option<&str>) -> Result<()> {
        self.repository.organize(kind, name, previous)
    }
    pub fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        self.repository.list_trash()
    }
    pub fn restore(&self, id: &str, revision: i64) -> Result<EntrySummary> {
        self.repository.restore(id, revision)
    }
    /// Core interface supplies the exact snapshot after Windows-service confirmation.
    pub fn empty_trash(&self, expected: Vec<EntryVersion>) -> Result<usize> {
        self.repository.empty_trash(expected)
    }
    pub fn settings(&self) -> Result<Settings> {
        self.repository.settings()
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.repository.save_settings(settings)
    }
    pub fn protected_values(&self) -> Result<Zeroizing<Vec<String>>> {
        self.repository.protected_values()
    }
    /// Human desktop operations only. MCP exposes summary metadata and local writes.
    pub fn read_field(
        &self,
        id: &str,
        revision: i64,
        field: &str,
        service: Option<&str>,
    ) -> Result<String> {
        let entry = self.repository.get(id)?;
        if entry.summary.revision != revision {
            return Err(AppError::conflict());
        }
        Ok(match field {
            "account" => entry.summary.account,
            "url" if service.is_some() => entry.summary.service(service)?.url.clone(),
            "url" => entry.summary.url,
            "secret" => entry.secret,
            _ => return Err(AppError::invalid()),
        })
    }
}

impl CredentialReader for CredentialService {
    fn get(&self, id: &str) -> Result<EntryDetail> {
        self.repository.get(id)
    }
}
