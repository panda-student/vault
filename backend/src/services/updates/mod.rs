//! Signed portable updates. Network metadata never supplies local paths or commands.
mod health;
pub(crate) mod installation;
mod package;
mod runtime;
mod verification;
use crate::definitions::errors::{AppError, Result};
use crate::definitions::{
    ports::system::{PauseLease, SystemPlatform},
    updates::UpdateLaunch,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const ENDPOINT: &str =
    "https://github.com/panda-student/vault/releases/latest/download/latest.json";
const MAX_PACKAGE: u64 = 64 * 1024 * 1024;
const PUBLIC_KEY: &str = include_str!("../../../update-public-key.txt");
// Only debug builds can use an isolated local test server. Shipping builds have
// no endpoint/key override and always use the embedded production public key.
fn test_source() -> Option<String> {
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("VAULT_UPDATE_TEST_SOURCE") {
        if let Ok(url) = reqwest::Url::parse(&value) {
            if url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some()
                && url.path() == "/latest.json"
                && url.username().is_empty()
                && url.password().is_none()
            {
                return Some(value);
            }
        }
    }
    None
}
fn failure(code: &'static str, message: &'static str) -> AppError {
    AppError::new(code, message)
}
fn network_error() -> AppError {
    failure("UPDATE_NETWORK", "无法连接更新源，请检查网络或代理后重试。")
}
fn invalid_update() -> AppError {
    failure("UPDATE_INVALID", "更新包或签名校验失败，原程序未被替换。")
}
fn busy() -> AppError {
    failure("UPDATE_BUSY", "更新操作正在进行，请稍后重试。")
}

use crate::definitions::updates::{Envelope, InstallPlan, Manifest, UpdateStatus};
use package::{extract_update_payload, verify_binary, verify_file};
pub use runtime::UpdateRuntime;
use runtime::runtime_paths;
use verification::decode;
#[derive(Default)]
struct State {
    status: UpdateStatus,
    envelope: Option<Envelope>,
    manifest: Option<Manifest>,
    ready: Option<PathBuf>,
}
pub(crate) struct UpdateService {
    platform: Arc<dyn SystemPlatform>,
    state: Mutex<State>,
    runtime: Mutex<Option<Arc<UpdateRuntime>>>,
    cancelled: AtomicBool,
    installing: AtomicBool,
}
impl UpdateService {
    pub(crate) fn new(platform: Arc<dyn SystemPlatform>) -> Self {
        Self {
            platform,
            state: Mutex::new(State {
                status: UpdateStatus {
                    phase: "idle".into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            runtime: Mutex::new(None),
            cancelled: AtomicBool::new(false),
            installing: AtomicBool::new(false),
        }
    }
}
impl UpdateService {
    pub fn bind(&self, runtime: Arc<UpdateRuntime>) -> Result<()> {
        *self.runtime.lock().map_err(|_| busy())? = Some(runtime);
        Ok(())
    }
    fn runtime(&self) -> Result<Arc<UpdateRuntime>> {
        self.runtime
            .lock()
            .map_err(|_| busy())?
            .clone()
            .ok_or_else(|| failure("UPDATE_UNAVAILABLE", "更新服务尚未就绪。"))
    }
    pub fn installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }
    pub fn status(&self) -> Result<UpdateStatus> {
        Ok(self.state.lock().map_err(|_| busy())?.status.clone())
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
    fn begin(&self, phase: &str) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| busy())?;
        if matches!(
            state.status.phase.as_str(),
            "checking" | "downloading" | "installing"
        ) {
            return Err(busy());
        }
        state.status.phase = phase.into();
        state.status.error = None;
        self.cancelled.store(false, Ordering::SeqCst);
        Ok(())
    }
    fn failed<T>(&self, error: AppError) -> Result<T> {
        if let Ok(mut state) = self.state.lock() {
            state.status.phase = "error".into();
            state.status.error = Some(if cfg!(debug_assertions) {
                error.to_string()
            } else {
                error.message.into()
            });
        }
        Err(error)
    }
    fn client() -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .https_only(test_source().is_none())
            .user_agent(concat!("Vault/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| network_error())
    }
    pub fn check(&self) -> Result<UpdateStatus> {
        self.begin("checking")?;
        let result = (|| {
            let response = Self::client()?
                .get(test_source().as_deref().unwrap_or(ENDPOINT))
                .send()
                .map_err(|_| network_error())?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let response = response.error_for_status().map_err(|_| network_error())?;
            let mut bytes = Vec::new();
            response
                .take(128 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| network_error())?;
            if bytes.len() > 128 * 1024 {
                return Err(invalid_update());
            }
            let envelope: Envelope =
                serde_json::from_slice(&bytes).map_err(|_| invalid_update())?;
            // Authenticate before comparing versions, including the up-to-date case.
            let manifest = decode(&envelope, PUBLIC_KEY, "0.0.0")?;
            if semver::Version::parse(&manifest.version).map_err(|_| invalid_update())?
                <= semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
            {
                return Ok(None);
            }
            Ok(Some((envelope, manifest)))
        })();
        match result {
            Ok(found) => {
                let mut state = self.state.lock().map_err(|_| busy())?;
                match found {
                    Some((envelope, manifest)) => {
                        let same_ready = state.manifest.as_ref().is_some_and(|m| {
                            m.version == manifest.version && m.sha256 == manifest.sha256
                        }) && state.ready.is_some();
                        state.status = UpdateStatus {
                            phase: if same_ready { "ready" } else { "available" }.into(),
                            version: Some(manifest.version.clone()),
                            notes: manifest.notes.clone(),
                            total: manifest.archive_size,
                            ..Default::default()
                        };
                        state.manifest = Some(manifest);
                        state.envelope = Some(envelope);
                        if !same_ready {
                            state.ready = None;
                        }
                    }
                    None => {
                        state.status = UpdateStatus {
                            phase: "up-to-date".into(),
                            ..Default::default()
                        };
                        state.manifest = None;
                        state.envelope = None;
                        state.ready = None;
                    }
                };
                Ok(state.status.clone())
            }
            Err(error) => self.failed(error),
        }
    }
    pub fn download(&self) -> Result<UpdateStatus> {
        self.begin("downloading")?;
        let result = (|| {
            let runtime = self.runtime()?;
            let envelope = self
                .state
                .lock()
                .map_err(|_| busy())?
                .envelope
                .clone()
                .ok_or_else(invalid_update)?;
            let manifest = decode(&envelope, PUBLIC_KEY, env!("CARGO_PKG_VERSION"))?;
            fs::create_dir_all(&runtime.cache).map_err(|_| AppError::storage())?;
            let partial = runtime.cache.join(format!("{}.part", uuid::Uuid::new_v4()));
            let unpacked = runtime.cache.join(format!("{}.exe", uuid::Uuid::new_v4()));
            let result = (|| {
                let download_size = manifest.archive_size;
                let asset = format!("/vault-{}.zip", manifest.version);
                let url = test_source()
                    .map(|url| url.replace("/latest.json", &asset))
                    .unwrap_or_else(|| manifest.url.clone());
                let mut response = Self::client()?
                    .get(url)
                    .send()
                    .map_err(|_| network_error())?
                    .error_for_status()
                    .map_err(|_| network_error())?;
                if response
                    .content_length()
                    .is_some_and(|size| size != download_size)
                {
                    return Err(invalid_update());
                }
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&partial)
                    .map_err(|_| AppError::storage())?;
                let mut received = 0;
                self.state.lock().map_err(|_| busy())?.status.received = 0;
                let mut buffer = [0; 65536];
                loop {
                    if self.cancelled.load(Ordering::SeqCst) {
                        return Err(failure("UPDATE_CANCELLED", "已取消下载，可以重新下载。"));
                    }
                    let count = response.read(&mut buffer).map_err(|_| network_error())?;
                    if count == 0 {
                        break;
                    }
                    received += count as u64;
                    if received > download_size {
                        return Err(invalid_update());
                    }
                    file.write_all(&buffer[..count])
                        .map_err(|_| AppError::storage())?;
                    self.state.lock().map_err(|_| busy())?.status.received = received;
                }
                file.sync_all().map_err(|error| {
                    AppError::storage().with("update.flush", &error.to_string())
                })?;
                drop(file);
                verify_file(&partial, download_size, &manifest.archive_sha256, b"PK")?;
                extract_update_payload(
                    &partial,
                    &unpacked,
                    &format!("vault-{}/vault.exe", manifest.version),
                    manifest.size,
                )?;
                verify_binary(&unpacked, &manifest)?;
                fs::remove_file(&partial).map_err(|_| AppError::storage())?;
                let ready = unpacked.clone();
                let mut state = self.state.lock().map_err(|_| busy())?;
                if let Some(old) = state.ready.replace(ready) {
                    let _ = fs::remove_file(old);
                }
                state.status.phase = "ready".into();
                Ok(state.status.clone())
            })();
            if result.is_err() {
                let _ = fs::remove_file(partial);
                let _ = fs::remove_file(unpacked);
            }
            result
        })();
        match result {
            Ok(status) => Ok(status),
            Err(error) => self.failed(error),
        }
    }
    pub fn pause_connections(&self) -> Result<Box<dyn PauseLease>> {
        if self.state.lock().map_err(|_| busy())?.status.phase != "ready" {
            return Err(failure("UPDATE_NOT_READY", "请先完成更新包下载。"));
        }
        self.platform.begin_pause(&self.runtime()?.executable)
    }
    pub fn prepare_install(
        &self,
        directory: &Path,
        pause: Box<dyn PauseLease>,
    ) -> Result<UpdateLaunch> {
        let runtime = self.runtime()?;
        let (envelope, ready) = {
            let state = self.state.lock().map_err(|_| busy())?;
            if state.status.phase != "ready" {
                return Err(failure("UPDATE_NOT_READY", "请先完成更新包下载。"));
            }
            (
                state.envelope.clone().ok_or_else(invalid_update)?,
                state.ready.clone().ok_or_else(invalid_update)?,
            )
        };
        let manifest = decode(&envelope, PUBLIC_KEY, env!("CARGO_PKG_VERSION"))?;
        verify_binary(&ready, &manifest)?;
        runtime.claim()?;
        let result = (|| {
            let stage = runtime
                .executable
                .parent()
                .ok_or_else(invalid_update)?
                .join(format!(".vault-update-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&stage).map_err(|_| {
                failure(
                    "UPDATE_DIRECTORY",
                    "程序目录不可写，请将便携版移到可写目录后重试。",
                )
            })?;
            let helper = stage.join("helper.exe");
            fs::copy(&runtime.executable, &helper).map_err(|_| AppError::storage())?;
            fs::copy(ready, stage.join("payload.exe")).map_err(|_| AppError::storage())?;
            let plan = InstallPlan {
                executable: runtime.executable.clone(),
                directory: directory.to_owned(),
                parent: std::process::id(),
                envelope,
                token: uuid::Uuid::new_v4().to_string(),
                pause_token: pause.token().to_owned(),
            };
            fs::write(
                stage.join("plan.json"),
                serde_json::to_vec(&plan).map_err(|_| invalid_update())?,
            )
            .map_err(|_| AppError::storage())?;
            Ok(UpdateLaunch {
                helper,
                plan: stage.join("plan.json"),
            })
        })();
        if result.is_err() {
            runtime.release_claim();
        } else {
            *runtime.pause.lock().map_err(|_| busy())? = Some(pause.token().to_owned());
            pause.handoff();
        }
        result
    }
    pub fn launched(&self) {
        self.installing.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.lock() {
            state.status.phase = "installing".into();
        }
    }
    pub fn launch_failed(&self) {
        if let Ok(runtime) = self.runtime() {
            runtime.release_claim();
            if let Ok(mut pending) = runtime.pause.lock() {
                if let Some(token) = pending.take() {
                    let _ = self.platform.adopt_pause(&runtime.executable, &token);
                }
            }
        }
    }
}

use crate::common::files::move_file;
/// Runs from a copy of the old executable, before vault/WebView initialization.
pub(crate) fn apply_update(plan_path: &Path, platform: &dyn SystemPlatform) -> Result<()> {
    let current = std::env::current_exe().map_err(|_| invalid_update())?;
    let stage = current.parent().ok_or_else(invalid_update)?;
    if current.file_name().and_then(|n| n.to_str()) != Some("helper.exe")
        || plan_path != stage.join("plan.json")
    {
        return Err(invalid_update());
    }
    let plan: InstallPlan =
        serde_json::from_slice(&fs::read(plan_path).map_err(|_| invalid_update())?)
            .map_err(|_| invalid_update())?;
    if !plan.executable.is_absolute()
        || !plan.directory.is_absolute()
        || plan.executable.parent() != stage.parent()
        || !stage
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(".vault-update-"))
        || uuid::Uuid::parse_str(&plan.token).is_err()
    {
        return Err(invalid_update());
    }
    let _resume = platform.adopt_pause(&plan.executable, &plan.pause_token)?;
    let manifest = decode(&plan.envelope, PUBLIC_KEY, env!("CARGO_PKG_VERSION"))?;
    let payload = stage.join("payload.exe");
    verify_binary(&payload, &manifest)?;
    platform.wait_for_process(plan.parent)?;
    let (lock_path, _) = runtime_paths(&plan.executable)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| AppError::storage())?;
    FileExt::try_lock_exclusive(&lock)
        .map_err(|_| failure("UPDATE_CLIENTS_ACTIVE", "仍有进程使用程序，更新未执行。"))?;
    let backup = stage.join("previous.exe");
    // The old image can remain locked briefly after process exit.
    let mut moved = false;
    for _ in 0..50 {
        if move_file(&plan.executable, &backup).is_ok() {
            moved = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !moved {
        return Err(failure("UPDATE_REPLACE", "无法替换程序，旧版本已保留。"));
    }
    if move_file(&payload, &plan.executable).is_err() {
        let _ = move_file(&backup, &plan.executable);
        return Err(invalid_update());
    }
    FileExt::unlock(&lock).map_err(|_| AppError::storage())?;
    let health = stage.join("healthy");
    let mut child = match platform.start_updated(
        &plan.executable,
        &plan.directory,
        Some((&health, &plan.token)),
    ) {
        Ok(child) => Some(child),
        Err(_) => None,
    };
    let mut healthy = false;
    if let Some(process) = child.as_mut() {
        for _ in 0..600 {
            if fs::read_to_string(&health).is_ok_and(|s| s == plan.token) {
                healthy = true;
                break;
            }
            if process.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if healthy {
        // Keep previous.exe as an executable-only recovery copy. Never touch data.
        let _ = fs::remove_file(plan_path);
        let _ = fs::remove_file(health);
        return Ok(());
    }
    if let Some(mut process) = child {
        let _ = process.kill();
        let _ = process.wait();
    }
    FileExt::try_lock_exclusive(&lock).map_err(|_| {
        failure(
            "UPDATE_ROLLBACK",
            "新版未确认启动，仍有进程占用；请从更新暂存目录恢复 previous.exe。",
        )
    })?;
    if let Err(error) = move_file(&plan.executable, &stage.join("failed.exe")) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(invalid_update());
        }
    }
    move_file(&backup, &plan.executable).map_err(|_| invalid_update())?;
    FileExt::unlock(&lock).map_err(|_| AppError::storage())?;
    health::restart_restored(platform, &plan.executable, &plan.directory)?;
    Err(failure(
        "UPDATE_ROLLED_BACK",
        "新版未能正常启动，已恢复旧程序。数据目录保持原样。",
    ))
}

#[cfg(test)]
#[path = "../../../../tests/backend/updates.rs"]
mod tests;
