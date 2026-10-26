//! Encrypted SQLite implementation of the repository port.
pub(crate) mod crypto;
mod repositories;
mod settings;
mod sqlite;
mod tags;
mod trash;
use crate::definitions::{
    credentials::*,
    errors::{AppError, Result},
    organization::*,
    ports::repository::MutationPolicy,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sqlite::record::{COLUMNS, KEY_AAD, Row, from_sql};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
pub(crate) struct Vault {
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

#[cfg(test)]
#[path = "../../../tests/backend/data.rs"]
mod tests;
