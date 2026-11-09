//! MCP view of application settings. Permission fields are readable but never writable here.
use super::credentials::{McpPermissions, PermissionRule};
use super::sync::{SyncPatch, SyncStatusView};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// Fields an MCP caller may change; everything else in the view is read-only.
pub const WRITABLE: &[&str] = &["closeToTray", "startupEnabled", "sync"];
/// Fields shown in the view that only the desktop may change.
pub const READ_ONLY: [&str; 7] = [
    "mcpEnabled",
    "mcpAccess",
    "mcpPermissions",
    "mcpTagRules",
    "mcpEntryRules",
    "version",
    "dataDirectory",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettingsView {
    pub close_to_tray: bool,
    pub startup_enabled: bool,
    pub mcp_enabled: bool,
    pub mcp_access: bool,
    pub mcp_permissions: McpPermissions,
    pub mcp_tag_rules: BTreeMap<String, PermissionRule>,
    pub mcp_entry_rules: BTreeMap<String, PermissionRule>,
    pub version: String,
    pub data_directory: PathBuf,
    /// Remote sync status; the passphrase and tokens are never part of it.
    pub sync: SyncStatusView,
    pub writable: &'static [&'static str],
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpSettingsPatch {
    pub close_to_tray: Option<bool>,
    pub startup_enabled: Option<bool>,
    /// Absent: unchanged. `null`: stop syncing (no data is removed anywhere).
    #[serde(default, deserialize_with = "present")]
    pub sync: Option<Option<SyncPatch>>,
}
impl McpSettingsPatch {
    pub fn is_empty(&self) -> bool {
        self.close_to_tray.is_none() && self.startup_enabled.is_none() && self.sync.is_none()
    }
}

/// Tell an explicit `null` apart from an absent field.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}
