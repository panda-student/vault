use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub struct UpdateLaunch {
    pub installer: PathBuf,
    pub directory: PathBuf,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub platform: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub notes: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub phase: String,
    pub version: Option<String>,
    pub notes: String,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
}
