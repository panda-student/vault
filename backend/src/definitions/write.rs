use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteArgs {
    pub(crate) credential_id: String,
    pub(crate) service_id: Option<String>,
    pub(crate) field: String,
    pub(crate) target: Destination,
}
impl WriteArgs {
    pub(crate) fn credential_id(&self) -> &str {
        &self.credential_id
    }
    pub(crate) fn is_file(&self) -> bool {
        matches!(self.target, Destination::File { .. })
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Destination {
    Input {
        #[serde(rename = "targetId")]
        target_id: String,
    },
    File {
        path: PathBuf,
        #[serde(default)]
        mode: FileMode,
        template: Option<String>,
        placeholder: Option<String>,
    },
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FileMode {
    #[default]
    Create,
    Replace,
}
