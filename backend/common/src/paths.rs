//! Stable, per-user coordination paths shared by transport and updates.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use vault_contracts::errors::{AppError, Result};
pub fn coordination_directory(executable: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(executable).map_err(|_| AppError::storage())?;
    let name = format!(
        "{:x}",
        Sha256::digest(canonical.to_string_lossy().to_lowercase().as_bytes())
    );
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(AppError::storage)?)
            .join("LocalPasswordManager")
            .join("updates")
            .join(name),
    )
}
