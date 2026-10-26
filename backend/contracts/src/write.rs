use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteArgs {
    pub credential_id: String,
    pub service_id: Option<String>,
    pub field: String,
    pub target: Destination,
}
impl WriteArgs {
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }
    pub fn is_file(&self) -> bool {
        matches!(self.target, Destination::File { .. })
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
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
pub enum FileMode {
    #[default]
    Create,
    Replace,
}
