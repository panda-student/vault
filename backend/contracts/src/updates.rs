use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub struct UpdateLaunch {
    pub helper: PathBuf,
    pub plan: PathBuf,
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
    pub archive_sha256: String,
    pub archive_size: u64,
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

#[derive(Serialize, Deserialize)]
pub struct InstallPlan {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub parent: u32,
    pub envelope: Envelope,
    pub token: String,
    pub pause_token: String,
}
