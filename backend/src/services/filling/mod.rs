//! Local filling service. It receives storage access internally, never from adapters.
use super::credentials::CredentialService;
use crate::definitions::{errors::Result, filling::CaptureRequest};
mod write;
use crate::definitions::write::WriteArgs;
#[derive(Default)]
pub(crate) struct FillingService;
impl FillingService {
    pub fn capture(
        &self,
        windows: &dyn crate::definitions::ports::system::SystemPlatform,
        request: CaptureRequest,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::json!(windows.capture_target(request)?))
    }
    pub fn write(
        &self,
        storage: &CredentialService,
        windows: &dyn crate::definitions::ports::system::SystemPlatform,
        args: WriteArgs,
    ) -> Result<serde_json::Value> {
        write::execute(storage, windows, args)
    }
}
