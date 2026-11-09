//! Encrypted SQLite implementation of the repository port.
pub(crate) mod crypto;
mod repositories;
mod settings;
mod sqlite;
mod sync;
mod tags;
mod transfer;
mod trash;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sqlite::record::{COLUMNS, KEY_AAD, LEGACY_KEY_AAD, Row, from_sql};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use vault_contracts::{
    credentials::*,
    errors::{AppError, Result},
    organization::*,
    ports::repository::MutationPolicy,
};
use zeroize::Zeroizing;
pub struct Vault {
    directory: PathBuf,
    key: Zeroizing<Vec<u8>>,
    row_aad_prefix: &'static str,
    // One connection per vault. Concurrent RPC workers serialize SQL access
    // here, while transaction-local policies still guard every mutation.
    connection: std::sync::Mutex<Connection>,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

impl Vault {
    /// Encrypt a value bound to its metadata name and insert or replace it.
    pub(crate) fn put_metadata(&self, c: &Connection, name: &str, plaintext: &[u8]) -> Result<()> {
        let packed = crypto::encrypt(&self.key, name.as_bytes(), plaintext)?;
        c.execute(
            "INSERT INTO metadata(name,value) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET value=excluded.value",
            params![name, packed],
        )
        .map_err(sql_error)?;
        Ok(())
    }
}

/// Secrets an entry replaced; they go when the entry is removed for good.
fn drop_retired_secrets(c: &Connection, id: &str) -> Result<()> {
    c.execute(
        "DELETE FROM metadata WHERE name GLOB ?1",
        [format!("retired_secret:{id}:*")],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The origin a live API entry is bound to; it goes when the entry leaves the live table.
fn drop_api_origin(c: &Connection, id: &str) -> Result<()> {
    c.execute(
        "DELETE FROM metadata WHERE name=?1",
        [format!("api_origin:{id}")],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Translate driver errors at the storage boundary; never expose SQL or values.
fn sql_error(error: rusqlite::Error) -> AppError {
    #[cfg(debug_assertions)]
    if let Some(code) = error.sqlite_error() {
        eprintln!("storage diagnostic: {:?}/{}", code.code, code.extended_code);
    }
    #[cfg(not(debug_assertions))]
    let _ = error;
    AppError::storage()
}

#[cfg(test)]
mod tests;
