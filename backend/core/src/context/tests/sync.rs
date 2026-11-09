//! Remote sync between two vaults through an in-memory remote.
use super::settings_mcp::FakePlatform;
use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use vault_contracts::ports::sync::{RemoteObject, RemoteState, RemoteStore, RemoteStoreFactory};
use vault_contracts::sync::ResolvedTarget;

const PASSPHRASE: &str = "correct horse battery";

/// One remote file shared by every device in a test.
#[derive(Default)]
struct Remote {
    file: Mutex<Option<(Vec<u8>, u64)>>,
    /// Writes to refuse as if another device had just written.
    refuse: AtomicUsize,
    /// Times a store was opened, i.e. a credential was resolved for the network.
    opened: AtomicUsize,
}
impl Remote {
    fn bytes(&self) -> Option<Vec<u8>> {
        self.file.lock().unwrap().as_ref().map(|(b, _)| b.clone())
    }
}
struct Store(Arc<Remote>);
impl RemoteStore for Store {
    fn read(&self) -> Result<Option<RemoteObject>> {
        Ok(self
            .0
            .file
            .lock()
            .unwrap()
            .as_ref()
            .map(|(bytes, v)| RemoteObject {
                bytes: bytes.clone(),
                version: v.to_string(),
            }))
    }
    fn write(&self, bytes: &[u8], expected: Option<&str>) -> Result<String> {
        let mut file = self.0.file.lock().unwrap();
        let current = file.as_ref().map(|(_, v)| v.to_string());
        // Writes are serialized by the file lock above.
        let refused = self.0.refuse.load(Ordering::SeqCst) > 0;
        if refused {
            self.0.refuse.fetch_sub(1, Ordering::SeqCst);
        }
        if refused || current.as_deref() != expected {
            return Err(AppError::new("SYNC_REMOTE_CHANGED", "changed"));
        }
        let version = file.as_ref().map_or(1, |(_, v)| v + 1);
        *file = Some((bytes.to_vec(), version));
        Ok(version.to_string())
    }
    fn prepare(&self, _: bool) -> Result<RemoteState> {
        Ok(RemoteState {
            exists: self.0.file.lock().unwrap().is_some(),
            created: false,
        })
    }
}
struct Stores(Arc<Remote>);
impl RemoteStoreFactory for Stores {
    fn open(&self, _: &ResolvedTarget) -> Result<Box<dyn RemoteStore>> {
        self.0.opened.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Store(self.0.clone())))
    }
}

struct Device {
    _dir: tempfile::TempDir,
    session: Arc<CoreSession>,
}
fn device(remote: &Arc<Remote>) -> Device {
    let dir = tempfile::tempdir().unwrap();
    let session = assemble(
        Arc::new(vault_storage::Vault::open(dir.path()).unwrap()),
        Arc::new(FakePlatform::default()),
        Arc::new(Stores(remote.clone())),
        false,
    )
    .unwrap();
    Device { _dir: dir, session }
}

/// Folder targets are only checked for an absolute path; the fake store
/// ignores where they point.
fn target() -> Value {
    json!({"kind": "folder", "directory": std::env::temp_dir()})
}

fn configure(session: &CoreSession, passphrase: &str) -> Result<Value> {
    session.invoke(
        "sync.configure",
        json!({"target": target(), "passphrase": passphrase, "deviceName": "PC"}),
    )
}

fn titles(session: &CoreSession) -> Vec<String> {
    let mut titles: Vec<String> = session
        .service
        .storage
        .list("", None)
        .unwrap()
        .into_iter()
        .map(|e| e.title)
        .collect();
    titles.sort();
    titles
}

fn rename(session: &CoreSession, title: &str, to: &str) {
    let entry = session
        .service
        .storage
        .list("", None)
        .unwrap()
        .into_iter()
        .find(|e| e.title == title)
        .unwrap();
    let patch = serde_json::from_value(
        json!({"id": entry.id, "expectedRevision": entry.revision, "title": to}),
    )
    .unwrap();
    session.execute(Operation::Update(vec![patch])).unwrap();
}

