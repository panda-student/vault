//! Authorized action routing; policy checks and concurrency gates live in the session.
use crate::context::{CoreSession, configuration_path};
use vault_permissions as permissions;
mod projection;
use projection as ai;
use serde_json::{Value, json};
use vault_common::json::encoded;
use vault_contracts::ports::system::PauseLease;
use vault_contracts::{
    credentials::EntryVersion,
    errors::{AppError, Result},
    operations::Operation,
};
impl CoreSession {
    pub(super) fn dispatch(
        &self,
        operation: Operation,
        pause: Option<Box<dyn PauseLease>>,
    ) -> Result<Value> {
        let storage = &self.service.storage;
        let is_mcp = self.caller == permissions::Caller::Mcp;
        match operation {
            Operation::OpenUrl(url) => {
                self.service.windows.open_url(&url)?;
                Ok(Value::Null)
            }
            Operation::UpdateStatus => encoded(self.service.updater.status()?),
            Operation::CheckUpdate => encoded(self.service.updater.check()?),
            Operation::DownloadUpdate => encoded(self.service.updater.download()?),
            Operation::CancelUpdate => {
                self.service.updater.cancel();
                Ok(Value::Null)
            }
            Operation::InstallUpdate => {
                let launch = self
                    .service
                    .updater
                    .prepare_install(self.directory(), pause.ok_or_else(AppError::invalid)?)?;
                if let Err(error) = self.service.windows.launch_update(launch) {
                    self.service.updater.launch_failed();
                    return Err(error);
                }
                self.service.updater.launched();
                Ok(Value::Null)
            }
            Operation::McpConfig => {
                let executable = std::env::current_exe().map_err(|_| {
                    AppError::new(
                        "CONFIG_UNAVAILABLE",
                        "Unable to locate the current executable",
                    )
                })?;
                Ok(
                    json!({"mcpServers":{"vault":{"command":configuration_path(&executable),"args":["--mcp","--data-dir",configuration_path(self.directory())]}}}),
                )
            }
            Operation::Search(args) if is_mcp => {
                ai::search(storage, &self.guard, args, &storage.settings()?)
            }
            Operation::Search(args) => {
                let entries: Vec<_> = storage
                    .list(&args.query, args.kind)?
                    .into_iter()
                    .filter(|e| args.tag.as_ref().is_none_or(|t| e.tags.contains(t)))
                    .collect();
                Ok(json!({"entries":entries,"hasMore":false}))
            }
            Operation::Read(ids) => ids
                .into_iter()
                .map(|id| {
                    if is_mcp {
                        ai::metadata(storage, &self.guard, &id)
                    } else {
                        encoded(storage.summary(&id)?)
                    }
                })
                .collect::<Result<Vec<_>>>()
                .map(Value::Array),
            Operation::Create(values) => self.saved(
                storage.create_many(
                    values,
                    &permissions::ChangePolicy {
                        caller: self.caller,
                        access: permissions::Access::Create,
                    },
                )?,
                is_mcp,
            ),
            Operation::Update(values) => self.saved(
                storage.update_many(
                    values,
                    &permissions::ChangePolicy {
                        caller: self.caller,
                        access: permissions::Access::Update,
                    },
                )?,
                is_mcp,
            ),
            Operation::Delete(values) => encoded(
                storage
                    .delete_many(
                        values,
                        &permissions::ChangePolicy {
                            caller: self.caller,
                            access: permissions::Access::Delete,
                        },
                    )?
                    .into_iter()
                    .map(|id| json!({"id":id,"status":"deleted"}))
                    .collect::<Vec<_>>(),
            ),
            Operation::Pin {
                id,
                expected_revision,
                pinned,
            } => {
                let pinned = storage.pin(&id, expected_revision, pinned)?;
                if is_mcp {
                    // MCP must never receive notes or other desktop-only fields.
                    ai::metadata(storage, &self.guard, &pinned.id)
                } else {
                    encoded(pinned)
                }
            }
            Operation::Organization => encoded(storage.organization()?),
            Operation::Organize {
                kind,
                name,
                previous,
            } => {
                if is_mcp {
                    permissions::guard_organize(storage, &name, previous.as_deref())?;
                }
                storage.organize(&kind, &name, previous.as_deref())?;
                Ok(Value::Null)
            }
            Operation::ListTrash => encoded(storage.list_trash()?),
            Operation::Restore {
                id,
                expected_revision,
            } => {
                let restored = storage.restore(&id, expected_revision)?;
                if is_mcp {
                    ai::metadata(storage, &self.guard, &restored.id)
                } else {
                    encoded(restored)
                }
            }
            Operation::EmptyTrash { owner_window } => {
                let entries = storage.list_trash()?;
                if entries.is_empty()
                    || !self
                        .service
                        .windows
                        .confirm_purge(owner_window, entries.len())?
                {
                    Ok(json!(0))
                } else {
                    encoded(
                        storage.empty_trash(
                            entries
                                .into_iter()
                                .map(|e| EntryVersion {
                                    id: e.summary.id,
                                    expected_revision: e.summary.revision,
                                })
                                .collect(),
                        )?,
                    )
                }
            }
            Operation::ReadField(args) => encoded(storage.read_field(
                &args.id,
                args.expected_revision,
                &args.field,
                args.service_id.as_deref(),
            )?),
            Operation::CopyField(args) => {
                let text = storage.read_field(
                    &args.id,
                    args.expected_revision,
                    &args.field,
                    args.service_id.as_deref(),
                )?;
                self.service.windows.copy_text(text)?;
                Ok(Value::Null)
            }
            Operation::CopyText(text) => {
                self.service.windows.copy_text(text)?;
                Ok(Value::Null)
            }
            Operation::Settings => encoded(storage.settings()?),
            Operation::PermissionPreview(settings) => encoded(
                storage
                    .list("", None)?
                    .iter()
                    .map(|e| permissions::preview(&settings, e))
                    .collect::<Vec<_>>(),
            ),
            Operation::SaveSettings(settings) => {
                storage.save_settings(&settings)?;
                encoded(settings)
            }
            Operation::SetStartup {
                executable,
                enabled,
            } => encoded(self.service.windows.set_startup(
                &executable,
                self.directory(),
                enabled,
            )?),
            Operation::CaptureTarget(request) => self
                .service
                .filling
                .capture(self.service.windows.as_ref(), request),
            Operation::Write(args) => {
                self.service
                    .filling
                    .write(storage, self.service.windows.as_ref(), args)
            }
        }
    }
}
