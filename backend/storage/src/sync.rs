//! Sync snapshot and transactional apply. Only core's sync workflow calls these.
use super::*;
use std::collections::BTreeMap;
use vault_contracts::sync::{LocalChange, SyncRecord, SyncSlot, SyncSnapshot};

const BLOB_PREFIX: &str = "sync:";

impl Vault {
    pub fn sync_snapshot(&self) -> Result<SyncSnapshot> {
        let connection = self.connection()?;
        let mut records = Vec::new();
        let mut slots = BTreeMap::new();
        for (table, trashed) in [("entries", false), ("trash", true)] {
            let extra = if trashed { ",deleted_at" } else { ",NULL" };
            let mut statement = connection
                .prepare(&format!("SELECT {COLUMNS}{extra} FROM {table}"))
                .map_err(crate::sql_error)?;
            let rows = statement
                .query_map([], |r| Ok((from_sql(r)?, r.get::<_, Option<i64>>(6)?)))
                .map_err(crate::sql_error)?;
            for row in rows {
                let (row, deleted_at) = row.map_err(crate::sql_error)?;
                let value = self.decode(&row)?;
                slots.insert(
                    row.id.clone(),
                    SyncSlot {
                        revision: row.revision,
                        trashed,
                    },
                );
                records.push(SyncRecord {
                    id: row.id,
                    value,
                    pinned: row.pinned,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    deleted_at,
                    device: String::new(),
                });
            }
        }
        Ok(SyncSnapshot { records, slots })
    }

    /// One IMMEDIATE transaction; a change whose entry moved since the
    /// snapshot is skipped rather than overwriting a concurrent local edit.
    pub fn apply_sync(
        &self,
        changes: Vec<LocalChange>,
        expected: &BTreeMap<String, SyncSlot>,
    ) -> Result<Vec<String>> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut skipped = Vec::new();
        for change in changes {
            let id = change.id().to_string();
            let current = current_slot(&tx, &id)?;
            if current.as_ref().map(|(_, slot)| *slot) != expected.get(&id).copied() {
                skipped.push(id);
                continue;
            }
            let previous = current.map(|(row, _)| row);
            match change {
                LocalChange::Upsert(record) => self.upsert(&tx, previous, record)?,
                LocalChange::Purge(id) => purge(&tx, &id)?,
            }
        }
        tx.commit().map_err(crate::sql_error)?;
        Ok(skipped)
    }

    fn upsert(&self, tx: &Connection, previous: Option<Row>, record: SyncRecord) -> Result<()> {
        if let Some(old) = &previous {
            // Replaced secrets stay protected from MCP output, as on an update.
            let old_value = self.decode(old)?;
            for secret in old_value.secrets() {
                if !record.value.secrets().any(|s| s == secret) {
                    self.retain_protected_value(tx, &old.id, secret)?;
                }
            }
        }
        let row = Row {
            id: record.id.clone(),
            packed: vec![],
            revision: previous.as_ref().map_or(1, |old| old.revision + 1),
            pinned: record.pinned,
            created_at: previous
                .as_ref()
                .map_or(record.created_at, |old| old.created_at),
            updated_at: record.updated_at,
        };
        let packed = self.encode(&row, &record.value)?;
        tx.execute("DELETE FROM entries WHERE id=?1", [&row.id])
            .map_err(crate::sql_error)?;
        tx.execute("DELETE FROM trash WHERE id=?1", [&row.id])
            .map_err(crate::sql_error)?;
        match record.deleted_at {
            None => tx.execute(
                "INSERT INTO entries (id,encrypted,revision,pinned,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![row.id, packed, row.revision, row.pinned, row.created_at, row.updated_at],
            ),
            Some(deleted_at) => tx.execute(
                "INSERT INTO trash (id,encrypted,revision,pinned,created_at,updated_at,deleted_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![row.id, packed, row.revision, row.pinned, row.created_at, row.updated_at, deleted_at],
            ),
        }
        .map_err(crate::sql_error)?;
        if record.deleted_at.is_some() {
            // As a local move to the trash does.
            crate::drop_api_origin(tx, &row.id)?;
        }
        Ok(())
    }

    pub fn sync_blob(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let key = blob_key(name)?;
        let connection = self.connection()?;
        let packed: Option<Vec<u8>> = connection
            .query_row("SELECT value FROM metadata WHERE name=?1", [&key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(crate::sql_error)?;
        packed
            .map(|packed| crypto::decrypt(&self.key, key.as_bytes(), &packed))
            .transpose()
    }

    pub fn store_sync_blob(&self, name: &str, bytes: Option<&[u8]>) -> Result<()> {
        let key = blob_key(name)?;
        let connection = self.connection()?;
        match bytes {
            Some(bytes) => self.put_metadata(&connection, &key, bytes),
            None => connection
                .execute("DELETE FROM metadata WHERE name=?1", [&key])
                .map(|_| ())
                .map_err(crate::sql_error),
        }
    }
}

fn current_slot(tx: &Connection, id: &str) -> Result<Option<(Row, SyncSlot)>> {
    for (table, trashed) in [("entries", false), ("trash", true)] {
        let row = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM {table} WHERE id=?1"),
                [id],
                from_sql,
            )
            .optional()
            .map_err(crate::sql_error)?;
        if let Some(row) = row {
            let slot = SyncSlot {
                revision: row.revision,
                trashed,
            };
            return Ok(Some((row, slot)));
        }
    }
    Ok(None)
}

/// Remove an entry for good, with its retired secrets, as emptying the trash does.
fn purge(tx: &Connection, id: &str) -> Result<()> {
    for sql in [
        "DELETE FROM entries WHERE id=?1",
        "DELETE FROM trash WHERE id=?1",
    ] {
        tx.execute(sql, [id]).map_err(crate::sql_error)?;
    }
    crate::drop_retired_secrets(tx, id)?;
    crate::drop_api_origin(tx, id)
}

fn blob_key(name: &str) -> Result<String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid());
    }
    Ok(format!("{BLOB_PREFIX}{name}"))
}
