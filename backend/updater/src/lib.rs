//! Signed installer updates. Network metadata never supplies local paths or commands.
mod health;
pub mod installation;
mod package;
mod runtime;
mod verification;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use fs2::FileExt;
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
use vault_contracts::errors::{AppError, Result};
use vault_contracts::updates::UpdateLaunch;

const ENDPOINT: &str =
    "https://github.com/panda-student/vault/releases/latest/download/latest.json";
const MAX_PACKAGE: u64 = 64 * 1024 * 1024;
const PUBLIC_KEY: &str = include_str!("../update-public-key.txt");
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

use package::verify_binary;
pub use runtime::UpdateRuntime;
use vault_contracts::updates::{Envelope, Manifest, UpdateStatus};
use verification::{decode, signed_version};
#[derive(Default)]
struct State {
    status: UpdateStatus,
    envelope: Option<Envelope>,
    manifest: Option<Manifest>,
    ready: Option<PathBuf>,
}
pub struct UpdateService {
    state: Mutex<State>,
    runtime: Mutex<Option<Arc<UpdateRuntime>>>,
    cancelled: AtomicBool,
    installing: AtomicBool,
}
impl UpdateService {
    pub fn new() -> Self {
        Self {
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
            // An older release line may still publish another manifest schema.
            if signed_version(&envelope, PUBLIC_KEY)?
                <= semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
            {
                return Ok(None);
            }
            let manifest = decode(&envelope, PUBLIC_KEY, "0.0.0")?;
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
                            total: manifest.size,
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
            let result = (|| {
                let download_size = manifest.size;
                let asset = format!("/vault-{}-setup.exe", manifest.version);
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
                verify_binary(&partial, &manifest)?;
                let ready = runtime
                    .cache
                    .join(format!("vault-{}-setup.exe", manifest.version));
                fs::rename(&partial, &ready).map_err(|_| AppError::storage())?;
                let mut state = self.state.lock().map_err(|_| busy())?;
                if let Some(old) = state.ready.replace(ready.clone()) {
                    if old != ready {
                        let _ = fs::remove_file(old);
                    }
                }
                state.status.phase = "ready".into();
                Ok(state.status.clone())
            })();
            if result.is_err() {
                let _ = fs::remove_file(partial);
            }
            result
        })();
        match result {
            Ok(status) => Ok(status),
            Err(error) => self.failed(error),
        }
    }
    /// Validate the verified installer and hand it to the desktop, which closes
    /// itself. The installer then takes the shared-session lock and drains MCP.
    pub fn prepare_install(&self) -> Result<UpdateLaunch> {
        let runtime = self.runtime()?;
        let (envelope, installer) = {
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
        verify_binary(&installer, &manifest)?;
        // Refuse early when other windows or old MCP sessions hold the program.
        runtime.claim()?;
        runtime.release_claim();
        let directory = runtime
            .executable
            .parent()
            .ok_or_else(invalid_update)?
            .to_owned();
        Ok(UpdateLaunch {
            installer,
            directory,
        })
    }
    pub fn launched(&self) {
        self.installing.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.lock() {
            state.status.phase = "installing".into();
        }
    }
}

#[cfg(test)]
mod tests;
