//! Desktop export/import plus the desktop-approved MCP export handshake.
//!
//! MCP never receives a passphrase, a path, plaintext or ciphertext: it asks the
//! on-screen user, then only learns whether the encrypted file was written.
use crate::{
    approval::{Approval, Messages},
    context::{CoreSession, now},
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use vault_contracts::{
    contracts::{
        ExportArgs, McpExportArgs, TransferFormat, TransferImportArgs, TransferPathArgs,
        TransferRespondArgs,
    },
    errors::{AppError, Result},
};
use vault_permissions as permissions;
use vault_transfer::Format;

pub(crate) const MCP_WAIT: Duration = Duration::from_secs(120);
const MAX_PATH: usize = 4096;

/// The pending MCP export: entry count and whether the trash is included.
pub(crate) type TransferCoordinator = Approval<(usize, bool), Outcome>;

pub(crate) enum Outcome {
    Exported(usize),
    Cancelled,
    Failed,
}

fn messages() -> Messages {
    Messages {
        busy: AppError::new(
            "EXPORT_PENDING",
            "已有一次导出正在等待桌面确认，请稍后重试。",
        ),
        ended: || AppError::new("EXPORT_CANCELLED", "导出请求已结束。"),
        timeout: AppError::new("EXPORT_TIMEOUT", "等待桌面端输入口令超时，导出已取消。"),
    }
}

/// Whether a desktop window is open to answer a request.
pub(crate) fn desktop_online(core: &CoreSession) -> Result<bool> {
    Ok(core
        .service
        .desktop
        .read()
        .map_err(|_| AppError::storage())?
        .owner_window
        != 0)
}

impl CoreSession {
    pub(crate) fn export(&self, args: ExportArgs) -> Result<Value> {
        let ExportArgs {
            format,
            include_trash,
            kinds,
            encrypted,
            passphrase,
            path,
        } = args;
        if kinds.is_empty() {
            return Err(AppError::invalid().with("kinds", "请至少选择一种凭据类型。"));
        }
        let format = match format {
            TransferFormat::Json => Format::Json,
            TransferFormat::Csv => Format::Csv,
        };
        let mut entries = self.service.storage.export_entries(include_trash)?;
        entries.retain(|entry| kinds.contains(&entry.kind));
        let bytes = if encrypted {
            let passphrase = passphrase.unwrap_or_default();
            if passphrase.is_empty() {
                return Err(AppError::invalid().with("passphrase", "请输入导出口令。"));
            }
            vault_transfer::encode_encrypted(format, &entries, &passphrase)?
        } else {
            vault_transfer::encode(format, &entries)?
        };
        let target = checked_path(&path)?;
        write_file(target, &bytes, self.directory())?;
        Ok(json!({"count": entries.len(), "path": path}))
    }

    pub(crate) fn inspect_export(&self, args: TransferPathArgs) -> Result<Value> {
        let bytes = read_file(&args.path)?;
        let inspection = vault_transfer::inspect(&bytes)?;
        let mut value = json!({
            "encrypted": inspection.encrypted,
            "format": match inspection.format {
                Format::Json => "json",
                Format::Csv => "csv",
            },
        });
        if !inspection.encrypted {
            value["count"] = json!(vault_transfer::decode(&bytes, None)?.entries.len());
        }
        Ok(value)
    }

    pub(crate) fn import_entries(&self, args: TransferImportArgs) -> Result<Value> {
        let bytes = read_file(&args.path)?;
        let decoded = vault_transfer::decode(&bytes, args.passphrase.as_deref())?;
        let mut values = Vec::with_capacity(decoded.entries.len());
        for imported in decoded.entries {
            let mut value = imported.value;
            self.service
                .storage
                .prepare_import(&mut value)
                .map_err(|error| row_error(imported.row, error))?;
            values.push(value);
        }
        let created = self.service.storage.import_many(
            values,
            &permissions::ChangePolicy {
                caller: self.caller,
                access: permissions::Access::Create,
            },
        )?;
        Ok(json!({"imported": created.len()}))
    }

    pub(crate) fn transfer_pending(&self) -> Result<Value> {
        let pending =
            self.service
                .transfer
                .pending()?
                .map(|(id, (count, include_trash), requested_at)| {
                    json!({
                        "id": id,
                        "count": count,
                        "includeTrash": include_trash,
                        "requestedAt": requested_at,
                    })
                });
        Ok(json!({ "pending": pending }))
    }

    pub(crate) fn transfer_respond(&self, args: TransferRespondArgs) -> Result<Value> {
        let TransferRespondArgs {
            id,
            passphrase,
            cancel,
        } = args;
        let Some((_count, include_trash)) = self.service.transfer.claim(&id)? else {
            return Err(AppError::new(
                "EXPORT_REQUEST_MISSING",
                "该导出请求已失效，请重新发起。",
            ));
        };
        if cancel {
            self.service.transfer.finish(&id, Outcome::Cancelled);
            return Ok(json!({"status": "cancelled"}));
        }
        let result = (|| {
            let passphrase = passphrase.unwrap_or_default();
            if passphrase.is_empty() {
                return Err(AppError::invalid().with("passphrase", "请输入导出口令。"));
            }
            let entries = self.service.storage.export_entries(include_trash)?;
            let bytes = vault_transfer::encode_encrypted(Format::Json, &entries, &passphrase)?;
            let directory = self.directory().join("exports");
            std::fs::create_dir_all(&directory).map_err(|_| export_failed())?;
            let path = directory.join(format!("vault-export-{}.vaultenc", now()));
            write_file(&path, &bytes, self.directory())?;
            Ok((entries.len(), path))
        })();
        match result {
            Ok((count, path)) => {
                self.service.transfer.finish(&id, Outcome::Exported(count));
                Ok(json!({"status": "exported", "count": count, "path": path}))
            }
            Err(error) => {
                self.service.transfer.finish(&id, Outcome::Failed);
                Err(error)
            }
        }
    }

    pub(crate) fn mcp_export(&self, args: McpExportArgs) -> Result<Value> {
        if self.caller != permissions::Caller::Mcp {
            return Err(AppError::new("METHOD_NOT_FOUND", "该方法不存在。"));
        }
        if !desktop_online(self)? {
            return Err(AppError::new(
                "DESKTOP_OFFLINE",
                "MCP 导出需要在桌面程序中输入口令，请先打开桌面界面。",
            ));
        }
        let count = self
            .service
            .storage
            .export_entries(args.include_trash)?
            .len();
        if count == 0 {
            return Err(AppError::new("EXPORT_EMPTY", "当前没有可导出的凭据。"));
        }
        let id = self
            .service
            .transfer
            .begin((count, args.include_trash), messages().busy)?;
        match self.service.transfer.wait(&id, MCP_WAIT, messages())? {
            Outcome::Exported(count) => Ok(json!({
                "status": "exported",
                "count": count,
                "format": "json",
                "encrypted": true,
            })),
            Outcome::Cancelled => Err(AppError::new("EXPORT_CANCELLED", "用户已取消导出。")),
            Outcome::Failed => Err(AppError::new("EXPORT_FAILED", "桌面端未能写出导出文件。")),
        }
    }
}

fn checked_path(path: &str) -> Result<&Path> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(AppError::invalid().with("path", "请选择有效的文件路径。"));
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(AppError::invalid().with("path", "请选择绝对文件路径。"));
    }
    Ok(path)
}

