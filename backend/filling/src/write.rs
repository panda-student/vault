use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use vault_contracts::ports::system::SystemPlatform;
use vault_contracts::{
    credentials::EntryDetail,
    errors::{AppError, Result},
    filling::AskpassAnswers,
    ports::repository::CredentialReader,
};
use zeroize::Zeroizing;

use vault_contracts::write::{Destination, FileMode, SECRET_FIELDS, WriteArgs};
fn failed(code: &'static str) -> AppError {
    AppError::new(
        code,
        "Local credential write failed; no file contents are returned.",
    )
}
pub(crate) fn execute(
    vault: &dyn CredentialReader,
    windows: &dyn SystemPlatform,
    args: WriteArgs,
) -> Result<Value> {
    let resolve = || {
        resolve_field(
            vault,
            &args.credential_id,
            &args.field,
            args.service_id.as_deref(),
            args.group_id.as_deref(),
        )
    };
    match args.target {
        Destination::Command {
            command,
            cwd,
            env,
            stdin,
            timeout_seconds,
        } => {
            let entry = vault.get(&args.credential_id)?;
            // `{{credential}}` stands for `field`; resolved only if it is used.
            let uses = |text: &str| text.contains("{{credential");
            let credential = if command.iter().any(|item| uses(item))
                || env.values().any(|value| uses(value))
                || stdin.as_deref().is_some_and(uses)
            {
                Some(Zeroizing::new(field_of(
                    &entry,
                    &args.field,
                    args.service_id.as_deref(),
                    args.group_id.as_deref(),
                )?))
            } else {
                None
            };
            let secret = SECRET_FIELDS.contains(&args.field.as_str());
            // git, ssh and sudo ask through a helper rather than reading stdin;
            // vault answers those prompts itself when they match this credential.
            let askpass = askpass_answers(&entry, args.group_id.as_deref())
                .and_then(|answers| windows.askpass(answers).ok());
            let injector =
                crate::command::injector(&entry, args.group_id.as_deref(), credential, secret);
            crate::command::execute(
                injector,
                crate::command::CommandSpec {
                    command,
                    cwd,
                    env,
                    stdin,
                    timeout_seconds,
                },
                askpass,
            )
        }
        Destination::Auto(target) => {
            let hosts = entry_hosts_of(vault, &args.credential_id)?;
            crate::auto::fill(windows, target, &hosts, resolve)
        }
        Destination::Input { target_id } => {
            Ok(json!(windows.fill_target(&target_id, Box::new(resolve))?))
        }
        Destination::Browser {
            port,
            selector,
            target_id,
        } => crate::browser::fill(port, &selector, target_id.as_deref(), resolve),
        Destination::File {
            path,
            mode,
            template,
            placeholder,
        } => {
            // A password or token is only ever put in by vault itself: into a
            // program it runs, a browser page or a captured input field. Written
            // to a file it would be read back by whatever runs next.
            if SECRET_FIELDS.contains(&args.field.as_str()) {
                return Err(AppError::new(
                    "SECRET_FILE_WRITE_FORBIDDEN",
                    "密码和 token 不能写进文件。需要它的程序请用 command 目标让 vault 自己运行并注入（环境变量、参数占位符或标准输入），登录网页请用 browser 或 input 目标。",
                ));
            }
            // Absolute local paths only: no shell, script, clipboard or URL transport.
            if !path.is_absolute() || path.file_name().is_none() {
                return Err(failed("ABSOLUTE_FILE_PATH_REQUIRED"));
            }
            let marker = placeholder.as_deref().unwrap_or("{{credential}}");
            if marker.is_empty() || marker.len() > 4096 {
                return Err(AppError::invalid());
            }
            let existing = matches!(mode, FileMode::Replace);
            let source = Zeroizing::new(if existing {
                if template.is_some() {
                    return Err(AppError::invalid());
                }
                let metadata =
                    fs::symlink_metadata(&path).map_err(|_| failed("FILE_UNAVAILABLE"))?;
                if !metadata.is_file()
                    || metadata.permissions().readonly()
                    || metadata.len() > 4 * 1024 * 1024
                {
                    return Err(failed("FILE_NOT_WRITABLE"));
                }
                fs::read_to_string(&path).map_err(|_| failed("UTF8_FILE_REQUIRED"))?
            } else {
                template.unwrap_or_else(|| "{{credential}}".into())
            });
            if source.len() > 4 * 1024 * 1024 || source.matches(marker).count() != 1 {
                return Err(failed("PLACEHOLDER_MUST_OCCUR_ONCE"));
            }
            let secret = Zeroizing::new(resolve()?);
            let rendered = Zeroizing::new(source.replacen(marker, &secret, 1));
            if existing {
                // Stage alongside destination, then atomically replace; no partial
                // original file on write failure. Remove the staging file on errors.
                let temporary =
                    path.with_file_name(format!(".vault-write-{}.tmp", uuid::Uuid::new_v4()));
                let result = (|| {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temporary)
                        .map_err(|_| failed("FILE_NOT_WRITABLE"))?;
                    file.write_all(rendered.as_bytes())
                        .and_then(|_| file.sync_all())
                        .map_err(|_| failed("FILE_WRITE_FAILED"))?;
                    drop(file);
                    // Detect changes while resolving the credential before replacing.
                    let current = Zeroizing::new(
                        fs::read_to_string(&path).map_err(|_| failed("FILE_UNAVAILABLE"))?,
                    );
                    if *current != *source {
                        return Err(failed("FILE_CHANGED"));
                    }
                    windows.replace_file(&path, &temporary)?;
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                result?;
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|_| failed("FILE_EXISTS_OR_NOT_WRITABLE"))?;
                if file
                    .write_all(rendered.as_bytes())
                    .and_then(|_| file.sync_all())
                    .is_err()
                {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(failed("FILE_WRITE_FAILED"));
                }
            }
            Ok(json!({"status":"file_written"}))
        }
    }
}

