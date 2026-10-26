//! Unified operation interface. Adapters hold a bound session, never raw services.
//! Only bootstrap selects caller identity; neither Operation nor caller is decoded
//! from a client-provided identity field. Policy runs before every business action.
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};
use vault_contracts::credentials::EntrySummary;
use vault_contracts::errors::{AppError, Result};
use vault_contracts::operations::{ClipboardPort, Operation};
use vault_credentials as storage;
use vault_filling as filling;
use vault_permissions as permissions;
use vault_permissions::output::{OutputGuard, SharedGuard};
#[cfg(test)]
mod tests;

use vault_contracts::ports::system::SystemPlatform;
use vault_updater as updater;

pub use updater::UpdateRuntime;

pub(crate) struct CoreService {
    pub(crate) updater: updater::UpdateService,
    pub(crate) storage: storage::CredentialService,
    pub(crate) filling: filling::FillingService,
    pub(crate) windows: Arc<dyn SystemPlatform>,
    pub(crate) permissions: permissions::PermissionService,
    pub(crate) desktop: RwLock<vault_contracts::desktop::DesktopMetadata>,
}
pub struct CoreSession {
    execution_gate: Arc<RwLock<()>>,
    pub(crate) service: Arc<CoreService>,
    pub(crate) caller: permissions::Caller,
    pub(crate) guard: SharedGuard,
}
/// Bootstrap-only binding. Client payloads never reach this function.
pub(crate) fn assemble(
    repository: Arc<dyn vault_contracts::ports::repository::VaultRepository>,
    platform: Arc<dyn SystemPlatform>,
    mcp_mode: bool,
) -> Result<Arc<CoreSession>> {
    Ok(Arc::new(CoreSession {
        execution_gate: Arc::new(RwLock::new(())),
        service: Arc::new(CoreService {
            updater: updater::UpdateService::new(platform.clone()),
            storage: storage::CredentialService::new(repository),
            filling: Default::default(),
            windows: platform,
            permissions: permissions::PermissionService,
            desktop: RwLock::new(Default::default()),
        }),
        caller: if mcp_mode {
            permissions::Caller::Mcp
        } else {
            permissions::Caller::Desktop
        },
        guard: Arc::new(RwLock::new(OutputGuard::default())),
    }))
}
use vault_common::json::encoded;
impl CoreSession {
    /// Process composition binds shell facts; wire payloads cannot set them.
    pub fn bind_desktop_metadata(
        &self,
        metadata: vault_contracts::desktop::DesktopMetadata,
    ) -> Result<()> {
        self.execute(Operation::Settings)?;
        *self
            .service
            .desktop
            .write()
            .map_err(|_| AppError::storage())? = metadata;
        Ok(())
    }
    pub fn restart_requested(&self) -> bool {
        self.service.updater.installing()
    }
    pub(crate) fn application_info(&self) -> Result<Value> {
        let settings = self.execute(Operation::Settings)?;
        let metadata = self
            .service
            .desktop
            .read()
            .map_err(|_| AppError::storage())?;
        let executable = std::env::current_exe().map_err(|_| AppError::storage())?;
        Ok(json!({
            "version":env!("CARGO_PKG_VERSION"),
            "backendPid":std::process::id(),
            "executable":executable,
            "dataDirectory":self.directory(),
            "webviewVersion":metadata.webview_version,
            "smokeTest":metadata.smoke_test,
            "settings":settings,
            "startupEnabled":self.service.windows.startup_enabled(&executable,
            self.directory())?
        }))
    }
    pub(crate) fn empty_trash(
        &self,
        _args: vault_contracts::contracts::PurgeArgs,
    ) -> Result<Value> {
        let owner = self
            .service
            .desktop
            .read()
            .map_err(|_| AppError::storage())?
            .owner_window;
        self.execute(Operation::EmptyTrash {
            owner_window: owner,
        })
    }
    /// Bootstrap may only downgrade a desktop-bound session to MCP identity.
    /// Both identities share services, update gate, target state and output guard.
    pub fn mcp_session(&self) -> Arc<Self> {
        Arc::new(Self {
            execution_gate: self.execution_gate.clone(),
            service: self.service.clone(),
            caller: permissions::Caller::Mcp,
            guard: self.guard.clone(),
        })
    }

