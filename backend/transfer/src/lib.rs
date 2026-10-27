//! Human-initiated vault export/import formats. Pure encoding: no storage, SQL,
//! caller identity or native calls. Validation of credential rules stays in core.
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use vault_contracts::{
    credentials::{ApiService, DatabaseFields, EntryInput, Kind},
    errors::{AppError, Result},
};
use zeroize::Zeroizing;

pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 10_000;
const FORMAT_NAME: &str = "vault-export";
const VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"VAULTEXP";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 4 + 4 + 4 + SALT_LEN;
const MIN_ENCRYPTED_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;
const MAX_KDF_MEMORY: u32 = 1024 * 1024;
const MAX_KDF_PASSES: u32 = 16;
const MAX_KDF_LANES: u32 = 8;

const CSV_HEADER: [&str; 13] = [
    "name",
    "url",
    "username",
    "password",
    "note",
    "type",
    "tags",
    "services",
    "documentation_url",
    "database_type",
    "host",
    "port",
    "database_name",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Csv,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
    fn code(self) -> u8 {
        match self {
            Self::Json => 1,
            Self::Csv => 2,
        }
    }
    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Json),
            2 => Some(Self::Csv),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    exported_at: i64,
    entries: Vec<EntryInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inspection {
    pub encrypted: bool,
    pub format: Format,
}

#[derive(Debug)]
pub struct Imported {
    /// 1-based row shown to the user: CSV physical row or JSON array position.
    pub row: usize,
    pub value: EntryInput,
}

#[derive(Debug)]
pub struct Decoded {
    pub format: Format,
    pub entries: Vec<Imported>,
}

pub(crate) struct Kdf {
    pub(crate) m_cost: u32,
    pub(crate) t_cost: u32,
    pub(crate) p_cost: u32,
}
pub(crate) const DEFAULT_KDF: Kdf = Kdf {
    m_cost: 65536,
    t_cost: 3,
    p_cost: 1,
};

pub fn encode(format: Format, entries: &[EntryInput]) -> Result<Vec<u8>> {
    check_entries(entries)?;
    let bytes = match format {
        Format::Json => json_payload(entries)?,
        Format::Csv => csv_payload(entries),
    };
    check_size(bytes.len())?;
    Ok(bytes)
}

pub fn encode_encrypted(
    format: Format,
    entries: &[EntryInput],
    passphrase: &str,
) -> Result<Vec<u8>> {
    encode_encrypted_with(format, entries, passphrase, DEFAULT_KDF)
}

pub(crate) fn encode_encrypted_with(
    format: Format,
    entries: &[EntryInput],
    passphrase: &str,
    kdf: Kdf,
) -> Result<Vec<u8>> {
    if passphrase.is_empty() {
        return Err(AppError::invalid().with("passphrase", "口令不能为空。"));
    }
    check_entries(entries)?;
    let payload = match format {
        Format::Json => json_payload(entries)?,
        Format::Csv => csv_payload(entries),
    };
    check_size(payload.len())?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::fill(&mut salt)
        .and_then(|_| getrandom::fill(&mut nonce_bytes))
        .map_err(|_| AppError::new("STORAGE_ERROR", "无法生成安全随机数。"))?;

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VERSION as u8);
    header.push(format.code());
    header.extend_from_slice(&kdf.m_cost.to_le_bytes());
    header.extend_from_slice(&kdf.t_cost.to_le_bytes());
    header.extend_from_slice(&kdf.p_cost.to_le_bytes());
    header.extend_from_slice(&salt);
    let aad = header.clone();
    header.extend_from_slice(&nonce_bytes);

    let key = derive_key(passphrase, &salt, &kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key[..])
        .map_err(|_| AppError::new("CRYPTO_ERROR", "无法初始化加密器。"))?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &payload,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::new("CRYPTO_ERROR", "无法加密导出文件。"))?;
    header.extend_from_slice(&ciphertext);
    check_size(header.len())?;
    Ok(header)
}

