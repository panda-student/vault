//! MCP read of all settings and update of non-permission settings.
//!
//! Permission rules are returned for visibility but can only be changed on the
//! desktop, so an MCP caller can never grant itself more access.
use crate::context::{CoreSession, current_executable};
use serde_json::Value;
use vault_common::json::encoded;
use vault_contracts::{
    errors::{AppError, Result},
    settings::{McpSettingsPatch, McpSettingsView, READ_ONLY, WRITABLE},
};

impl CoreSession {
    pub(crate) fn mcp_settings(&self) -> Result<Value> {
        encoded(self.settings_view()?)
    }

    /// Validate first, then apply in order, so the request takes effect fully
    /// or not at all. Sync may wait on the network or the desktop user, so it
    /// is changed on its own and never mixed with other settings.
    pub(crate) fn mcp_update_settings(&self, patch: McpSettingsPatch) -> Result<Value> {
        if patch.is_empty() {
            return Err(AppError::invalid().with("settings", "请至少提供一个要修改的设置。"));
        }
        match patch.sync {
            Some(_) if patch.close_to_tray.is_some() || patch.startup_enabled.is_some() => {
                Err(AppError::invalid()
                    .with("sync", "请单独修改同步设置，不要与其他设置一起提交。"))
            }
            Some(sync) => self.mcp_sync_patch(sync),
            // This operation runs outside the execution gate; take it here.
            None => self.gated(|| self.apply_local_settings(&patch)),
        }?;
        encoded(self.settings_view()?)
    }

    /// A failed startup change rolls the tray change back.
    fn apply_local_settings(&self, patch: &McpSettingsPatch) -> Result<()> {
        let storage = &self.service.storage;
        // Only a value that actually changed needs restoring on failure.
        let previous_tray = match patch.close_to_tray {
            Some(value) => Some(storage.set_close_to_tray(value)?).filter(|old| *old != value),
            None => None,
        };
        if let Some(enabled) = patch.startup_enabled {
            let changed = current_executable().and_then(|executable| {
                self.service
                    .windows
                    .set_startup(&executable, self.directory(), enabled)
            });
            if let Err(error) = changed {
                // Keep the startup error as the cause; a failed restore is reported
                // alongside it instead of replacing it.
                let restored = previous_tray
                    .is_none_or(|previous| storage.set_close_to_tray(previous).is_ok());
                return Err(if restored {
                    error
                } else {
                    error.with(
                        "closeToTray",
                        "开机启动修改失败，且关闭到托盘未能恢复原值，请在桌面检查。",
                    )
                });
            }
        }
        Ok(())
    }

    fn settings_view(&self) -> Result<McpSettingsView> {
        let settings = self.service.storage.settings()?;
        Ok(McpSettingsView {
            close_to_tray: settings.close_to_tray,
            startup_enabled: self.startup_enabled()?,
            mcp_enabled: settings.mcp_enabled,
            mcp_access: settings.mcp_access,
            mcp_permissions: settings.mcp_permissions,
            mcp_tag_rules: settings.mcp_tag_rules,
            mcp_entry_rules: settings.mcp_entry_rules,
            version: env!("CARGO_PKG_VERSION").into(),
            data_directory: self.directory().to_path_buf(),
            sync: self.sync_status_for_mcp()?,
            writable: WRITABLE,
        })
    }
}

/// Parse an MCP patch, naming read-only fields explicitly instead of a generic
/// unknown-field error so the AI knows to send the user to the desktop.
pub(crate) fn parse_patch(value: Value) -> Result<McpSettingsPatch> {
    if let Some(object) = value.as_object()
        && let Some(field) = object.keys().find(|key| READ_ONLY.contains(&key.as_str()))
    {
        return Err(
            AppError::new("SETTINGS_READ_ONLY", "该设置只能在桌面修改。")
                .with(field, "权限规则和应用信息只能在桌面修改。"),
        );
    }
    serde_json::from_value(value)
        .map_err(|error| AppError::invalid().with("settings", &error.to_string()))
}
