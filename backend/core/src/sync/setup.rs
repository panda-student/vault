//! Choosing a sync target. The passphrase is typed only on the desktop; an MCP
//! caller asks for a new target and waits for the user's confirmation there.
//!
//! Nothing reaches the network for an MCP request until the user has seen the
//! location and answered: the referenced credential is only read afterwards.
use super::{CONFIG, STATE, STATUS, SyncConfig, default_device, folder_directory, not_configured};
use crate::{
    approval::{Approval, Messages},
    context::CoreSession,
    transfer::{MCP_WAIT, desktop_online},
};
use serde_json::{Value, json};
use vault_common::json::encoded;
use vault_contracts::{
    errors::{AppError, Result},
    ports::sync::RemoteStore,
    sync::{
        DEFAULT_INTERVAL, MAX_INTERVAL, SyncPatch, SyncRequestView, SyncRespondArgs, SyncSetup,
        SyncStatusView, SyncTarget,
    },
};
use vault_transfer::sync::{self as container, SyncKey};

const MAX_DEVICE_NAME: usize = 64;

/// A target to connect to, from the desktop form or a pending MCP request.
#[derive(Clone)]
pub(crate) struct Connect {
    target: SyncTarget,
    create: bool,
    interval_minutes: Option<u32>,
    device_name: Option<String>,
}

pub(crate) enum Answer {
    Connected,
    Cancelled,
    Failed(&'static str, &'static str),
}

pub(crate) type Requests = Approval<Connect, Answer>;

fn messages() -> Messages {
    Messages {
        busy: AppError::new(
            "SYNC_REQUEST_PENDING",
            "已有一个同步设置正在等待桌面确认，请稍后重试。",
        ),
        ended: || AppError::new("SYNC_CANCELLED", "同步设置请求已结束。"),
        timeout: AppError::new("SYNC_TIMEOUT", "等待桌面端确认同步设置超时，未做任何改动。"),
    }
}

impl CoreSession {
    pub(crate) fn sync_configure(&self, setup: SyncSetup) -> Result<SyncStatusView> {
        let SyncSetup {
            target,
            passphrase,
            interval_minutes,
            device_name,
            create,
        } = setup;
        let connect = Connect {
            target,
            create,
            interval_minutes,
            device_name,
        };
        self.connect(&connect, passphrase)
    }

    /// Check that the target is reachable and writable without changing anything.
    pub(crate) fn sync_test(&self, setup: SyncSetup) -> Result<Value> {
        let store = self.open_store(&setup.target)?;
        store.prepare(false)?;
        Ok(json!({"remoteExists": probe(store.as_ref())?}))
    }

    pub(crate) fn sync_disable(&self) -> Result<SyncStatusView> {
        let run = self.service.sync.lock_run()?;
        for name in [CONFIG, STATE, STATUS] {
            self.save::<()>(name, None)?;
        }
        self.service.sync.set_key(None)?;
        drop(run);
        self.sync_status()
    }

    /// Entries whose groups can supply a WebDAV password or a GitHub token.
    /// Metadata only; the desktop shows these for the user to pick from.
    pub(crate) fn sync_credentials(&self) -> Result<Value> {
        let entries: Vec<Value> = self
            .service
            .storage
            .list("", None)?
            .into_iter()
            .filter(|e| e.groups.iter().any(|g| g.has_password || g.has_token))
            .map(|e| {
                json!({
                    "id": e.id,
                    "name": e.name,
                    "kind": e.kind,
                    "groups": e.groups,
                })
            })
            .collect();
        Ok(Value::Array(entries))
    }

    /// The pending MCP request as the desktop shows it. The credential name
    /// is looked up here, for the desktop only, never for the MCP caller.
    pub(crate) fn sync_pending(&self) -> Result<Value> {
        let pending = self
            .service
            .sync
            .requests
            .pending()?
            .map(|(id, c, at)| SyncRequestView {
                id,
                kind: c.target.kind(),
                location: c.target.summary(),
                credential_name: c
                    .target
                    .credential()
                    .and_then(|r| self.credential_name(&r.credential_id)),
                create: c.create,
                requested_at: at,
            });
        Ok(json!({ "pending": encoded(pending)? }))
    }

    /// The desktop answers a pending MCP request. A mistyped passphrase leaves
    /// the request open so the user can try again within the same wait.
    pub(crate) fn sync_respond(&self, args: SyncRespondArgs) -> Result<Value> {
        let requests = &self.service.sync.requests;
        let Some(connect) = requests.claim(&args.id)? else {
            return Err(AppError::new(
                "SYNC_REQUEST_MISSING",
                "该同步设置请求已失效，请让 AI 重新发起。",
            ));
        };
        if args.cancel {
            requests.finish(&args.id, Answer::Cancelled);
            return Ok(json!({"status": "cancelled"}));
        }
        let result = self.connect(&connect, args.passphrase);
        match &result {
            Ok(_) => requests.finish(&args.id, Answer::Connected),
            Err(error) if passphrase_error(error) => requests.release(&args.id),
            Err(error) => requests.finish(&args.id, Answer::Failed(error.code, error.message)),
        }
        encoded(result?)
    }

