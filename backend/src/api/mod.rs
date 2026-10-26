//! Public backend contract. Implementation modules remain private.
pub(crate) mod rpc;
pub use crate::definitions::desktop::DesktopMetadata;
pub use crate::definitions::organization::Organization;
pub use crate::definitions::organization::TrashEntry;
pub use crate::definitions::{
    credentials::*,
    errors::{AppError, Result},
    filling::CaptureRequest,
    operations::{ClipboardPort, FieldRequest, Search},
    write::WriteArgs,
};
pub use crate::services::CoreSession;

mod session;
