use super::super::*;
impl Vault {
    pub(crate) fn write_many<T, R>(
        &self,
        values: Vec<T>,
        mut action: impl FnMut(&Connection, T) -> Result<R>,
    ) -> Result<Vec<R>> {
        if values.is_empty() || values.len() > 100 {
            return Err(AppError::invalid().with("items", "批量数量必须在 1-100 之间。"));
        }
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut result = Vec::with_capacity(values.len());
        for value in values {
            result.push(action(&tx, value)?);
        }
        tx.commit()?;
        Ok(result)
    }
}
