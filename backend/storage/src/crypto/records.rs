use super::super::*;
impl Vault {
    pub fn protected_values(&self) -> Result<Zeroizing<Vec<String>>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM entries UNION ALL SELECT {COLUMNS} FROM trash"
            ))
            .map_err(crate::sql_error)?;
        let mut values = Zeroizing::new(Vec::new());
        for row in statement
            .query_map([], from_sql)
            .map_err(crate::sql_error)?
        {
            values.push(self.decode(&row.map_err(crate::sql_error)?)?.secret);
        }
        let mut history = connection
            .prepare("SELECT name,value FROM metadata WHERE name GLOB 'retired_secret:*'")
            .map_err(crate::sql_error)?;
        for row in history
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(crate::sql_error)?
        {
            let (name, packed) = row.map_err(crate::sql_error)?;
            let value = crypto::decrypt(&self.key, name.as_bytes(), &packed)?;
            values.push(String::from_utf8(value.to_vec()).map_err(|_| AppError::corrupt())?);
        }
        Ok(values)
    }
    pub(crate) fn retain_protected_value(
        &self,
        connection: &Connection,
        id: &str,
        value: &str,
    ) -> Result<()> {
        let name = format!("retired_secret:{}:{}", id, uuid::Uuid::new_v4());
        let packed = crypto::encrypt(&self.key, name.as_bytes(), value.as_bytes())?;
        connection
            .execute(
                "INSERT INTO metadata(name,value) VALUES (?1,?2)",
                params![name, packed],
            )
            .map_err(crate::sql_error)?;
        Ok(())
    }
    pub(crate) fn decode(&self, row: &Row) -> Result<EntryInput> {
        let plaintext = crypto::decrypt(&self.key, &row.aad(), &row.packed)?;
        serde_json::from_slice(&plaintext).map_err(|_| AppError::corrupt())
    }
    pub(crate) fn encode(&self, row: &Row, value: &EntryInput) -> Result<Vec<u8>> {
        let plaintext = Zeroizing::new(serde_json::to_vec(value).map_err(|_| AppError::invalid())?);
        crypto::encrypt(&self.key, &row.aad(), &plaintext)
    }
}
