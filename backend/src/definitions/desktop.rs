//! Desktop runtime facts supplied by the trusted shell during composition.
#[derive(Default)]
pub struct DesktopMetadata {
    pub webview_version: String,
    pub smoke_test: bool,
    pub owner_window: isize,
}
