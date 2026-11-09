//! Remote sync: one encrypted file at a user-chosen remote, merged three ways.
//!
//! Order of one run: read remote → decrypt → merge with the local snapshot and
//! the last agreed base → write the remote conditionally (retry on a concurrent
//! write) → apply locally in one transaction → save the new base. The network
//! is never used while the execution gate is held.
mod setup;
mod worker;

use crate::context::{CoreSession, now};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use vault_contracts::{
    errors::{AppError, Result},
    ports::{
        repository::CredentialReader,
        sync::{RemoteStore, RemoteStoreFactory},
    },
    sync::{
        CredentialRef, DEFAULT_INTERVAL, ResolvedTarget, SyncChoice, SyncConflict, SyncError,
        SyncOutcome, SyncRecord, SyncStatusView, SyncTarget,
    },
};
use vault_sync::merge::{self, Base, Input};
use vault_transfer::sync::{self as container, SyncKey};
use zeroize::{Zeroize, Zeroizing};

const CONFIG: &str = "config";
const STATE: &str = "state";
const STATUS: &str = "status";
/// Reads, merges and conditional writes tried before giving up on a busy remote.
const ATTEMPTS: u32 = 4;
/// Wait before the next attempt, doubling each time. A remote such as GitHub
/// can keep serving the previous version for a moment after another device
/// wrote it; retrying at once would read the same stale version again.
#[cfg(not(test))]
const RETRY_BASE: std::time::Duration = std::time::Duration::from_millis(750);
#[cfg(test)]
const RETRY_BASE: std::time::Duration = std::time::Duration::from_millis(1);

pub(crate) struct SyncService {
    stores: Arc<dyn RemoteStoreFactory>,
    /// One run, setup or disable at a time.
    run: Mutex<()>,
    running: AtomicBool,
    last_run: Mutex<Option<Instant>>,
    /// Key derived from the passphrase for the remote's salt; argon2 runs once.
    key: Mutex<Option<SyncKey>>,
    pub(crate) requests: setup::Requests,
    pub(crate) schedule: worker::Schedule,
}
impl SyncService {
    pub(crate) fn new(stores: Arc<dyn RemoteStoreFactory>) -> Self {
        Self {
            stores,
            run: Mutex::new(()),
            running: AtomicBool::new(false),
            last_run: Mutex::new(None),
            key: Mutex::new(None),
            requests: Default::default(),
            schedule: Default::default(),
        }
    }
    pub(crate) fn mark_dirty(&self) {
        self.schedule.mark_dirty();
    }
    fn lock_run(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.run.lock().map_err(|_| AppError::storage())
    }
    fn set_key(&self, key: Option<SyncKey>) -> Result<()> {
        *self.key.lock().map_err(|_| AppError::storage())? = key;
        Ok(())
    }
}

/// Saved encrypted in the vault. Never serialized to a client.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncConfig {
    target: SyncTarget,
    passphrase: String,
    interval_minutes: u32,
    device_name: String,
}
impl Drop for SyncConfig {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}