pub fn inspect(bytes: &[u8]) -> Result<Inspection> {
    if bytes.is_empty() {
        return Err(invalid_file("文件为空。".into()));
    }
    if bytes.starts_with(MAGIC) {
        if bytes.len() < MIN_ENCRYPTED_LEN {
            return Err(invalid_file("加密导出文件不完整。".into()));
        }
        if bytes[8] as u32 != VERSION {
            return Err(unsupported_version());
        }
        let format =
            Format::from_code(bytes[9]).ok_or_else(|| invalid_file("未知的导出格式。".into()))?;
        return Ok(Inspection {
            encrypted: true,
            format,
        });
    }
    let text = plain_text(bytes)?;
    Ok(Inspection {
        encrypted: false,
        format: detect_plain(text),
    })
}

pub fn decode(bytes: &[u8], passphrase: Option<&str>) -> Result<Decoded> {
    if bytes.is_empty() {
        return Err(invalid_file("文件为空。".into()));
    }
    check_size(bytes.len())?;
    if bytes.starts_with(MAGIC) {
        return decode_encrypted(bytes, passphrase);
    }
    let text = plain_text(bytes)?;
    decode_payload(detect_plain(text), text.as_bytes())
}

fn decode_encrypted(bytes: &[u8], passphrase: Option<&str>) -> Result<Decoded> {
    if bytes.len() < MIN_ENCRYPTED_LEN {
        return Err(invalid_file("加密导出文件不完整。".into()));
    }
    if bytes[8] as u32 != VERSION {
        return Err(unsupported_version());
    }
    let format =
        Format::from_code(bytes[9]).ok_or_else(|| invalid_file("未知的导出格式。".into()))?;
    let kdf = Kdf {
        m_cost: u32::from_le_bytes(bytes[10..14].try_into().unwrap()),
        t_cost: u32::from_le_bytes(bytes[14..18].try_into().unwrap()),
        p_cost: u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
    };
    if kdf.m_cost < 8 * kdf.p_cost.max(1)
        || kdf.m_cost > MAX_KDF_MEMORY
        || kdf.t_cost == 0
        || kdf.t_cost > MAX_KDF_PASSES
        || kdf.p_cost == 0
        || kdf.p_cost > MAX_KDF_LANES
    {
        return Err(invalid_file("导出文件使用了不受支持的加密参数。".into()));
    }
    let passphrase = passphrase
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::new("PASSPHRASE_REQUIRED", "该文件已加密，请输入口令。"))?;
    let salt = &bytes[HEADER_LEN - SALT_LEN..HEADER_LEN];
    let nonce_bytes = &bytes[HEADER_LEN..HEADER_LEN + NONCE_LEN];
    let key = derive_key(passphrase, salt, &kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key[..])
        .map_err(|_| invalid_file("无法初始化解密器。".into()))?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(nonce_bytes),
            Payload {
                msg: &bytes[HEADER_LEN + NONCE_LEN..],
                aad: &bytes[..HEADER_LEN],
            },
        )
        .map_err(|_| decrypt_failed())?;
    decode_payload(format, &plain)
}

fn decode_payload(format: Format, payload: &[u8]) -> Result<Decoded> {
    let text = std::str::from_utf8(payload)
        .map_err(|_| invalid_file("文件不是有效的 UTF-8 文本。".into()))?;
    match format {
        Format::Json => decode_json(text),
        Format::Csv => decode_csv(text.strip_prefix('\u{feff}').unwrap_or(text)),
    }
}

fn decode_json(text: &str) -> Result<Decoded> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| invalid_file("JSON 结构不完整或无法解析。".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid_file("JSON 根节点必须是对象。".into()))?;
    if object.get("format").and_then(|v| v.as_str()) != Some(FORMAT_NAME) {
        return Err(invalid_file("不是受支持的导出文件。".into()));
    }
    match object.get("version").and_then(|v| v.as_u64()) {
        Some(version) if version == VERSION as u64 => {}
        Some(_) => return Err(unsupported_version()),
        None => return Err(invalid_file("导出文件缺少版本号。".into())),
    }
    let raw = object
        .get("entries")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid_file("导出文件缺少 entries 数组。".into()))?;
    if raw.len() > MAX_ENTRIES {
        return Err(too_many());
    }
    let mut entries = Vec::with_capacity(raw.len());
    for (index, item) in raw.iter().enumerate() {
        let value = serde_json::from_value::<EntryInput>(item.clone())
            .map_err(|_| invalid_file(format!("第 {} 条：字段格式不正确。", index + 1)))?;
        entries.push(Imported {
            row: index + 1,
            value,
        });
    }
    Ok(Decoded {
        format: Format::Json,
        entries,
    })
}