#[test]
fn edits_deletes_and_new_entries_reach_the_other_device() {
    let remote = Arc::new(Remote::default());
    let (a, b) = (device(&remote), device(&remote));
    create(&a.session, "alpha");
    let status = configure(&a.session, PASSPHRASE).unwrap();
    assert_eq!(status["configured"], true);
    assert_eq!(status["lastOutcome"]["pushed"], 1);

    // An empty vault joining restores the remote without asking.
    let status = configure(&b.session, PASSPHRASE).unwrap();
    assert_eq!(status["firstJoin"], false);
    assert_eq!(titles(&b.session), ["alpha"]);

    rename(&b.session, "alpha", "alpha-2");
    create(&b.session, "beta");
    b.session.invoke("sync.run", json!({})).unwrap();
    a.session.invoke("sync.run", json!({})).unwrap();
    assert_eq!(titles(&a.session), ["alpha-2", "beta"]);

    // Moving to the trash travels too.
    let beta = a
        .session
        .service
        .storage
        .list("beta", None)
        .unwrap()
        .remove(0);
    a.session
        .execute(Operation::Delete(vec![EntryVersion {
            id: beta.id,
            expected_revision: beta.revision,
        }]))
        .unwrap();
    a.session.invoke("sync.run", json!({})).unwrap();
    b.session.invoke("sync.run", json!({})).unwrap();
    assert_eq!(titles(&b.session), ["alpha-2"]);
    assert_eq!(b.session.service.storage.list_trash().unwrap().len(), 1);
}

#[test]
fn wrong_passphrase_restores_nothing_and_saves_nothing() {
    let remote = Arc::new(Remote::default());
    let (a, b) = (device(&remote), device(&remote));
    create(&a.session, "alpha");
    configure(&a.session, PASSPHRASE).unwrap();
    let before = remote.bytes();

    let error = configure(&b.session, "wrong").unwrap_err();
    assert_eq!(error.code, "SYNC_PASSPHRASE");
    assert!(titles(&b.session).is_empty());
    assert_eq!(
        b.session.invoke("sync.status", json!({})).unwrap()["configured"],
        false
    );
    assert_eq!(remote.bytes(), before);
}

#[test]
fn first_join_with_data_on_both_sides_waits_for_the_users_choice() {
    let remote = Arc::new(Remote::default());
    let (a, b) = (device(&remote), device(&remote));
    create(&a.session, "alpha");
    configure(&a.session, PASSPHRASE).unwrap();
    create(&b.session, "beta");
    let before = remote.bytes();

    let status = configure(&b.session, PASSPHRASE).unwrap();
    assert_eq!(status["firstJoin"], true);
    assert_eq!(status["conflicts"].as_array().unwrap().len(), 2);
    assert_eq!(titles(&b.session), ["beta"]);
    assert_eq!(remote.bytes(), before);

    let status = b
        .session
        .invoke("sync.resolve", json!({"choice": "twoWay"}))
        .unwrap();
    assert_eq!(status["firstJoin"], false);
    assert_eq!(titles(&b.session), ["alpha", "beta"]);
    a.session.invoke("sync.run", json!({})).unwrap();
    assert_eq!(titles(&a.session), ["alpha", "beta"]);
}

