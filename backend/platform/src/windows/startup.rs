use std::path::Path;
use vault_contracts::errors::{AppError, Result};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
};
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "Vault";
const LEGACY_NAME: &str = "LocalPasswordManager.Portable";
fn command(executable: &Path, data_directory: &Path) -> String {
    format!(
        "\"{}\" --data-dir \"{}\" --background",
        executable.display(),
        data_directory.display()
    )
}
fn migrate_legacy(value: &str) -> Result<()> {
    set_at(KEY, NAME, true, value)?;
    set_at(KEY, LEGACY_NAME, false, value)
}
pub fn enabled(executable: &Path, data_directory: &Path) -> Result<bool> {
    let value = command(executable, data_directory);
    let current = enabled_at(KEY, NAME, &value)?;
    let legacy = enabled_at(KEY, LEGACY_NAME, &value)?;
    if legacy && !current {
        // Reading settings must not fail because the old value could not be
        // rewritten; the legacy value still means startup is enabled.
        let _ = migrate_legacy(&value);
    }
    Ok(current || legacy)
}
pub fn set(enabled: bool, executable: &Path, data_directory: &Path) -> Result<()> {
    let value = command(executable, data_directory);
    set_at(KEY, NAME, enabled, &value)?;
    set_at(KEY, LEGACY_NAME, false, &value)
}
fn enabled_at(path: &str, name: &str, expected: &str) -> Result<bool> {
    let root = RegKey::predef(HKEY_CURRENT_USER);
    match root.open_subkey_with_flags(path, KEY_READ) {
        Ok(key) => match key.get_value::<String, _>(name) {
            Ok(value) => Ok(value == expected),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(AppError::new(
                "STARTUP_ERROR",
                "无法读取当前用户的启动设置。",
            )),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(AppError::new(
            "STARTUP_ERROR",
            "无法读取当前用户的启动设置。",
        )),
    }
}
fn set_at(path: &str, name: &str, enabled: bool, value: &str) -> Result<()> {
    let root = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let (key, _) = root
            .create_subkey(path)
            .map_err(|_| AppError::new("STARTUP_ERROR", "无法开启登录后启动。"))?;
        key.set_value(name, &value)
            .map_err(|_| AppError::new("STARTUP_ERROR", "无法开启登录后启动。"))?;
    } else {
        match root.open_subkey_with_flags(path, KEY_WRITE) {
            Ok(key) => match key.delete_value(name) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(AppError::new("STARTUP_ERROR", "无法关闭登录后启动。")),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(AppError::new("STARTUP_ERROR", "无法关闭登录后启动。")),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_registry_roundtrip_and_quoted_paths() {
        // Keep test keys outside `Software\Vault`: the installer acceptance guard
        // treats that key as an existing installation.
        let path = format!("Software\\VaultTests\\Acceptance\\{}", uuid::Uuid::new_v4());
        let value = command(
            Path::new("D:\\中文 路径\\App.exe"),
            Path::new("D:\\中文 数据"),
        );
        assert!(value.starts_with('"'));
        assert!(value.contains("\" --data-dir \""));
        assert!(value.ends_with("--background"));
        assert!(!enabled_at(&path, NAME, &value).unwrap());
        set_at(&path, NAME, true, &value).unwrap();
        assert!(enabled_at(&path, NAME, &value).unwrap());
        set_at(&path, LEGACY_NAME, true, &value).unwrap();
        assert!(enabled_at(&path, LEGACY_NAME, &value).unwrap());
        set_at(&path, LEGACY_NAME, false, &value).unwrap();
        assert!(!enabled_at(&path, LEGACY_NAME, &value).unwrap());
        set_at(&path, NAME, false, &value).unwrap();
        assert!(!enabled_at(&path, NAME, &value).unwrap());
        let root = RegKey::predef(HKEY_CURRENT_USER);
        root.delete_subkey(&path).unwrap();
        root.delete_subkey("Software\\VaultTests\\Acceptance").ok();
        root.delete_subkey("Software\\VaultTests").ok();
    }
}