/// The base agreed with one target; a different target starts from no base.
#[derive(Default, Serialize, Deserialize)]
struct SyncState {
    target: Option<SyncTarget>,
    base: Base,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredStatus {
    last_sync_at: Option<i64>,
    last_outcome: SyncOutcome,
    error: Option<SyncError>,
    conflicts: Vec<SyncConflict>,
    first_join: bool,
}

struct Report {
    outcome: SyncOutcome,
    conflicts: Vec<SyncConflict>,
    first_join: bool,
}

/// Clears the running flag however a run ends.
struct Running<'a>(&'a AtomicBool);
impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl CoreSession {
    /// Run one sync. `choice` settles pending conflicts or a first join.
    pub(crate) fn sync_run(&self, choice: Option<SyncChoice>) -> Result<SyncStatusView> {
        let sync = &self.service.sync;
        let run = sync.lock_run()?;
        let config = self.sync_config()?.ok_or_else(not_configured)?;
        sync.running.store(true, Ordering::Release);
        let running = Running(&sync.running);
        if let Ok(mut last) = sync.last_run.lock() {
            *last = Some(Instant::now());
        }
        let result = self.sync_with(&config, choice);
        // The sync's own error matters more than a failure to record it.
        let recorded = self.record(&result);
        drop(running);
        drop(run);
        result?;
        recorded?;
        self.sync_status()
    }

    fn record(&self, result: &Result<Report>) -> Result<()> {
        let status = match result {
            Ok(report) => StoredStatus {
                last_sync_at: Some(now()),
                last_outcome: report.outcome,
                error: None,
                conflicts: report.conflicts.clone(),
                first_join: report.first_join,
            },
            Err(error) => StoredStatus {
                error: Some(SyncError {
                    code: error.code.into(),
                    message: error.message.into(),
                }),
                ..self.load(STATUS)?.unwrap_or_default()
            },
        };
        self.save(STATUS, Some(&status))
    }

    /// Read, merge, write the remote, then apply locally; a concurrent remote
    /// write sends the whole round back to reading.
    fn sync_with(&self, config: &SyncConfig, choice: Option<SyncChoice>) -> Result<Report> {
        let store = self.open_store(&config.target)?;
        let state = self.load::<SyncState>(STATE)?.unwrap_or_default();
        let base = (state.target.as_ref() == Some(&config.target)).then_some(&state.base);
        let mut cache = self
            .service
            .sync
            .key
            .lock()
            .map_err(|_| AppError::storage())?;
        for attempt in 1..=ATTEMPTS {
            let remote = store.read()?;
            let bytes = remote.as_ref().map(|object| object.bytes.as_slice());
            let key = key_for(&mut cache, &config.passphrase, bytes)?;
            let remote_records = bytes
                .map(|bytes| self.checked_records(container::open(key, bytes)?))
                .transpose()?;
            let snapshot = self.gated(|| self.service.storage.sync_snapshot())?;
            let mut local = snapshot.records;
            for record in &mut local {
                record.device.clone_from(&config.device_name);
            }
            let merged = merge::merge(
                &Input {
                    local: &local,
                    remote: remote_records.as_deref(),
                    base,
                    now: now(),
                },
                choice,
            );
            if merged.first_join {
                return Ok(Report {
                    outcome: merged.outcome,
                    conflicts: merged.conflicts,
                    first_join: true,
                });
            }
            if merged.write_remote {
                let sealed = container::seal(key, &merged.remote)?;
                let expected = remote.as_ref().map(|object| object.version.as_str());
                match store.write(&sealed, expected) {
                    Err(error) if error.code == "SYNC_REMOTE_CHANGED" && attempt < ATTEMPTS => {
                        std::thread::sleep(RETRY_BASE * 2u32.pow(attempt - 1));
                        continue;
                    }
                    written => written?,
                };
            }
            self.apply_local(config, merged.local, &snapshot.slots, merged.base)?;
            return Ok(Report {
                outcome: merged.outcome,
                conflicts: merged.conflicts,
                first_join: false,
            });
        }
        Err(AppError::new(
            "SYNC_REMOTE_CHANGED",
            "远端同步文件持续被其他设备修改，请稍后重试。",
        ))
    }

    /// Make this computer match the written remote and remember the agreement.
    /// An entry edited here since the snapshot is skipped; it then differs
    /// from the new base, so the next run pushes it.
    fn apply_local(
        &self,
        config: &SyncConfig,
        changes: Vec<vault_contracts::sync::LocalChange>,
        slots: &std::collections::BTreeMap<String, vault_contracts::sync::SyncSlot>,
        base: Base,
    ) -> Result<()> {
        self.gated(|| self.service.storage.apply_sync(changes, slots))?;
        self.save(
            STATE,
            Some(&SyncState {
                target: Some(config.target.clone()),
                base,
            }),
        )
    }

    /// Records from another device go through the same checks as an import.
    fn checked_records(&self, mut records: Vec<SyncRecord>) -> Result<Vec<SyncRecord>> {
        let mut seen = BTreeSet::new();
        for record in &mut records {
            if uuid::Uuid::parse_str(&record.id).is_err() || !seen.insert(record.id.clone()) {
                return Err(invalid_remote());
            }
            self.service
                .storage
                .prepare_import(&mut record.value)
                .map_err(|_| invalid_remote())?;
        }
        Ok(records)
    }

    fn open_store(&self, target: &SyncTarget) -> Result<Box<dyn RemoteStore>> {
        self.service.sync.stores.open(&self.resolve_target(target)?)
    }

    /// The target with its secrets read from the referenced vault entry, read
    /// fresh on every run so a rotated token takes effect at once.
    fn resolve_target(&self, target: &SyncTarget) -> Result<ResolvedTarget> {
        Ok(match target {
            SyncTarget::Folder { directory, path } => ResolvedTarget::Folder {
                directory: folder_directory(directory, self.directory())?,
                path: path.clone(),
            },
            SyncTarget::Webdav {
                url,
                path,
                credential,
            } => {
                let (account, password) = self.credential_secret(credential, |g| &g.password)?;
                ResolvedTarget::Webdav {
                    url: url.clone(),
                    path: path.clone(),
                    username: account,
                    password,
                }
            }
            SyncTarget::Github {
                owner,
                repo,
                branch,
                path,
                token,
            } => {
                let (_, token) = self.credential_secret(token, |g| {
                    if g.token.is_empty() {
                        &g.password
                    } else {
                        &g.token
                    }
                })?;
                ResolvedTarget::Github {
                    owner: owner.clone(),
                    repo: repo.clone(),
                    branch: branch.clone(),
                    path: path.clone(),
                    token,
                }
            }
        })
    }

    /// Account and one secret of the referenced group. Trusted internal read:
    /// the values only reach the remote store, never any output.
    fn credential_secret(
        &self,
        reference: &CredentialRef,
        pick: impl Fn(&vault_contracts::credentials::CredentialGroup) -> &String,
    ) -> Result<(String, Zeroizing<String>)> {
        let detail = self
            .service
            .storage
            .get(&reference.credential_id)
            .map_err(|_| credential_missing())?;
        let group = detail
            .group(reference.group_id.as_deref())
            .map_err(|_| credential_missing())?;
        let secret = pick(group);
        if secret.is_empty() {
            return Err(credential_missing());
        }
        Ok((group.account.clone(), Zeroizing::new(secret.clone())))
    }

    pub(crate) fn sync_status(&self) -> Result<SyncStatusView> {
        let running = self.service.sync.running.load(Ordering::Acquire);
        let Some(config) = self.sync_config()? else {
            return Ok(SyncStatusView {
                interval_minutes: DEFAULT_INTERVAL,
                device_name: default_device(),
                running,
                ..Default::default()
            });
        };
        let status = self.load::<StoredStatus>(STATUS)?.unwrap_or_default();
        Ok(SyncStatusView {
            configured: true,
            location: Some(config.target.summary()),
            credential_name: config
                .target
                .credential()
                .and_then(|r| self.credential_name(&r.credential_id)),
            target: Some(config.target.clone()),
            interval_minutes: config.interval_minutes,
            device_name: config.device_name.clone(),
            running,
            last_sync_at: status.last_sync_at,
            last_outcome: status.last_outcome,
            error: status.error,
            conflicts: status.conflicts,
            first_join: status.first_join,
        })
    }

    /// The status for an MCP caller: entry names in conflicts may belong to
    /// entries its permission rules hide, so only the count is kept.
    pub(crate) fn sync_status_for_mcp(&self) -> Result<SyncStatusView> {
        let mut view = self.sync_status()?;
        view.conflicts.clear();
        view.credential_name = None;
        // The referenced entry may be one the AI's permission rules hide.
        view.target = None;
        Ok(view)
    }

    fn credential_name(&self, id: &str) -> Option<String> {
        self.service.storage.summary(id).ok().map(|s| s.name)
    }

    pub(crate) fn sync_config(&self) -> Result<Option<SyncConfig>> {
        self.load(CONFIG)
    }

    /// The saved sync passphrase, so MCP output never echoes it.
    /// A sync config that cannot be read holds no passphrase anyone can see,
    /// so it must not stop every MCP call; sync itself reports the damage.
    pub(crate) fn sync_passphrase(&self) -> Option<Zeroizing<String>> {
        match self.sync_config() {
            Ok(config) => config.map(|config| Zeroizing::new(config.passphrase.clone())),
            Err(_) => None,
        }
    }

    fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        self.service
            .storage
            .sync_blob(name)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| AppError::storage()))
            .transpose()
    }

    /// Callers never hold the execution gate: sync work runs outside it and
    /// takes it only for each storage write, so an update install waits.
    fn save<T: Serialize>(&self, name: &str, value: Option<&T>) -> Result<()> {
        let bytes = value
            .map(|value| serde_json::to_vec(value).map(Zeroizing::new))
            .transpose()
            .map_err(|_| AppError::storage())?;
        self.gated(|| {
            self.service
                .storage
                .store_sync_blob(name, bytes.as_ref().map(|b| b.as_slice()))
        })
    }
}