#[test]
fn same_entry_changed_on_both_sides_is_held_until_resolved() {
    let remote = Arc::new(Remote::default());
    let (a, b) = (device(&remote), device(&remote));
    create(&a.session, "alpha");
    create(&a.session, "other");
    configure(&a.session, PASSPHRASE).unwrap();
    configure(&b.session, PASSPHRASE).unwrap();

    rename(&a.session, "alpha", "from-a");
    rename(&a.session, "other", "other-a");
    a.session.invoke("sync.run", json!({})).unwrap();
    rename(&b.session, "alpha", "from-b");
    let status = b.session.invoke("sync.run", json!({})).unwrap();
    // The untouched-elsewhere change still came through; the conflict waits.
    assert_eq!(status["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(status["lastOutcome"]["conflicts"], 1);
    assert_eq!(titles(&b.session), ["from-b", "other-a"]);

    b.session
        .invoke("sync.resolve", json!({"choice": "localWins"}))
        .unwrap();
    a.session.invoke("sync.run", json!({})).unwrap();
    assert_eq!(titles(&a.session), ["from-b", "other-a"]);
    let status = b.session.invoke("sync.status", json!({})).unwrap();
    assert!(status["conflicts"].as_array().unwrap().is_empty());
}

#[test]
fn a_busy_remote_is_retried_then_reported() {
    let remote = Arc::new(Remote::default());
    let a = device(&remote);
    create(&a.session, "alpha");
    configure(&a.session, PASSPHRASE).unwrap();

    create(&a.session, "beta");
    remote.refuse.store(3, Ordering::SeqCst);
    a.session.invoke("sync.run", json!({})).unwrap();

    create(&a.session, "gamma");
    remote.refuse.store(4, Ordering::SeqCst);
    let error = a.session.invoke("sync.run", json!({})).unwrap_err();
    assert_eq!(error.code, "SYNC_REMOTE_CHANGED");
    let status = a.session.invoke("sync.status", json!({})).unwrap();
    assert_eq!(status["error"]["code"], "SYNC_REMOTE_CHANGED");
    // Nothing was lost: the next run pushes it.
    a.session.invoke("sync.run", json!({})).unwrap();
    let b = device(&remote);
    configure(&b.session, PASSPHRASE).unwrap();
    assert_eq!(titles(&b.session), ["alpha", "beta", "gamma"]);
}

#[test]
fn referenced_credential_must_exist() {
    let remote = Arc::new(Remote::default());
    let a = device(&remote);
    let error = a
        .session
        .invoke(
            "sync.configure",
            json!({
                "target": {
                    "kind": "webdav",
                    "url": "https://dav.example.com",
                    "credential": {"credentialId": "missing"}
                },
                "passphrase": PASSPHRASE,
            }),
        )
        .unwrap_err();
    assert_eq!(error.code, "SYNC_CREDENTIAL_MISSING");
}

fn allow_settings(session: &CoreSession) {
    let mut s = settings(session);
    s.mcp_permissions.settings = true;
    session.execute(Operation::SaveSettings(s)).unwrap();
}

fn go_online(session: &CoreSession) {
    session
        .bind_desktop_metadata(vault_contracts::desktop::DesktopMetadata {
            owner_window: 1,
            ..Default::default()
        })
        .unwrap();
}

fn wait_for_request(desktop: &CoreSession) -> Value {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let value = desktop.invoke("sync.pending", json!({})).unwrap();
        if !value["pending"].is_null() {
            return value["pending"].clone();
        }
        assert!(Instant::now() < until, "desktop never saw the request");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn mcp_target_change_waits_for_the_desktop_passphrase() {
    let remote = Arc::new(Remote::default());
    let (a, b) = (device(&remote), device(&remote));
    create(&a.session, "alpha");
    configure(&a.session, PASSPHRASE).unwrap();
    allow_settings(&b.session);
    let mcp = b.session.mcp_session();
    let patch = json!({"sync": {"target": target(), "intervalMinutes": 5}});

    assert_eq!(
        mcp.invoke("settings.mcpUpdate", patch.clone())
            .unwrap_err()
            .code,
        "DESKTOP_OFFLINE"
    );
    go_online(&b.session);
    assert_eq!(
        mcp.invoke(
            "settings.mcpUpdate",
            json!({"sync": {"target": target()}, "closeToTray": true})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    // The user refuses: nothing is saved.
    let opened = remote.opened.load(Ordering::SeqCst);
    let waiting = {
        let mcp = mcp.clone();
        let patch = patch.clone();
        std::thread::spawn(move || mcp.invoke("settings.mcpUpdate", patch))
    };
    let request = wait_for_request(&b.session);
    assert_eq!(request["kind"], "folder");
    // Nothing reached the remote before the user answered.
    assert_eq!(remote.opened.load(Ordering::SeqCst), opened);
    b.session
        .invoke("sync.respond", json!({"id": request["id"], "cancel": true}))
        .unwrap();
    assert_eq!(waiting.join().unwrap().unwrap_err().code, "SYNC_CANCELLED");
    assert_eq!(
        b.session.invoke("sync.status", json!({})).unwrap()["configured"],
        false
    );

    // A mistyped passphrase leaves the request open for another try.
    let waiting = {
        let mcp = mcp.clone();
        std::thread::spawn(move || mcp.invoke("settings.mcpUpdate", patch))
    };
    let request = wait_for_request(&b.session);
    let error = b
        .session
        .invoke(
            "sync.respond",
            json!({"id": request["id"], "passphrase": "wrong"}),
        )
        .unwrap_err();
    assert_eq!(error.code, "SYNC_PASSPHRASE");
    assert!(titles(&b.session).is_empty());
    let request = wait_for_request(&b.session);
    b.session
        .invoke(
            "sync.respond",
            json!({"id": request["id"], "passphrase": PASSPHRASE}),
        )
        .unwrap();
    let view = waiting.join().unwrap().unwrap();
    assert_eq!(view["sync"]["configured"], true);
    assert_eq!(view["sync"]["intervalMinutes"], 5);
    assert_eq!(titles(&b.session), ["alpha"]);
    assert!(!view.to_string().contains(PASSPHRASE));

    // Interval and device name change without asking; null stops syncing.
    let view = mcp
        .invoke(
            "settings.mcpUpdate",
            json!({"sync": {"intervalMinutes": 0, "deviceName": "Laptop"}}),
        )
        .unwrap();
    assert_eq!(view["sync"]["intervalMinutes"], 0);
    assert_eq!(view["sync"]["deviceName"], "Laptop");
    assert!(view["sync"].get("target").is_none());
    let status = mcp.invoke("sync.mcpRun", json!({})).unwrap();
    assert!(!status.to_string().contains(PASSPHRASE));
    // Stopping must not be mixed with other changes.
    assert_eq!(
        mcp.invoke(
            "settings.mcpUpdate",
            json!({"sync": {"enabled": false, "intervalMinutes": 5}})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let view = mcp
        .invoke("settings.mcpUpdate", json!({"sync": {"enabled": false}}))
        .unwrap();
    assert_eq!(view["sync"]["configured"], false);
    assert_eq!(titles(&b.session), ["alpha"]);
    // `null` stops it too, for clients that send it.
    b.session
        .invoke(
            "sync.configure",
            json!({"target": target(), "passphrase": PASSPHRASE}),
        )
        .unwrap();
    let view = mcp
        .invoke("settings.mcpUpdate", json!({"sync": null}))
        .unwrap();
    assert_eq!(view["sync"]["configured"], false);
}

#[test]
fn desktop_sync_methods_are_closed_to_mcp() {
    let remote = Arc::new(Remote::default());
    let a = device(&remote);
    allow_settings(&a.session);
    let mcp = a.session.mcp_session();
    for (method, params) in [
        ("sync.status", json!({})),
        ("sync.credentials", json!({})),
        (
            "sync.configure",
            json!({"target": target(), "passphrase": "x"}),
        ),
        ("sync.test", json!({"target": target()})),
        ("sync.run", json!({})),
        ("sync.resolve", json!({"choice": "localWins"})),
        ("sync.disable", json!({})),
        ("sync.pending", json!({})),
        ("sync.respond", json!({"id": "x", "cancel": true})),
    ] {
        assert_eq!(
            mcp.invoke(method, params).unwrap_err().code,
            "PERMISSION_DENIED",
            "{method}"
        );
    }
    assert!(mcp.tool_allowed("sync_now").unwrap());
    assert_eq!(
        mcp.invoke("sync.mcpRun", json!({})).unwrap_err().code,
        "SYNC_NOT_CONFIGURED"
    );
}
