//! Internal caller identity and capability categories. Neither is deserialized from a request.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Caller {
    Desktop,
    Mcp,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Access {
    Public,
    Read,
    Create,
    Update,
    Delete,
    Input,
    File,
    DesktopOnly,
}