    /// MCP change of sync settings. `None` stops syncing; a new target needs
    /// the desktop user; interval and device name apply directly.
    pub(crate) fn mcp_sync_patch(&self, patch: Option<SyncPatch>) -> Result<()> {
        let Some(patch) = patch else {
            return self.sync_disable().map(|_| ());
        };
        let others = patch.target.is_some()
            || patch.create
            || patch.interval_minutes.is_some()
            || patch.device_name.is_some();
        match patch.enabled {
            Some(false) if !others => return self.sync_disable().map(|_| ()),
            Some(false) => {
                return Err(
                    AppError::invalid().with("enabled", "停用同步时请只传 enabled: false。")
                );
            }
            Some(true) if patch.target.is_none() => {
                return Err(AppError::invalid()
                    .with("enabled", "启用同步请传 target；enabled 只用于停用。"));
            }
            _ => {}
        }
        let interval = patch.interval_minutes.map(checked_interval).transpose()?;
        let device = patch
            .device_name
            .as_deref()
            .map(checked_device)
            .transpose()?;
        let current = self.sync_config()?;
        match patch.target {
            Some(target) if current.as_ref().map(|c| &c.target) != Some(&target) => self
                .request_target(Connect {
                    target,
                    create: patch.create,
                    interval_minutes: interval,
                    device_name: device,
                }),
            _ => self.update_schedule(interval, device),
        }
    }

    /// Interval and device name take effect without asking.
    fn update_schedule(&self, interval: Option<u32>, device: Option<String>) -> Result<()> {
        let _run = self.service.sync.lock_run()?;
        let mut config = self.sync_config()?.ok_or_else(not_configured)?;
        if let Some(interval) = interval {
            config.interval_minutes = interval;
        }
        if let Some(device) = device {
            config.device_name = device;
        }
        self.save(CONFIG, Some(&config))
    }

    /// Ask the desktop user, who sees the location and types the passphrase.
    /// Only the target's shape is checked here: no credential is read and no
    /// request leaves this computer before the user approves.
    fn request_target(&self, connect: Connect) -> Result<()> {
        if !desktop_online(self)? {
            return Err(AppError::new(
                "DESKTOP_OFFLINE",
                "设置同步位置需要用户在桌面确认并输入同步口令，请先打开桌面界面。",
            ));
        }
        if let SyncTarget::Folder { directory, .. } = &connect.target {
            folder_directory(directory, self.directory())?;
        }
        let requests = &self.service.sync.requests;
        let id = requests.begin(connect, messages().busy)?;
        match requests.wait(&id, MCP_WAIT, messages())? {
            Answer::Connected => Ok(()),
            Answer::Cancelled => Err(AppError::new(
                "SYNC_CANCELLED",
                "用户在桌面拒绝了该同步设置，未做任何改动。",
            )),
            Answer::Failed(code, message) => Err(AppError::new(code, message)),
        }
    }

    /// Save a target after its passphrase is proven: an existing sync file
    /// must open with it before anything is stored or merged into the vault.
    fn connect(&self, connect: &Connect, passphrase: Option<String>) -> Result<SyncStatusView> {
        let interval = connect.interval_minutes.map(checked_interval).transpose()?;
        let device = connect
            .device_name
            .as_deref()
            .map(checked_device)
            .transpose()?;
        let run = self.service.sync.lock_run()?;
        let current = self.sync_config()?;
        let same_target = current.as_ref().is_some_and(|c| c.target == connect.target);
        let passphrase = match (passphrase.filter(|p| !p.is_empty()), &current) {
            (Some(passphrase), _) => passphrase,
            (None, Some(current)) if same_target => current.passphrase.clone(),
            _ => return Err(AppError::invalid().with("passphrase", "请输入同步口令。")),
        };
        let key = self.verify(&connect.target, &passphrase, connect.create)?;
        let config = SyncConfig {
            target: connect.target.clone(),
            passphrase,
            interval_minutes: interval
                .or(current.as_ref().map(|c| c.interval_minutes))
                .unwrap_or(DEFAULT_INTERVAL),
            device_name: device
                .or(current.as_ref().map(|c| c.device_name.clone()))
                .unwrap_or_else(default_device),
        };
        if !same_target {
            // The base and status belonged to the previous target.
            self.save::<()>(STATE, None)?;
            self.save::<()>(STATUS, None)?;
        }
        self.save(CONFIG, Some(&config))?;
        self.service.sync.set_key(Some(key))?;
        drop(run);
        // Saved either way; a failed first run shows in the status.
        let _ = self.sync_run(None);
        self.sync_status()
    }

    fn verify(&self, target: &SyncTarget, passphrase: &str, create: bool) -> Result<SyncKey> {
        let store = self.open_store(target)?;
        store.prepare(create)?;
        match store.read()? {
            Some(object) => {
                let key = SyncKey::for_file(passphrase, &object.bytes)?;
                container::open(&key, &object.bytes)?;
                Ok(key)
            }
            None => SyncKey::create(passphrase),
        }
    }
}

/// Whether our sync file exists. Anything else at that path is refused.
fn probe(store: &dyn RemoteStore) -> Result<bool> {
    match store.read()? {
        Some(object) if container::is_sync_file(&object.bytes) => Ok(true),
        Some(_) => Err(AppError::new(
            "SYNC_FOREIGN_FILE",
            "同步路径上已有其他文件，不是本程序的同步文件，未做任何改动。请换一个路径。",
        )),
        None => Ok(false),
    }
}

/// Errors the user fixes by typing the passphrase again.
fn passphrase_error(error: &AppError) -> bool {
    error.code == "SYNC_PASSPHRASE"
        || error
            .details
            .iter()
            .flatten()
            .any(|issue| issue.field == "passphrase")
}

fn checked_interval(minutes: u32) -> Result<u32> {
    if minutes > MAX_INTERVAL {
        return Err(AppError::invalid().with(
            "intervalMinutes",
            "同步间隔为 0（仅手动和修改后）到 1440 分钟。",
        ));
    }
    Ok(minutes)
}

fn checked_device(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_DEVICE_NAME {
        return Err(AppError::invalid().with("deviceName", "设备名称为 1-64 个字符。"));
    }
    Ok(name.into())
}
