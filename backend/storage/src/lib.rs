//! Encrypted SQLite implementation of the repository port.
pub(crate) mod crypto;
mod repositories;
mod settings;
mod sqlite;
mod tags;
mod trash;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sqlite::record::{COLUMNS, KEY_AAD, Row, from_sql};
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
