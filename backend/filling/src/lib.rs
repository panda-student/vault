//! Local filling service. It receives storage access internally, never from adapters.
use vault_contracts::ports::repository::CredentialReader;
use vault_contracts::{errors::Result, filling::CaptureRequest};
#[cfg(test)]
mod tests;
mod write;
use vault_contracts::write::WriteArgs;
#[derive(Default)]
pub struct FillingService;
impl FillingService {
    pub fn capture(
        &self,
        windows: &dyn vault_contracts::ports::system::SystemPlatform,
        request: CaptureRequest,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::json!(windows.capture_target(request)?))
    }
    pub fn write(
        &self,
        storage: &dyn CredentialReader,
        windows: &dyn vault_contracts::ports::system::SystemPlatform,
        args: WriteArgs,
    ) -> Result<serde_json::Value> {
        write::execute(storage, windows, args)
    }
}
