//! MCP settings: read everything, change only non-permission settings.
use super::*;
use std::process::Child;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use vault_contracts::filling::FillStatus;
use vault_contracts::ports::system::{DesktopProcess, PauseLease};
use vault_contracts::updates::UpdateLaunch;

/// Startup lives in memory so tests never touch the user's registry.
#[derive(Default)]
pub(super) struct FakePlatform {
    startup: Mutex<bool>,
    fail_startup: AtomicBool,
}
fn unsupported<T>() -> Result<T> {
    Err(AppError::new("UNSUPPORTED", "测试平台不支持该操作。"))
}
impl SystemPlatform for FakePlatform {
    fn capture_target(&self, _: CaptureRequest) -> Result<FillStatus> {
        unsupported()
    }
    fn fill_target(
        &self,
        _: &str,
        _: Box<dyn FnOnce() -> Result<String> + '_>,
    ) -> Result<FillStatus> {
        unsupported()
    }
    fn replace_file(&self, _: &Path, _: &Path) -> Result<()> {
        unsupported()
    }
    fn bind_clipboard(&self, _: Arc<dyn ClipboardPort>) -> Result<()> {
        unsupported()
    }
    fn copy_text(&self, _: String) -> Result<()> {
        unsupported()
    }
    fn open_url(&self, _: &str) -> Result<()> {
        unsupported()
    }
    fn startup_enabled(&self, _: &Path, _: &Path) -> Result<bool> {
        Ok(*self.startup.lock().unwrap())
    }
    fn set_startup(&self, _: &Path, _: &Path, enabled: bool) -> Result<bool> {
        if self.fail_startup.load(Ordering::SeqCst) {
            return Err(AppError::new("STARTUP_FAILED", "无法写入开机启动项。"));
        }
        *self.startup.lock().unwrap() = enabled;
        Ok(enabled)
    }
    fn confirm_purge(&self, _: isize, _: usize) -> Result<bool> {
        unsupported()
    }
    fn launch_update(&self, _: UpdateLaunch) -> Result<()> {
        unsupported()
    }
    fn begin_pause(&self, _: &Path) -> Result<Box<dyn PauseLease>> {
        unsupported()
    }
    fn wait_for_process(&self, _: u32) -> Result<()> {
        unsupported()
    }
    fn start_updated(&self, _: &Path, _: &Path, _: Option<(&Path, &str)>) -> Result<Child> {
        unsupported()
    }
    fn desktops(&self, _: &Path) -> Result<Vec<Box<dyn DesktopProcess>>> {
        unsupported()
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    platform: Arc<FakePlatform>,
    desktop: Arc<CoreSession>,
    mcp: Arc<CoreSession>,
}
fn fixture(allow_settings: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(FakePlatform::default());
    let desktop = assemble(
        Arc::new(vault_storage::Vault::open(dir.path()).unwrap()),
        platform.clone(),
        Arc::new(vault_sync::Stores),
        false,
    )
    .unwrap();
    let mcp = desktop.mcp_session();
    let mut s = settings(&desktop);
    s.mcp_permissions.settings = allow_settings;
    s.mcp_tag_rules.insert(
        "work".into(),
        PermissionRule {
            delete: Some(false),
            ..Default::default()
        },
    );
    desktop.execute(Operation::SaveSettings(s)).unwrap();
    Fixture {
        _dir: dir,
        platform,
        desktop,
        mcp,
    }
}

#[test]
fn settings_tools_are_off_by_default_and_hidden_from_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let mcp = open(dir.path(), true).unwrap();
    for name in ["get_settings", "update_settings", "sync_now"] {
        assert!(!mcp.tool_allowed(name).unwrap(), "{name}");
    }
    assert_eq!(
        mcp.invoke("settings.mcpGet", json!({})).unwrap_err().code,
        "PERMISSION_DENIED"
    );
    assert_eq!(
        mcp.invoke("settings.mcpUpdate", json!({"closeToTray": true}))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
}

#[test]
fn settings_permission_ignores_tag_and_entry_rules() {
    let f = fixture(false);
    let entry = create(&f.desktop, "tagged");
    let mut s = settings(&f.desktop);
    let grant = PermissionRule {
        access: Some(true),
        ..Default::default()
    };
    s.mcp_tag_rules.insert("work".into(), grant.clone());
    s.mcp_entry_rules.insert(entry.id, grant);
    f.desktop.execute(Operation::SaveSettings(s)).unwrap();
    assert!(!f.mcp.tool_allowed("get_settings").unwrap());

    // The global access switch still applies on top of the settings grant.
    let mut s = settings(&f.desktop);
    s.mcp_permissions.settings = true;
    s.mcp_access = false;
    f.desktop.execute(Operation::SaveSettings(s)).unwrap();
    assert!(!f.mcp.tool_allowed("get_settings").unwrap());
}

#[test]
fn mcp_reads_every_setting_including_permission_rules() {
    let f = fixture(true);
    assert!(f.mcp.tool_allowed("get_settings").unwrap());
    assert!(f.mcp.tool_allowed("update_settings").unwrap());
    let view = f.mcp.invoke("settings.mcpGet", json!({})).unwrap();
    assert_eq!(view["closeToTray"], false);
    assert_eq!(view["startupEnabled"], false);
    assert_eq!(view["mcpPermissions"]["settings"], true);
    assert_eq!(view["mcpTagRules"]["work"]["delete"], false);
    assert_eq!(
        view["writable"],
        json!(["closeToTray", "startupEnabled", "sync"])
    );
    assert_eq!(view["sync"]["configured"], false);
    assert_eq!(view["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn mcp_changes_tray_and_startup() {
    let f = fixture(true);
    let view = f
        .mcp
        .invoke(
            "settings.mcpUpdate",
            json!({"closeToTray": true, "startupEnabled": true}),
        )
        .unwrap();
    assert_eq!(view["closeToTray"], true);
    assert_eq!(view["startupEnabled"], true);
    assert!(settings(&f.desktop).close_to_tray);
    assert!(*f.platform.startup.lock().unwrap());
    // Permission rules survive a tray change.
    assert_eq!(
        settings(&f.desktop).mcp_tag_rules["work"].delete,
        Some(false)
    );
}

#[test]
fn mcp_cannot_change_permission_rules_or_send_empty_patches() {
    let f = fixture(true);
    let before = serde_json::to_value(settings(&f.desktop)).unwrap();
    for patch in [
        json!({"closeToTray": true, "mcpPermissions": {"delete": true}}),
        json!({"mcpEnabled": false}),
        json!({"mcpAccess": true}),
        json!({"mcpTagRules": {}}),
        json!({"mcpEntryRules": {}}),
    ] {
        let error = f.mcp.invoke("settings.mcpUpdate", patch).unwrap_err();
        assert_eq!(error.code, "SETTINGS_READ_ONLY");
        assert!(error.details.is_some());
    }
    assert_eq!(
        f.mcp
            .invoke("settings.mcpUpdate", json!({"unknown": true}))
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        f.mcp
            .invoke("settings.mcpUpdate", json!({}))
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    assert_eq!(serde_json::to_value(settings(&f.desktop)).unwrap(), before);
    for (method, params) in [
        ("settings.save", json!({"settings": before})),
        ("startup.set", json!({"enabled": true})),
    ] {
        assert_eq!(
            f.mcp.invoke(method, params).unwrap_err().code,
            "PERMISSION_DENIED",
            "{method}"
        );
    }
}

#[test]
fn failed_startup_change_rolls_back_the_tray_change() {
    let f = fixture(true);
    f.platform.fail_startup.store(true, Ordering::SeqCst);
    let error = f
        .mcp
        .invoke(
            "settings.mcpUpdate",
            json!({"closeToTray": true, "startupEnabled": true}),
        )
        .unwrap_err();
    assert_eq!(error.code, "STARTUP_FAILED");
    assert!(!settings(&f.desktop).close_to_tray);
    assert!(!*f.platform.startup.lock().unwrap());
}

#[test]
fn settings_saved_before_the_permission_existed_still_load() {
    let legacy = json!({
        "mcpEnabled": true,
        "closeToTray": false,
        "mcpPermissions": {
            "readMetadata": true, "create": true, "update": true,
            "delete": true, "fillInput": true, "writeFile": true
        },
        "mcpAccess": true,
        "mcpTagRules": {},
        "mcpEntryRules": {}
    });
    let parsed: Settings = serde_json::from_value(legacy).unwrap();
    assert!(!parsed.mcp_permissions.settings);
}
