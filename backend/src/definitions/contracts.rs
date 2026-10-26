//! Named parameters shared by every JSON-RPC transport.
use super::credentials::Settings;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Batch<T> {
    pub(crate) items: Vec<T>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ById {
    pub(crate) id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyArgs {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizeArgs {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) previous: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PinArgs {
    pub(crate) id: String,
    pub(crate) expected_revision: i64,
    pub(crate) pinned: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SecretArgs {
    pub id: String,
    pub expected_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TextArgs {
    pub text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UrlArgs {
    pub url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsArgs {
    pub settings: Settings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartupArgs {
    pub enabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PurgeArgs {}
