//! Named parameters shared by every JSON-RPC transport.
use super::credentials::Settings;
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
