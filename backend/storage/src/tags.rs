use super::*;

const CATALOG: &str = "organization-v1";

impl Vault {
    pub(crate) fn organization_on(&self, c: &Connection) -> Result<Organization> {
        let packed: Option<Vec<u8>> = c
            .query_row("SELECT value FROM metadata WHERE name=?1", [CATALOG], |r| {
                r.get(0)
            })
            .optional()
            .map_err(crate::sql_error)?;
        let mut catalog: Organization = match packed {
            Some(bytes) => {
                serde_json::from_slice(&crypto::decrypt(&self.key, CATALOG.as_bytes(), &bytes)?)
                    .map_err(|_| AppError::corrupt())?
            }
            None => Organization::default(),
        };
        let mut query = c
            .prepare(&format!(
                "SELECT {COLUMNS} FROM entries UNION ALL SELECT {COLUMNS} FROM trash"
            ))
            .map_err(crate::sql_error)?;
        for row in query.query_map([], from_sql).map_err(crate::sql_error)? {
            catalog
                .tags
                .extend(self.decode(&row.map_err(crate::sql_error)?)?.tags)
        }
        catalog.tags.sort();
        catalog.tags.dedup();
        Ok(catalog)
    }
    pub(crate) fn save_catalog_on(&self, c: &Connection, catalog: &Organization) -> Result<()> {
        let plaintext =
            Zeroizing::new(serde_json::to_vec(catalog).map_err(|_| AppError::invalid())?);
        let packed = crypto::encrypt(&self.key, CATALOG.as_bytes(), &plaintext)?;
        c.execute("INSERT INTO metadata(name,value) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET value=excluded.value",params![CATALOG,packed]).map_err(crate::sql_error)?;
        Ok(())
    }
    pub fn organization(&self) -> Result<Organization> {
        let connection = self.connection()?;
        self.organization_on(&connection)
    }
    pub fn organize(&self, kind: &str, name: &str, previous: Option<&str>) -> Result<()> {
        let name = name.trim();
        let deleting = name.is_empty() && previous.is_some();
        if kind != "tag" || (!deleting && name.is_empty()) || name.len() > 256 {
            return Err(AppError::invalid());
        }
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let mut catalog = self.organization_on(&tx)?;
        if !deleting && catalog.tags.iter().any(|n| n == name) && previous != Some(name) {
            return Err(AppError::new("DUPLICATE_NAME", "标签名称已存在。"));
        }
        if let Some(old) = previous {
            if !catalog.tags.iter().any(|n| n == old) {
                return Err(AppError::conflict());
            }
            catalog.tags.retain(|n| n != old);
            for table in ["entries", "trash"] {
                let rows: Vec<Row> = {
                    let mut q = tx
                        .prepare(&format!("SELECT {COLUMNS} FROM {table}"))
                        .map_err(crate::sql_error)?;
                    q.query_map([], from_sql)
                        .map_err(crate::sql_error)?
                        .collect::<std::result::Result<_, _>>()
                        .map_err(crate::sql_error)?
                };
                for mut row in rows {
                    let mut value = self.decode(&row)?;
                    if !value.tags.iter().any(|t| t == old) {
                        continue;
                    }
                    value.tags.retain(|t| t != old);
                    if !deleting && !value.tags.iter().any(|t| t == name) {
                        value.tags.push(name.into())
                    }
                    row.revision += 1;
                    row.updated_at = now();
                    tx.execute(
                        &format!(
                            "UPDATE {table} SET encrypted=?1,revision=?2,updated_at=?3 WHERE id=?4"
                        ),
                        params![
                            self.encode(&row, &value)?,
                            row.revision,
                            row.updated_at,
                            row.id
                        ],
                    )
                    .map_err(crate::sql_error)?;
                }
            }
            let mut settings = self.settings_on(&tx)?;
            if let Some(rule) = settings.mcp_tag_rules.remove(old)
                && !deleting
            {
                settings.mcp_tag_rules.insert(name.into(), rule);
            }
            self.save_settings_on(&tx, &settings)?;
        }
        if !deleting {
            catalog.tags.push(name.into())
        }
        catalog.tags.sort();
        catalog.tags.dedup();
        self.save_catalog_on(&tx, &catalog)?;
        tx.commit().map_err(crate::sql_error)?;
        Ok(())
    }
}
