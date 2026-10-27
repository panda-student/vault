//! Transport-neutral operation contract. Caller identity is never deserialized.
use super::errors::Result;
use super::{
    contracts::{
        ExportArgs, McpExportArgs, TransferImportArgs, TransferPathArgs, TransferRespondArgs,
    },
    credentials::{EntryInput, EntryPatch, EntryVersion, Kind, Settings},
    filling::CaptureRequest,
    write::WriteArgs,
};
use serde::Deserialize;
use std::path::PathBuf;

/// Desktop supplies the OS clipboard adapter; validation and permission remain core-owned.
pub trait ClipboardPort: Send + Sync {
    fn write_text(&self, text: String) -> Result<()>;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
    #[serde(default)]
    pub query: String,
    pub kind: Option<Kind>,
    pub tag: Option<String>,
}
impl Search {
    pub fn new(query: String, kind: Option<Kind>) -> Self {
        Self {
            query,
            kind,
            tag: None,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldRequest {
    pub id: String,
    pub expected_revision: i64,
    pub field: String,
    pub service_id: Option<String>,
}
pub enum Operation {
    UpdateStatus,
    CheckUpdate,
    DownloadUpdate,
    CancelUpdate,
    InstallUpdate,
    McpConfig,
    Search(Search),
    Read(Vec<String>),
    Create(Vec<EntryInput>),
    Update(Vec<EntryPatch>),
    Delete(Vec<EntryVersion>),
    Pin {
        id: String,
        expected_revision: i64,
        pinned: bool,
    },
    Organization,
    Organize {
        kind: String,
        name: String,
        previous: Option<String>,
    },
    ListTrash,
    Restore {
        id: String,
        expected_revision: i64,
    },
    /// Window handle is supplied by the trusted desktop adapter, never a request boolean.
    EmptyTrash {
        owner_window: isize,
    },
    ReadField(FieldRequest),
    CopyField(FieldRequest),
    CopyText(String),
    OpenUrl(String),
    Settings,
    SaveSettings(Settings),
    PermissionPreview(Settings),
    SetStartup {
        executable: PathBuf,
        enabled: bool,
    },
    CaptureTarget(CaptureRequest),
    Write(WriteArgs),
    /// Desktop-only file export; MCP can never choose a path or receive content.
    Export(ExportArgs),
    InspectExport(TransferPathArgs),
    /// Desktop-only all-or-nothing import.
    Import(TransferImportArgs),
    TransferPending,
    TransferRespond(TransferRespondArgs),
    /// MCP asks the on-screen user for a passphrase; the caller receives status only.
    McpExport(McpExportArgs),
}