/// Hosts a credential belongs to: its website, its API addresses and its
/// database host. A prompt naming any other host is not answered.
pub(crate) fn entry_hosts(entry: &EntryDetail) -> Vec<String> {
    let mut hosts = Vec::new();
    let mut add = |value: &str| {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        let host = url::Url::parse(value)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .or_else(|| {
                url::Url::parse(&format!("https://{value}"))
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
            });
        if let Some(host) = host {
            let host = host.trim_matches(['[', ']']).to_lowercase();
            if !host.is_empty() && !hosts.contains(&host) {
                hosts.push(host);
            }
        }
    };
    add(&entry.summary.url);
    for service in &entry.summary.services {
        add(&service.url);
    }
    if let Some(database) = &entry.summary.database {
        add(&database.host);
    }
    hosts
}

fn entry_hosts_of(vault: &dyn CredentialReader, id: &str) -> Result<Vec<String>> {
    Ok(entry_hosts(&vault.get(id)?))
}

/// The account and secret an askpass helper may hand out, or `None` when the
/// group holds no secret.
fn askpass_answers(entry: &EntryDetail, group_id: Option<&str>) -> Option<AskpassAnswers> {
    let group = entry.group(group_id).ok()?;
    let secret = Zeroizing::new(group.field("secret").ok()?);
    if secret.is_empty() {
        return None;
    }
    let account = group
        .field("account")
        .ok()
        .filter(|account| !account.is_empty())
        .map(Zeroizing::new);
    Some(AskpassAnswers {
        account,
        secret,
        hosts: entry_hosts(entry),
    })
}

/// Shared local resolution: no plaintext from this function is an MCP response.
pub fn resolve_field(
    vault: &dyn CredentialReader,
    id: &str,
    field: &str,
    service_id: Option<&str>,
    group_id: Option<&str>,
) -> Result<String> {
    field_of(&vault.get(id)?, field, service_id, group_id)
}

pub(crate) fn field_of(
    entry: &EntryDetail,
    field: &str,
    service_id: Option<&str>,
    group_id: Option<&str>,
) -> Result<String> {
    Ok(match field {
        "account" | "environment" | "password" | "token" | "secret" => {
            entry.group(group_id)?.field(field)?
        }
        "url" => entry.summary.url.clone(),
        "service_url" => entry.summary.service(service_id)?.url.clone(),
        "host" | "port" | "database_name" | "database_type" => {
            let db = entry.summary.database.clone().ok_or_else(|| {
                AppError::invalid().with("field", "只有数据库凭据有连接信息字段。")
            })?;
            match field {
                "host" => db.host,
                "port" => db.port,
                "database_name" => db.database_name,
                _ => db.database_type,
            }
        }
        _ => return Err(AppError::invalid().with("field", "不支持的字段。")),
    })
}
