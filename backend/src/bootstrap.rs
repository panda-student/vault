//! Trusted process composition, separate from client operations.
pub use crate::platform::runtime;
pub use crate::platform::windows::pipes;
pub use crate::services::UpdateRuntime;

/// Assemble the backend with its encrypted repository. No framework is required.
pub fn open(
    directory: &std::path::Path,
    mcp_mode: bool,
) -> crate::api::Result<std::sync::Arc<crate::api::CoreSession>> {
    crate::services::assemble(
        std::sync::Arc::new(crate::data::Vault::open(directory)?),
        std::sync::Arc::new(crate::platform::windows::WindowsService::default()),
        mcp_mode,
    )
}

/// Installer and updater bootstrap use injected Windows capabilities without opening a vault.
pub fn replace_package(
    target: &std::path::Path,
    report: &std::path::Path,
) -> crate::api::Result<()> {
    crate::services::updates::installation::replace_package(
        target,
        report,
        &crate::platform::windows::WindowsService::default(),
    )
}
pub fn apply_update(plan: &std::path::Path) -> crate::api::Result<()> {
    crate::services::updates::apply_update(
        plan,
        &crate::platform::windows::WindowsService::default(),
    )
}
