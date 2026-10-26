use super::credentials::EntrySummary;
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
pub struct Organization {
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    #[serde(flatten)]
    pub summary: EntrySummary,
    pub deleted_at: i64,
}
