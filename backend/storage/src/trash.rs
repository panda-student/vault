//! Desktop recovery operations. Intentionally absent from the MCP dispatcher.
use super::*;

impl Vault {
    pub fn list_trash(&self) -> Result<Vec<TrashEntry>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {COLUMNS},deleted_at FROM trash ORDER BY deleted_at DESC,id"
            ))
            .map_err(crate::sql_error)?;
        let rows = statement
            .query_map([], |r| Ok((from_sql(r)?, r.get::<_, i64>(6)?)))
            .map_err(crate::sql_error)?;
        rows.map(|record| {
            let (row, deleted_at) = record.map_err(crate::sql_error)?;
            let value = self.decode(&row)?;
            Ok(TrashEntry {
                summary: row.summary(&value),
                deleted_at,
            })
        })
        .collect()
    }

    pub fn restore(&self, id: &str, expected_revision: i64) -> Result<EntrySummary> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut row = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM trash WHERE id=?1"),
                [id],
                from_sql,
            )
            .optional()
            .map_err(crate::sql_error)?
            .ok_or(AppError::new("NOT_FOUND", "垃圾桶条目不存在。"))?;
        if row.revision != expected_revision {
            return Err(AppError::conflict());
        }
        let value = self.decode(&row)?;
        // Old clients must not overwrite a restored entry using a pre-delete version.
        row.revision += 1;
        row.updated_at = now();
        let packed = self.encode(&row, &value)?;
        tx.execute("INSERT INTO entries (id,encrypted,revision,pinned,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![row.id, packed, row.revision, row.pinned, row.created_at, row.updated_at]).map_err(crate::sql_error)?;
        tx.execute("DELETE FROM trash WHERE id=?1", [id])
            .map_err(crate::sql_error)?;
        tx.commit().map_err(crate::sql_error)?;
        Ok(row.summary(&value))
    }

    /// Delete only the exact snapshot the human confirmed; concurrent changes fail.
    pub fn empty_trash(&self, expected: Vec<EntryVersion>) -> Result<usize> {
        let mut connection = self.connection()?;
        connection
            .execute_batch("PRAGMA secure_delete=ON;")
            .map_err(crate::sql_error)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut actual: Vec<(String, i64)> = {
            let mut statement = tx
                .prepare("SELECT id,revision FROM trash")
                .map_err(crate::sql_error)?;
            statement
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(crate::sql_error)?
                .collect::<std::result::Result<_, _>>()
                .map_err(crate::sql_error)?
        };
        let mut expected: Vec<_> = expected
            .into_iter()
            .map(|e| (e.id, e.expected_revision))
            .collect();
        actual.sort();
        expected.sort();
        if actual != expected {
            return Err(AppError::conflict());
        }
        // Retired values use entry-owned keys in the current format.
        for (id, _) in &actual {
            tx.execute(
                "DELETE FROM metadata WHERE name GLOB ?1",
                [format!("retired_secret:{id}:*")],
            )
            .map_err(crate::sql_error)?;
        }
        let count = tx
            .execute("DELETE FROM trash", [])
            .map_err(crate::sql_error)?;
        tx.commit().map_err(crate::sql_error)?;
        Ok(count)
    }
}
