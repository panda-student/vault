//! Encrypted container for the remote sync file.
//!
//! Layout: `VAULTSYN` | version | m_cost | t_cost | p_cost | salt | nonce | ciphertext.
//! The salt stays fixed for a remote so the derived key can be cached; every
//! write uses a fresh nonce. The header is authenticated as associated data.
use super::{
    DEFAULT_KDF, Kdf, MAX_BYTES, MAX_ENTRIES, NONCE_LEN, TAG_LEN, derive_key, kdf_supported,
    open_after_header, random_failed, seal_after_header,
};
use serde::{Deserialize, Serialize};
use vault_contracts::{
    errors::{AppError, Result},
    sync::SyncRecord,
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"VAULTSYN";
const VERSION: u8 = 1;
const FORMAT_NAME: &str = "vault-sync";
const SALT_LEN: usize = 16;
const HEADER_LEN: usize = MAGIC.len() + 1 + 12 + SALT_LEN;

/// A key derived from the sync passphrase for one remote's salt.
pub struct SyncKey {
    salt: [u8; SALT_LEN],
    kdf: Kdf,
    key: Zeroizing<[u8; 32]>,
}
impl SyncKey {
    /// Key for a remote that does not exist yet: picks a new random salt.
    pub fn create(passphrase: &str) -> Result<Self> {
        require_passphrase(passphrase)?;
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt).map_err(|_| random_failed())?;
        Self::derive(passphrase, salt, DEFAULT_KDF)
    }
    /// Key matching an existing sync file's salt and KDF parameters.
    pub fn for_file(passphrase: &str, bytes: &[u8]) -> Result<Self> {
        require_passphrase(passphrase)?;
        let (salt, kdf) = header(bytes)?;
        Self::derive(passphrase, salt, kdf)
    }
    fn derive(passphrase: &str, salt: [u8; SALT_LEN], kdf: Kdf) -> Result<Self> {
        let key = derive_key(passphrase, &salt, &kdf)?;
        Ok(Self { salt, kdf, key })
    }
    /// Whether this cached key fits `bytes`. A different salt means another
    /// device recreated the file: derive again with `for_file`. Errors report
    /// a foreign, damaged or newer file.
    pub fn matches(&self, bytes: &[u8]) -> Result<bool> {
        let (salt, kdf) = header(bytes)?;
        Ok(salt == self.salt
            && (kdf.m_cost, kdf.t_cost, kdf.p_cost)
                == (self.kdf.m_cost, self.kdf.t_cost, self.kdf.p_cost))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    records: Vec<SyncRecord>,
}

/// True when `bytes` start like this program's sync file.
pub fn is_sync_file(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

pub fn seal(key: &SyncKey, records: &[SyncRecord]) -> Result<Vec<u8>> {
    if records.len() > MAX_ENTRIES {
        return Err(AppError::new(
            "SYNC_TOO_LARGE",
            "同步条目超过 10000 条上限。",
        ));
    }
    let payload = Zeroizing::new(
        serde_json::to_vec(&Envelope {
            format: FORMAT_NAME.into(),
            version: VERSION.into(),
            records: records.to_vec(),
        })
        .map_err(|_| AppError::invalid())?,
    );
    let mut header = Vec::with_capacity(HEADER_LEN + NONCE_LEN + payload.len() + TAG_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);
    key.kdf.write_le(&mut header);
    header.extend_from_slice(&key.salt);
    let sealed = seal_after_header(header, &key.key, &payload)?;
    if sealed.len() > MAX_BYTES {
        return Err(too_large());
    }
    Ok(sealed)
}

/// Decrypt with a key that `matches` the file; a stale key is reported apart
/// from a wrong passphrase so the caller can derive again.
pub fn open(key: &SyncKey, bytes: &[u8]) -> Result<Vec<SyncRecord>> {
    if bytes.len() > MAX_BYTES {
        return Err(too_large());
    }
    if !key.matches(bytes)? {
        return Err(AppError::new(
            "SYNC_KEY_STALE",
            "同步文件已在其他设备上重建，需要重新派生密钥。",
        ));
    }
    if bytes.len() < HEADER_LEN + NONCE_LEN + TAG_LEN {
        return Err(damaged());
    }
    let plain = open_after_header(bytes, HEADER_LEN, &key.key).ok_or_else(wrong_passphrase)?;
    let envelope: Envelope = serde_json::from_slice(&plain).map_err(|_| damaged())?;
    if envelope.format != FORMAT_NAME || envelope.version != u32::from(VERSION) {
        return Err(unsupported());
    }
    if envelope.records.len() > MAX_ENTRIES {
        return Err(damaged());
    }
    Ok(envelope.records)
}

fn header(bytes: &[u8]) -> Result<([u8; SALT_LEN], Kdf)> {
    if !is_sync_file(bytes) {
        return Err(AppError::new(
            "SYNC_FOREIGN_FILE",
            "同步路径上已有其他文件，不是本程序的同步文件，未做任何改动。请换一个路径。",
        ));
    }
    if bytes.len() < HEADER_LEN {
        return Err(damaged());
    }
    if bytes[8] != VERSION {
        return Err(unsupported());
    }
    let kdf = Kdf::read_le(&bytes[9..21]);
    if !kdf_supported(&kdf) {
        return Err(damaged());
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[21..HEADER_LEN]);
    Ok((salt, kdf))
}

fn require_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.is_empty() {
        return Err(AppError::invalid().with("passphrase", "同步口令不能为空。"));
    }
    Ok(())
}
fn wrong_passphrase() -> AppError {
    AppError::new(
        "SYNC_PASSPHRASE",
        "同步口令不正确，或同步文件已损坏，未写入任何数据。",
    )
}
fn damaged() -> AppError {
    AppError::new("SYNC_DAMAGED", "同步文件不完整或已损坏，未写入任何数据。")
}
fn unsupported() -> AppError {
    AppError::new(
        "SYNC_UNSUPPORTED",
        "同步文件来自更新的版本，请先升级本程序。",
    )
}
fn too_large() -> AppError {
    AppError::new("SYNC_TOO_LARGE", "同步文件超过 32 MB 上限。")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vault_contracts::credentials::{CredentialGroup, EntryInput, Kind};

    fn record(id: &str) -> SyncRecord {
        SyncRecord {
            id: id.into(),
            value: EntryInput {
                kind: Kind::Password,
                title: "邮箱".into(),
                tags: vec![],
                database: None,
                url: "https://mail.example.com".into(),
                documentation_url: String::new(),
                services: vec![],
                notes: String::new(),
                groups: vec![CredentialGroup {
                    id: "g".into(),
                    account: "me".into(),
                    password: "FAKE_SYNC_SECRET".into(),
                    ..Default::default()
                }],
            },
            pinned: false,
            created_at: 1,
            updated_at: 2,
            deleted_at: None,
            device: "PC".into(),
        }
    }
    fn cheap_with(passphrase: &str, salt: u8) -> SyncKey {
        let kdf = Kdf {
            m_cost: 64,
            t_cost: 1,
            p_cost: 1,
        };
        SyncKey::derive(passphrase, [salt; SALT_LEN], kdf).unwrap()
    }
    fn cheap(passphrase: &str) -> SyncKey {
        cheap_with(passphrase, 7)
    }

    #[test]
    fn roundtrip_hides_secrets_and_reuses_salt() {
        let key = cheap("right");
        let bytes = seal(&key, &[record("a")]).unwrap();
        assert!(is_sync_file(&bytes));
        assert!(!String::from_utf8_lossy(&bytes).contains("FAKE_SYNC_SECRET"));
        assert_eq!(open(&key, &bytes).unwrap()[0].id, "a");
        let again = SyncKey::for_file("right", &bytes).unwrap();
        assert!(again.matches(&bytes).unwrap());
        assert_eq!(open(&again, &bytes).unwrap().len(), 1);
        // A second write keeps the salt but changes the nonce.
        let second = seal(&key, &[record("a")]).unwrap();
        assert_eq!(bytes[..HEADER_LEN], second[..HEADER_LEN]);
        assert_ne!(bytes, second);
    }

    #[test]
    fn wrong_passphrase_stale_key_and_tampering_are_told_apart() {
        let bytes = seal(&cheap("right"), &[record("a")]).unwrap();
        let wrong = SyncKey::for_file("wrong", &bytes).unwrap();
        assert_eq!(open(&wrong, &bytes).unwrap_err().code, "SYNC_PASSPHRASE");
        // A key for another salt (file recreated elsewhere) is stale, not wrong.
        assert_eq!(
            open(&cheap_with("right", 8), &bytes).unwrap_err().code,
            "SYNC_KEY_STALE"
        );
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert_eq!(
            open(&cheap("right"), &tampered).unwrap_err().code,
            "SYNC_PASSPHRASE"
        );
    }

    #[test]
    fn foreign_and_newer_files_report_their_real_cause() {
        let foreign = b"# notes.md\nsomething else".to_vec();
        assert!(!is_sync_file(&foreign));
        assert_eq!(
            SyncKey::for_file("x", &foreign).err().unwrap().code,
            "SYNC_FOREIGN_FILE"
        );
        assert_eq!(
            open(&cheap("x"), &foreign).unwrap_err().code,
            "SYNC_FOREIGN_FILE"
        );
        let mut newer = seal(&cheap("x"), &[]).unwrap();
        newer[8] = 9;
        assert_eq!(
            open(&cheap("x"), &newer).unwrap_err().code,
            "SYNC_UNSUPPORTED"
        );
        assert_eq!(SyncKey::create("").err().unwrap().code, "INVALID_INPUT");
    }

    #[test]
    fn hostile_kdf_headers_are_rejected_without_overflow() {
        let mut bytes = seal(&cheap("x"), &[]).unwrap();
        bytes[17..21].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(open(&cheap("x"), &bytes).unwrap_err().code, "SYNC_DAMAGED");
    }
}
