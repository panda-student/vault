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
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(AppError::storage)?);
    let directory = coordination_root(&local).join("updates").join(&name);
    if fs::create_dir_all(&directory).is_err() {
        // A dangling junction or a non-directory at `Vault` must not wedge
        // startup; the legacy path is still valid for the current update.
        let legacy = local
            .join("LocalPasswordManager")
            .join("updates")
            .join(&name);
        fs::create_dir_all(&legacy).map_err(|_| AppError::storage())?;
        return Ok(legacy);
    }
    Ok(directory)
}

/// Existing profiles keep their coordination state while the visible directory
/// becomes `Vault`: a junction makes both paths address the same files, so an
/// in-place update can still pause the previous version's bridges.
fn coordination_root(local: &Path) -> PathBuf {
    let current = local.join("Vault");
    let legacy = local.join("LocalPasswordManager");
    if let Ok(meta) = std::fs::symlink_metadata(&current) {
        if meta.file_type().is_symlink() || is_windows_directory_link(&meta) {
            if current.exists() {
                return current;
            }
            // Dangling junction: remove it and let the legacy path rebuild the
            // target, or create a fresh directory below.
            let _ = std::fs::remove_dir(&current).or_else(|_| std::fs::remove_file(&current));
            if std::fs::symlink_metadata(&current).is_ok() {
                return legacy;
            }
        } else if legacy.exists() {
            // Two independent directories exist; keep the legacy coordination
            // state so an in-place update can still pause the old bridges.
            return legacy;
        } else {
            return current;
        }
    }
    if legacy.exists() {
        if create_junction(&current, &legacy).is_ok() && current.exists() {
            return current;
        }
        return legacy;
    }
    current
}

#[cfg(windows)]
fn is_windows_directory_link(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::FileTypeExt;
    meta.file_type().is_symlink_dir()
}

#[cfg(not(windows))]
fn is_windows_directory_link(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    let cmd = std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("cmd.exe"))
        .unwrap_or_else(|| PathBuf::from("cmd.exe"));
    let status = std::process::Command::new(cmd)
        .arg("/C")
        .raw_arg(format!(
            "mklink /J \"{}\" \"{}\"",
            link.display(),
            target.display()
        ))
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("junction creation failed"));
    }
    if std::fs::canonicalize(link)? != std::fs::canonicalize(target)? {
        return Err(std::io::Error::other("junction target mismatch"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn create_junction(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("junctions are Windows-only"))
}

#[cfg(test)]
mod tests {
    use super::coordination_root;
    #[test]
    fn fresh_profile_uses_vault() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(coordination_root(temp.path()), temp.path().join("Vault"));
    }
    #[test]
    fn legacy_profile_is_bridged_or_used() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("LocalPasswordManager");
        std::fs::create_dir_all(&legacy).unwrap();
        let root = coordination_root(temp.path());
        if root == temp.path().join("Vault") {
            std::fs::write(root.join("probe"), b"x").unwrap();
            assert!(
                legacy.join("probe").exists(),
                "Vault must alias the legacy directory"
            );
        } else {
            assert_eq!(root, legacy);
        }
    }
}
