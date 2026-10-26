use super::super::*;
impl Vault {
    pub fn open(directory: &Path) -> Result<Self> {
        if !directory.is_absolute() {
            return Err(AppError::invalid());
        }
        std::fs::create_dir_all(directory).map_err(|_| AppError::storage())?;
        let directory = directory.canonicalize().map_err(|_| AppError::storage())?;
        let path = directory.join("vault.db");
        let existed = path.exists();
        if existed
            && std::fs::metadata(&path)
                .map_err(|_| AppError::storage())?
                .len()
                == 0
        {
            return Err(AppError::new(
                "INVALID_DATABASE",
                "已有密码库为空或不完整，请保留原文件并检查备份。",
            ));
        }
        // Reject other formats before changing journal mode or opening a write transaction.
        if existed {
            let inspection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(crate::sql_error)?;
            let version: i64 = inspection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .map_err(crate::sql_error)?;
            if version != 4 {
                return Err(AppError::new(
                    "UNSUPPORTED_DATABASE",
                    "仅支持当前密码库格式，原文件未被修改。",
                ));
            }
        }
        let mut connection = Connection::open(&path).map_err(crate::sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(crate::sql_error)?;
        // Only encrypted content reaches SQLite, including its WAL and free pages.
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;",
            )
            .map_err(crate::sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(crate::sql_error)?;
        let version: i64 = transaction
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(crate::sql_error)?;
        let key = if version == 0 && !existed {
            let tables: i64 = transaction
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |row| row.get(0),
                )
                .map_err(crate::sql_error)?;
            if tables != 0 {
                return Err(AppError::new(
                    "INVALID_DATABASE",
                    "该文件不是本应用的密码库，未进行初始化。",
                ));
            }
            transaction
                .execute_batch(include_str!("schema.sql"))
                .map_err(crate::sql_error)?;
            let key = Zeroizing::new(crypto::random::<32>()?.to_vec());
            let wrapped = crypto::protect(&key)?;
            transaction
                .execute(
                    "INSERT INTO metadata (name,value) VALUES ('wrapped_key',?1)",
                    [wrapped],
                )
                .map_err(crate::sql_error)?;
            let check = crypto::encrypt(&key, KEY_AAD, KEY_AAD)?;
            transaction
                .execute(
                    "INSERT INTO metadata (name,value) VALUES ('key_check',?1)",
                    [check],
                )
                .map_err(crate::sql_error)?;
            let settings =
                serde_json::to_vec(&Settings::default()).map_err(|_| AppError::storage())?;
            transaction
                .execute(
                    "INSERT INTO metadata (name,value) VALUES ('settings',?1)",
                    [settings],
                )
                .map_err(crate::sql_error)?;
            key
        } else {
            if version != 4 {
                return Err(AppError::new(
                    "UNSUPPORTED_DATABASE",
                    "密码库格式不受支持，原文件未被覆盖。",
                ));
            }
            let wrapped: Vec<u8> = transaction
                .query_row(
                    "SELECT value FROM metadata WHERE name='wrapped_key'",
                    [],
                    |r| r.get(0),
                )
                .map_err(crate::sql_error)?;
            let key = crypto::unprotect(&wrapped)?;
            if key.len() != 32 {
                return Err(AppError::corrupt());
            }
            let packed: Vec<u8> = transaction
                .query_row(
                    "SELECT value FROM metadata WHERE name='key_check'",
                    [],
                    |r| r.get(0),
                )
                .map_err(crate::sql_error)?;
            if crypto::decrypt(&key, KEY_AAD, &packed)?.as_slice() != KEY_AAD {
                return Err(AppError::corrupt());
            }
            key
        };
        transaction.commit().map_err(crate::sql_error)?;
        let vault = Self {
            directory,
            key,
            connection: std::sync::Mutex::new(connection),
        };
        Ok(vault)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub(crate) fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| AppError::storage())
    }
}