fn decode_csv(text: &str) -> Result<Decoded> {
    let records = parse_csv(text)?;
    let Some((_, header)) = records.first() else {
        return Err(invalid_file("CSV 文件为空。".into()));
    };
    let index: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(position, name)| (name.trim().to_ascii_lowercase(), position))
        .collect();
    index
        .get("name")
        .ok_or_else(|| invalid_file("CSV 缺少 name 列。".into()))?;
    let mut entries = Vec::new();
    for (row, fields) in records.iter().skip(1) {
        if fields.iter().all(|field| field.trim().is_empty()) {
            continue;
        }
        if fields.len() < header.len() {
            return Err(invalid_file(format!(
                "第 {row} 行：列数为 {}，少于表头的 {} 列。",
                fields.len(),
                header.len()
            )));
        }
        entries.push(Imported {
            row: *row,
            value: csv_entry(*row, &index, fields)?,
        });
    }
    if entries.len() > MAX_ENTRIES {
        return Err(too_many());
    }
    Ok(Decoded {
        format: Format::Csv,
        entries,
    })
}

fn csv_entry(row: usize, index: &HashMap<String, usize>, fields: &[String]) -> Result<EntryInput> {
    let get = |name: &str| -> &str {
        index
            .get(name)
            .and_then(|position| fields.get(*position))
            .map(String::as_str)
            .unwrap_or("")
    };
    let kind = match get("type").trim().to_ascii_lowercase().as_str() {
        "" | "password" => Kind::Password,
        "api" => Kind::Api,
        "database" | "db" => Kind::Database,
        "generic" => Kind::Generic,
        other => {
            return Err(invalid_file(format!(
                "第 {row} 行：类型 “{other}” 不受支持。"
            )));
        }
    };
    let tags = parse_tags(row, get("tags"))?;
    let services: Vec<ApiService> = if get("services").trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(get("services"))
            .map_err(|_| invalid_file(format!("第 {row} 行：services 字段格式不正确。")))?
    };
    let database_type = get("database_type").trim();
    let host = get("host").trim();
    let port = get("port").trim();
    let database_name = get("database_name").trim();
    let database = if [database_type, host, port, database_name]
        .iter()
        .any(|value| !value.is_empty())
    {
        Some(DatabaseFields {
            database_type: database_type.into(),
            host: host.into(),
            port: port.into(),
            database_name: database_name.into(),
        })
    } else {
        None
    };
    Ok(EntryInput {
        kind,
        title: get("name").to_string(),
        tags,
        database,
        account: get("username").to_string(),
        url: get("url").to_string(),
        documentation_url: get("documentation_url").to_string(),
        services,
        secret: get("password").to_string(),
        notes: get("note").to_string(),
    })
}

fn parse_tags(row: usize, value: &str) -> Result<Vec<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if value.starts_with('[') {
        return serde_json::from_str(value)
            .map_err(|_| invalid_file(format!("第 {row} 行：tags 字段格式不正确。")));
    }
    Ok(value
        .split(';')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect())
}

/// RFC 4180 reader that keeps the 1-based record number for user-facing errors.
fn parse_csv(text: &str) -> Result<Vec<(usize, Vec<String>)>> {
    let mut chars = text.chars().peekable();
    let mut records = Vec::new();
    let mut row = 1usize;
    loop {
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut quoted = false;
        let mut started = false;
        loop {
            match chars.next() {
                None => {
                    if quoted {
                        return Err(invalid_file(format!("第 {row} 行：引号未闭合。")));
                    }
                    if started || !field.is_empty() {
                        fields.push(field);
                    }
                    if !fields.is_empty() {
                        records.push((row, fields));
                    }
                    return Ok(records);
                }
                Some('"') => {
                    started = true;
                    if quoted {
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            field.push('"');
                        } else {
                            quoted = false;
                        }
                    } else if field.is_empty() {
                        quoted = true;
                    } else {
                        return Err(invalid_file(format!("第 {row} 行：引号位置不正确。")));
                    }
                }
                Some(',') if !quoted => {
                    started = true;
                    fields.push(std::mem::take(&mut field));
                }
                Some('\r') if !quoted => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    fields.push(std::mem::take(&mut field));
                    records.push((row, fields));
                    row += 1;
                    break;
                }
                Some('\n') if !quoted => {
                    fields.push(std::mem::take(&mut field));
                    records.push((row, fields));
                    row += 1;
                    break;
                }
                Some(value) => {
                    started = true;
                    field.push(value);
                }
            }
        }
    }
}