fn read_file(path: &str) -> Result<Vec<u8>> {
    let path = checked_path(path)?;
    let metadata = std::fs::metadata(path).map_err(|_| import_failed())?;
    if !metadata.is_file() {
        return Err(AppError::new("IMPORT_FAILED", "请选择文件而不是文件夹。"));
    }
    if metadata.len() > vault_transfer::MAX_BYTES as u64 {
        return Err(AppError::new(
            "FILE_TOO_LARGE",
            "导入文件超过 32 MB 上限，未导入任何条目。",
        ));
    }
    std::fs::read(path).map_err(|_| import_failed())
}

fn write_file(path: &Path, bytes: &[u8], data: &Path) -> Result<()> {
    if is_database_file(path, data) {
        return Err(AppError::invalid().with("path", "不能覆盖密码库文件，请选择其他位置。"));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        return Err(AppError::new(
            "EXPORT_FAILED",
            "导出目录不存在，请重新选择保存位置。",
        ));
    }
    std::fs::write(path, bytes).map_err(|_| export_failed())
}

fn is_database_file(path: &Path, data: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !matches!(
        name.to_ascii_lowercase().as_str(),
        "vault.db" | "vault.db-wal" | "vault.db-shm"
    ) {
        return false;
    }
    match (
        path.parent()
            .and_then(|parent| std::fs::canonicalize(parent).ok()),
        std::fs::canonicalize(data).ok(),
    ) {
        (Some(parent), Some(data)) => parent == data,
        _ => false,
    }
}

fn row_error(row: usize, error: AppError) -> AppError {
    let reason = error
        .details
        .as_ref()
        .and_then(|details| details.first())
        .map(|issue| issue.reason.clone())
        .unwrap_or_else(|| error.message.to_string());
    AppError::new(error.code, error.message).with("row", &format!("第 {row} 行：{reason}"))
}

fn import_failed() -> AppError {
    AppError::new(
        "IMPORT_FAILED",
        "无法读取导入文件，请确认文件存在且可访问。",
    )
}

fn export_failed() -> AppError {
    AppError::new(
        "EXPORT_FAILED",
        "无法写入导出文件，请检查目录权限或磁盘空间。",
    )
}
