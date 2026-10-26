use tauri_plugin_clipboard_manager::ClipboardExt;
use vault_core::api::{AppError, ClipboardPort, Result};
pub(super) struct DesktopClipboard(pub tauri::AppHandle);
impl ClipboardPort for DesktopClipboard {
    fn write_text(&self, text: String) -> Result<()> {
        self.0
            .clipboard()
            .write_text(text)
            .map_err(|_| AppError::new("CLIPBOARD_ERROR", "无法写入系统剪贴板，请重试。"))
    }
}
