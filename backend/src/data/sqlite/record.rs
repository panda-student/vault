use super::super::*;
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) packed: Vec<u8>,
    pub(crate) revision: i64,
    pub(crate) pinned: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

impl Row {
    pub(crate) fn aad(&self) -> Vec<u8> {
        format!(
            "local-password-v1:{}:{}:{}:{}:{}",
            self.id, self.revision, self.pinned, self.created_at, self.updated_at
        )
        .into_bytes()
    }
    pub(crate) fn summary(&self, value: &EntryInput) -> EntrySummary {
        EntrySummary {
            id: self.id.clone(),
            kind: value.kind,
            title: value.title.clone(),
            tags: value.tags.clone(),
            database: value.database.clone(),
            account: value.account.clone(),
            url: value.url.clone(),
            documentation_url: value.documentation_url.clone(),
            services: value.services.clone(),
            notes: value.notes.clone(),
            pinned: self.pinned,
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }
}

pub(crate) fn from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        id: row.get(0)?,
        packed: row.get(1)?,
        revision: row.get(2)?,
        pinned: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
pub(crate) const COLUMNS: &str = "id, encrypted, revision, pinned, created_at, updated_at";
pub(crate) const KEY_AAD: &[u8] = b"local-password-manager-key-check-v1";
