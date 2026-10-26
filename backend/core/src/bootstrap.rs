//! Trusted process composition, separate from client operations.
pub use vault_platform::runtime;
pub use vault_platform::windows::pipes;
pub use vault_updater::UpdateRuntime;

/// Assemble the backend with its encrypted repository. No framework is required.
pub fn open(
    directory: &std::path::Path,
    mcp_mode: bool,
) -> crate::api::Result<std::sync::Arc<crate::api::CoreSession>> {
    crate::context::assemble(
        std::sync::Arc::new(vault_storage::Vault::open(directory)?),
        std::sync::Arc::new(vault_platform::windows::WindowsService::default()),
        mcp_mode,
    )
}

/// Installer and updater bootstrap use injected Windows capabilities without opening a vault.
pub fn replace_package(
    target: &std::path::Path,
    report: &std::path::Path,
) -> crate::api::Result<()> {
    vault_updater::installation::replace_package(
        target,
        report,
        &vault_platform::windows::WindowsService::default(),
    )
}
pub fn apply_update(plan: &std::path::Path) -> crate::api::Result<()> {
    vault_updater::apply_update(plan, &vault_platform::windows::WindowsService::default())
}
