use super::super::*;
impl Vault {
    pub(crate) fn find(connection: &Connection, id: &str) -> Result<Row> {
        if id.len() > 64 {
            return Err(AppError::invalid());
        }
        connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM entries WHERE id=?1"),
                [id],
                from_sql,
            )
            .optional()
            .map_err(crate::sql_error)?
            .ok_or(AppError::new("NOT_FOUND", "条目不存在，可能已被删除。"))
    }
    pub fn list(&self, query: &str, kind: Option<Kind>) -> Result<Vec<EntrySummary>> {
        if query.len() > 4096 {
            return Err(AppError::invalid());
        }
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!("SELECT {COLUMNS} FROM entries"))
            .map_err(crate::sql_error)?;
        let rows = statement
            .query_map([], from_sql)
            .map_err(crate::sql_error)?;
        let query = query.trim().to_lowercase();
        let mut result = Vec::new();
        for row in rows {
            let row = row.map_err(crate::sql_error)?;
            let value = self.decode(&row)?;
            if kind.is_none_or(|k| k == value.kind)
                && ([
                    &value.title,
                    &value.account,
                    &value.url,
                    &value.documentation_url,
                ]
                .iter()
                .any(|s| s.to_lowercase().contains(&query))
                    || value.tags.iter().any(|s| s.to_lowercase().contains(&query))
                    || value.database.as_ref().is_some_and(|db| {
                        [&db.host, &db.database_type, &db.database_name]
                            .iter()
                            .any(|s| s.to_lowercase().contains(&query))
                    })
                    || value
                        .services
                        .iter()
                        .any(|s| s.url.to_lowercase().contains(&query)))
            {
                result.push(row.summary(&value));
            }
        }
        result.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(result)
    }
    pub fn get(&self, id: &str) -> Result<EntryDetail> {
        let connection = self.connection()?;
        let row = Self::find(&connection, id)?;
        let value = self.decode(&row)?;
        Ok(EntryDetail {
            summary: row.summary(&value),
            secret: value.secret,
        })
    }
    pub fn create_many_checked(
        &self,
        values: Vec<EntryInput>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>> {
        self.write_many(values, |connection, mut value| {
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
        policy.authorize(&self.settings_on(connection)?,None,&row.summary(&value))?;
        connection.execute("INSERT INTO entries (id,encrypted,revision,pinned,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",params![row.id,packed,row.revision,row.pinned,row.created_at,row.updated_at]).map_err(crate::sql_error)?;
        Ok(row.summary(&value))
        })
    }
    pub fn update_many_checked(
        &self,
        values: Vec<EntryPatch>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<EntrySummary>> {
        let mut ids = std::collections::HashSet::new();
        if values.iter().any(|v| !ids.insert(&v.id)) {
            return Err(AppError::invalid().with("items", "批量列表存在重复的条目 ID。"));
        }
        self.write_many(values, |connection, patch| {
            let mut row = Self::find(connection, &patch.id)?;
            if row.revision != patch.expected_revision {
                return Err(AppError::conflict());
            }
            let value = self.decode(&row)?;
            let original = row.summary(&value);
            let old_secret = Zeroizing::new(value.secret.clone());
            let value = policy.patch(value, patch)?;
            policy.authorize(
                &self.settings_on(connection)?,
                Some(&original),
                &row.summary(&value),
            )?;
            if old_secret.as_str() != value.secret {
                self.retain_protected_value(connection, &row.id, &old_secret)?;
            }
            row.revision += 1;
            row.updated_at = now();
            let packed = self.encode(&row, &value)?;
            connection
                .execute(
                    "UPDATE entries SET encrypted=?1,revision=?2,updated_at=?3 WHERE id=?4",
                    params![packed, row.revision, row.updated_at, row.id],
                )
                .map_err(crate::sql_error)?;
            Ok(row.summary(&value))
        })
    }
    pub fn pin(&self, id: &str, expected_revision: i64, pinned: bool) -> Result<EntrySummary> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut row = Self::find(&tx, id)?;
        if row.revision != expected_revision {
            return Err(AppError::conflict());
        }
        let value = self.decode(&row)?;
        row.pinned = pinned;
        row.revision += 1;
        row.updated_at = now();
        let packed = self.encode(&row, &value)?;
        tx.execute(
            "UPDATE entries SET encrypted=?1,revision=?2,pinned=?3,updated_at=?4 WHERE id=?5",
            params![packed, row.revision, row.pinned, row.updated_at, row.id],
        )
        .map_err(crate::sql_error)?;
        tx.commit().map_err(crate::sql_error)?;
        Ok(row.summary(&value))
    }
    pub fn delete_many_checked(
        &self,
        values: Vec<EntryVersion>,
        policy: &dyn MutationPolicy,
    ) -> Result<Vec<String>> {
        let mut ids = std::collections::HashSet::new();
        if values.iter().any(|v| !ids.insert(&v.id)) {
            return Err(AppError::invalid().with("items", "批量列表存在重复的条目 ID。"));
        }
        self.write_many(values, |connection, item| {
            let row = Self::find(connection, &item.id)?;
            let original = row.summary(&self.decode(&row)?);
            policy.authorize(&self.settings_on(connection)?,Some(&original),&original)?;
            if row.revision != item.expected_revision {
                return Err(AppError::conflict());
            }
            // Move the authenticated encrypted record, never decrypt into SQLite.
            connection.execute(
                "INSERT INTO trash (id,encrypted,revision,pinned,created_at,updated_at,deleted_at) SELECT id,encrypted,revision,pinned,created_at,updated_at,?2 FROM entries WHERE id=?1",
                params![item.id, now()],
            ).map_err(crate::sql_error)?;
            connection.execute("DELETE FROM entries WHERE id=?1", [&item.id]).map_err(crate::sql_error)?;
            connection.execute(
                "DELETE FROM metadata WHERE name=?1",
                [format!("api_origin:{}", item.id)],
            ).map_err(crate::sql_error)?;
            Ok(item.id)
        })
    }
    pub fn settings(&self) -> Result<Settings> {
        let connection = self.connection()?;
        self.settings_on(&connection)
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        self.save_settings_on(&tx, settings)?;
        tx.commit().map_err(crate::sql_error)?;
        Ok(())
    }
}
