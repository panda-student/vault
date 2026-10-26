//! Public backend contract. Implementation modules remain private.
pub(crate) mod rpc;
pub use crate::context::CoreSession;
pub use vault_contracts::desktop::DesktopMetadata;
pub use vault_contracts::organization::Organization;
pub use vault_contracts::organization::TrashEntry;
pub use vault_contracts::{
    credentials::*,
    errors::{AppError, Result},
    filling::CaptureRequest,
    operations::{ClipboardPort, FieldRequest, Search},
    write::WriteArgs,
};

mod session;

pub use vault_permissions::output::{OutputGuard, SharedGuard};
