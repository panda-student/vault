use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteArgs {
    pub credential_id: String,
    pub service_id: Option<String>,
    /// Required when the entry has more than one credential group.
    pub group_id: Option<String>,
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
    pub fn is_command(&self) -> bool {
        matches!(self.target, Destination::Command { .. })
    }
    pub fn is_auto(&self) -> bool {
        matches!(self.target, Destination::Auto(_))
    }
}
/// One target that vault fills by whichever way works: a browser page over its
/// debugging port, a console program waiting for a password, a named desktop
/// field set through accessibility, then keyboard input into that field.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoTarget {
    pub port: Option<u16>,
    pub selector: Option<String>,
    pub target_id: Option<String>,
    pub window_title: Option<String>,
    pub field_name: Option<String>,
    pub process_id: Option<u32>,
    pub expected_password: Option<bool>,
    /// Press Enter after a console password; nothing else is submitted.
    #[serde(default)]
    pub submit: bool,
}
/// Fields whose value is a secret. Vault itself injects them into an input, a
/// browser page or a command it runs; they never go to a file another program reads.
pub const SECRET_FIELDS: [&str; 3] = ["password", "token", "secret"];
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
    Input {
        #[serde(rename = "targetId")]
        target_id: String,
    },
    /// A page of a local browser started with a remote debugging port. Vault
    /// connects to that port itself and types the value into the element
    /// `selector` names.
    Browser {
        port: u16,
        selector: String,
        #[serde(rename = "targetId")]
        target_id: Option<String>,
    },
    /// A program vault starts itself. Stored values go where placeholders such
    /// as `{{password}}` stand in `command`, `env` values and `stdin`; vault
    /// waits for the program and returns its output with those values masked.
    Command {
        command: Vec<String>,
        cwd: Option<PathBuf>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        stdin: Option<String>,
        #[serde(rename = "timeoutSeconds")]
        timeout_seconds: Option<u64>,
    },
    File {
        path: PathBuf,
        #[serde(default)]
        mode: FileMode,
        template: Option<String>,
        placeholder: Option<String>,
    },
    Auto(AutoTarget),
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMode {
    #[default]
    Create,
    Replace,
}