/// The cached key if it fits the remote file, else one derived for it; a
/// missing remote gets a new salt.
fn key_for<'a>(
    cache: &'a mut Option<SyncKey>,
    passphrase: &str,
    bytes: Option<&[u8]>,
) -> Result<&'a SyncKey> {
    let fits = match (cache.as_ref(), bytes) {
        (Some(key), Some(bytes)) => key.matches(bytes)?,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if !fits {
        *cache = Some(match bytes {
            Some(bytes) => SyncKey::for_file(passphrase, bytes)?,
            None => SyncKey::create(passphrase)?,
        });
    }
    cache.as_ref().ok_or_else(AppError::storage)
}

/// A folder target must be absolute and must not be the vault's own data directory.
fn folder_directory(directory: &str, data: &std::path::Path) -> Result<PathBuf> {
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err(AppError::invalid().with("directory", "请选择绝对路径的文件夹。"));
    }
    let same = match (
        std::fs::canonicalize(&directory),
        std::fs::canonicalize(data),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => directory == data,
    };
    if same {
        return Err(
            AppError::invalid().with("directory", "不能把密码库自己的数据目录用作同步位置。")
        );
    }
    Ok(directory)
}

fn default_device() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "此电脑".into())
}

fn not_configured() -> AppError {
    AppError::new("SYNC_NOT_CONFIGURED", "尚未设置同步位置。")
}

fn credential_missing() -> AppError {
    AppError::new(
        "SYNC_CREDENTIAL_MISSING",
        "同步引用的凭据不存在、已移入回收站，或该凭据组没有所需的密码或 Token。",
    )
}

fn invalid_remote() -> AppError {
    AppError::new(
        "SYNC_DAMAGED",
        "远端同步文件中有无法识别的条目，未做任何改动。",
    )
}