    /// Bootstrap only; all process modes must acquire the lease before opening storage.
    pub fn bind_update_runtime(&self, runtime: Arc<UpdateRuntime>) -> Result<()> {
        self.service.updater.bind(runtime)
    }
    /// Bootstrap capability binding, not an operation available to client payloads.
    pub fn bind_clipboard(&self, port: Arc<dyn ClipboardPort>) -> Result<()> {
        self.service.permissions.authorize_operation(
            self.caller,
            &Operation::CopyText(String::new()),
            &self.service.storage.settings()?,
            &self.service.storage,
        )?;
        self.service.windows.bind_clipboard(port)
    }
    pub fn directory(&self) -> &Path {
        self.service.storage.directory()
    }
    pub fn output_guard(&self) -> SharedGuard {
        self.guard.clone()
    }
    pub fn refresh_output_guard(&self, guard: &mut OutputGuard) -> Result<()> {
        for value in self.service.storage.protected_values()?.iter() {
            guard.add(value);
        }
        Ok(())
    }
    pub fn protect_input(&self, value: &Value) -> Result<()> {
        let mut guard = self.guard.write().map_err(|_| AppError::storage())?;
        if let Some(secret) = value.get("secret").and_then(Value::as_str) {
            guard.add(secret);
        }
        if let Some(items) = value.get("items").and_then(Value::as_array) {
            for item in items {
                if let Some(secret) = item.get("secret").and_then(Value::as_str) {
                    guard.add(secret);
                }
            }
        }
        Ok(())
    }
    /// Discovery and execution consult the same persisted permission policy.
    pub fn tool_allowed(&self, name: &str) -> Result<bool> {
        let settings = self.service.storage.settings()?;
        self.service
            .permissions
            .tool_allowed(self.caller, name, &settings, &self.service.storage)
    }
    pub(crate) fn call<T: DeserializeOwned>(&self, operation: Operation) -> Result<T> {
        serde_json::from_value(self.execute(operation)?).map_err(|_| AppError::storage())
    }
    pub(crate) fn execute(&self, operation: Operation) -> Result<Value> {
        let installing = matches!(&operation, Operation::InstallUpdate);
        // Drain MCP before taking the shared exclusive gate: a request already
        // queued on that gate must be able to finish, never deadlock the updater.
        let pause = if installing {
            self.service.permissions.authorize_operation(
                self.caller,
                &operation,
                &self.service.storage.settings()?,
                &self.service.storage,
            )?;
            Some(self.service.updater.pause_connections()?)
        } else {
            None
        };
        let _exclusive = if installing {
            Some(
                self.execution_gate
                    .write()
                    .map_err(|_| AppError::storage())?,
            )
        } else {
            None
        };
        let _shared = if !installing {
            Some(
                self.execution_gate
                    .read()
                    .map_err(|_| AppError::storage())?,
            )
        } else {
            None
        };
        if self.service.updater.installing() && !matches!(&operation, Operation::UpdateStatus) {
            return Err(AppError::new(
                "UPDATE_IN_PROGRESS",
                "程序正在退出以完成更新。",
            ));
        }
        let storage = &self.service.storage;
        self.service.permissions.authorize_operation(
            self.caller,
            &operation,
            &storage.settings()?,
            storage,
        )?;
        let is_mcp = self.caller == permissions::Caller::Mcp;
        if is_mcp {
            let mut guard = self.guard.write().map_err(|_| AppError::storage())?;
            self.refresh_output_guard(&mut guard)?;
            match &operation {
                Operation::Create(values) => {
                    for value in values {
                        guard.add(&value.secret);
                    }
                }
                Operation::Update(values) => {
                    for value in values {
                        if let Some(secret) = &value.secret {
                            guard.add(secret);
                        }
                    }
                }
                _ => {}
            }
        }
        let result = self.dispatch(operation, pause);

        if is_mcp {
            let mut guard = self.guard.write().map_err(|_| AppError::storage())?;
            self.refresh_output_guard(&mut guard)?;
            result.map(|mut value| {
                guard.scrub(&mut value);
                value
            })
        } else {
            result
        }
    }
    pub(crate) fn saved(&self, entries: Vec<EntrySummary>, is_mcp: bool) -> Result<Value> {
        if is_mcp {
            encoded(
                entries
                    .into_iter()
                    .map(|e| {
                        json!({
                            "id":e.id,
                            "revision":e.revision,
                            "status":"saved"
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            encoded(entries)
        }
    }
}

// Normalize product-owned path segments for generated configuration;
// unrelated custom directory names retain exactly the user's chosen spelling.
pub(crate) fn configuration_path(path: &Path) -> std::path::PathBuf {
    path.components()
        .map(|component| {
            let value = component.as_os_str();
            if value.to_string_lossy().eq_ignore_ascii_case("vault") {
                std::ffi::OsString::from("vault")
            } else if value.to_string_lossy().eq_ignore_ascii_case("vault.exe") {
                std::ffi::OsString::from("vault.exe")
            } else {
                value.to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
fn open(directory: &Path, mcp_mode: bool) -> Result<Arc<CoreSession>> {
    crate::bootstrap::open(directory, mcp_mode)
}