fn json_payload(entries: &[EntryInput]) -> Result<Vec<u8>> {
    let envelope = Envelope {
        format: FORMAT_NAME.into(),
        version: VERSION,
        exported_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default()
            .max(1),
        entries: entries.to_vec(),
    };
    serde_json::to_vec(&envelope)
        .map_err(|_| AppError::new("STORAGE_ERROR", "无法序列化导出内容。"))
}

fn csv_payload(entries: &[EntryInput]) -> Vec<u8> {
    let mut text = String::new();
    text.push_str(&CSV_HEADER.join(","));
    text.push_str("\r\n");
    for entry in entries {
        let database = entry.database.clone().unwrap_or_default();
        let row = [
            entry.title.clone(),
            entry.url.clone(),
            entry.account.clone(),
            entry.secret.clone(),
            entry.notes.clone(),
            kind_name(entry.kind).to_string(),
            serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&entry.services).unwrap_or_else(|_| "[]".into()),
            entry.documentation_url.clone(),
            database.database_type,
            database.host,
            database.port,
            database.database_name,
        ];
        for (position, value) in row.iter().enumerate() {
            if position > 0 {
                text.push(',');
            }
            push_csv_field(&mut text, value);
        }
        text.push_str("\r\n");
    }
    text.into_bytes()
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Password => "password",
        Kind::Api => "api",
        Kind::Database => "database",
        Kind::Generic => "generic",
    }
}

fn push_csv_field(output: &mut String, value: &str) {
    if value.contains([',', '"', '\r', '\n']) {
        output.push('"');
        for character in value.chars() {
            if character == '"' {
                output.push('"');
            }
            output.push(character);
        }
        output.push('"');
    } else {
        output.push_str(value);
    }
}

fn derive_key(passphrase: &str, salt: &[u8], kdf: &Kdf) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(32))
        .map_err(|_| invalid_file("Argon2 参数无效。".into()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key[..])
        .map_err(|_| invalid_file("无法派生加密密钥。".into()))?;
    Ok(key)
}

fn plain_text(bytes: &[u8]) -> Result<&str> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_file("文件不是有效的 UTF-8 文本。".into()))?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text))
}

fn detect_plain(text: &str) -> Format {
    if text.trim_start().starts_with('{') {
        Format::Json
    } else {
        Format::Csv
    }
}

fn check_entries(entries: &[EntryInput]) -> Result<()> {
    if entries.len() > MAX_ENTRIES {
        return Err(too_many());
    }
    Ok(())
}

fn check_size(size: usize) -> Result<()> {
    if size > MAX_BYTES {
        return Err(AppError::new(
            "FILE_TOO_LARGE",
            "导出内容超过 32 MB 上限，请减少条目后重试。",
        ));
    }
    Ok(())
}

fn too_many() -> AppError {
    AppError::invalid().with("entries", "一次最多导入或导出 10000 条凭据。")
}

fn unsupported_version() -> AppError {
    AppError::new(
        "UNSUPPORTED_EXPORT_VERSION",
        "导出文件版本不受支持，请升级程序后再导入。",
    )
}

pub(crate) fn invalid_file(reason: String) -> AppError {
    AppError::new("INVALID_EXPORT", "导入文件格式不正确，未导入任何条目。").with("file", &reason)
}

pub(crate) fn decrypt_failed() -> AppError {
    AppError::new("DECRYPT_FAILED", "口令错误或文件已损坏，未导入任何条目。")
}

#[cfg(test)]
mod tests;
