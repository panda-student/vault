//! Desktop export/import primitives. Row-level validation stays in core so the
//! UI can report the physical file position of an offending record.
use super::*;

const MAX_IMPORT: usize = 10_000;

impl Vault {
    pub fn export_entries(&self, include_trash: bool) -> Result<Vec<EntryInput>> {
        let connection = self.connection()?;
        let mut sql = format!("SELECT {COLUMNS} FROM entries");
        if include_trash {
            sql.push_str(&format!(" UNION ALL SELECT {COLUMNS} FROM trash"));
        }
        sql.push_str(" ORDER BY updated_at");
        let mut statement = connection.prepare(&sql).map_err(crate::sql_error)?;
        let rows = statement
            .query_map([], from_sql)
            .map_err(crate::sql_error)?;
        let mut values = Vec::new();
        for row in rows {
            let row = row.map_err(crate::sql_error)?;
            values.push(self.decode(&row)?);
        }
        Ok(values)
    }

    /// One IMMEDIATE transaction: a single failure discards every imported row.
    pub fn import_many(
        &self,
        values: Vec<EntryInput>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>> {
        if values.is_empty() || values.len() > MAX_IMPORT {
            return Err(AppError::invalid().with("entries", "一次导入的数量必须在 1-10000 之间。"));
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut created = Vec::with_capacity(values.len());
        for mut value in values {
            policy.prepare(&mut value)?;
            let time = now();
            let row = Row {
                id: uuid::Uuid::new_v4().to_string(),
                packed: vec![],
                revision: 1,
                pinned: false,
                created_at: time,
                updated_at: time,
            };
            let packed = self.encode(&row, &value)?;
            policy.authorize(&self.settings_on(&tx)?, None, &row.summary(&value))?;
            tx.execute(
                "INSERT INTO entries (id,encrypted,revision,pinned,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![row.id, packed, row.revision, row.pinned, row.created_at, row.updated_at],
            )
            .map_err(crate::sql_error)?;
            created.push(row.summary(&value));
        }
        tx.commit().map_err(crate::sql_error)?;
        Ok(created)
    }
}
