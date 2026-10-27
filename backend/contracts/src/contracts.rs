//! Named parameters shared by every JSON-RPC transport.
use super::credentials::{Kind, Settings};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch<T> {
    pub items: Vec<T>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ById {
    pub id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyArgs {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizeArgs {
    pub kind: String,
    pub name: String,
    pub previous: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinArgs {
    pub id: String,
    pub expected_revision: i64,
    pub pinned: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretArgs {
    pub id: String,
    pub expected_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextArgs {
    pub text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UrlArgs {
    pub url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsArgs {
    pub settings: Settings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupArgs {
    pub enabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeArgs {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferFormat {
    Json,
    Csv,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportArgs {
    pub format: TransferFormat,
    pub include_trash: bool,
    pub kinds: Vec<Kind>,
    pub encrypted: bool,
    pub passphrase: Option<String>,
    pub path: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferImportArgs {
    pub path: String,
    pub passphrase: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRespondArgs {
    pub id: String,
    pub passphrase: Option<String>,
    #[serde(default)]
    pub cancel: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferPathArgs {
    pub path: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpExportArgs {
    #[serde(default)]
    pub include_trash: bool,
}
