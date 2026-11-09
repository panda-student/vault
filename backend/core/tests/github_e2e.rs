//! Manual end-to-end check against a real private GitHub repository.
//! Calls the same desktop and MCP methods the app uses. Never prints the token.
//! Run: E2E_TOKEN_FILE=… E2E_OWNER=… E2E_REPO=… E2E_PATH=<new file path in the repo>
//!      E2E_DIR=… cargo test -p vault-core
//!      --test github_e2e -- --ignored --nocapture
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use vault_core::api::{CoreSession, DesktopMetadata};

const PASSPHRASE: &str = "e2e sync passphrase 7f3a";

struct Env {
    token: String,
    owner: String,
    repo: String,
    path: String,
    dir: PathBuf,
}
fn env() -> Env {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let token = std::fs::read_to_string(var("E2E_TOKEN_FILE"))
        .unwrap()
        .trim()
        .to_string();
    assert!(!token.is_empty());
    Env {
        token,
        owner: var("E2E_OWNER"),
        repo: var("E2E_REPO"),
        path: var("E2E_PATH"),
        dir: PathBuf::from(var("E2E_DIR")),
    }
}

/// Print a step, failing if the token or passphrase would appear in any output.
fn step(env: &Env, name: &str, value: &Value) {
    let text = value.to_string();
    assert!(!text.contains(&env.token), "{name}: token leaked");
    println!("STEP {name}: {text}");
}

fn device(env: &Env, name: &str) -> Arc<CoreSession> {
    let dir = env.dir.join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let session = vault_core::bootstrap::open(&dir, false).unwrap();
    // A desktop window is open: MCP requests can be answered.
    session
        .bind_desktop_metadata(DesktopMetadata {
            owner_window: 1,
            ..Default::default()
        })
        .unwrap();
    session
}

fn create(session: &CoreSession, value: Value) -> String {
    session.invoke("credentials.create", value).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}
fn login(session: &CoreSession, title: &str) -> String {
    create(
        session,
        json!({"kind": "password", "title": title, "url": "https://example.com",
               "secret": format!("pw-{title}")}),
    )
}
fn token_entry(session: &CoreSession, env: &Env) -> String {
    create(
        session,
        json!({"kind": "api", "title": "GitHub sync token",
               "services": [{"protocol": "custom", "url": "https://api.github.com"}],
               "groups": [{"token": env.token}]}),
    )
}
fn titles(session: &CoreSession) -> Vec<String> {
    let mut titles: Vec<String> = session
        .invoke("credentials.list", json!({}))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap().to_string())
        .collect();
    titles.sort();
    titles
}
fn entry(session: &CoreSession, title: &str) -> Value {
    session
        .invoke("credentials.list", json!({}))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == title)
        .unwrap_or_else(|| panic!("no entry {title}"))
        .clone()
}
fn rename(session: &CoreSession, title: &str, to: &str) {
    let e = entry(session, title);
    session
        .invoke(
            "credentials.update",
            json!({"id": e["id"], "expectedRevision": e["revision"], "title": to}),
        )
        .unwrap();
}
fn target(env: &Env, credential: &str) -> Value {
    json!({"kind": "github", "owner": env.owner, "repo": env.repo, "path": env.path,
           "token": {"credentialId": credential}})
}

#[test]
#[ignore = "needs a real GitHub token and network"]
fn github_sync_end_to_end() {
    let env = env();
    // Random and never printed: the remote file holds the real token, so no
    // one, including whoever reads this source, can open it afterwards.
    let passphrase = uuid::Uuid::new_v4().to_string();

    // A: two logins and the token entry; the AI may change settings.
    let a = device(&env, "a");
    login(&a, "alpha");
    login(&a, "beta");
    let a_token = token_entry(&a, &env);
    let mut settings = a.invoke("settings.get", json!({})).unwrap();
    settings["mcpPermissions"]["settings"] = json!(true);
    a.invoke("settings.save", json!({"settings": settings}))
        .unwrap();
    let mcp = a.mcp_session();

    // 1. The AI asks for a new private repository. Nothing happens until the
    //    desktop user sees the request and types the passphrase.
    let waiting = {
        let mcp = mcp.clone();
        let patch = json!({"sync": {"target": target(&env, &a_token), "create": true,
                                    "intervalMinutes": 0, "deviceName": "PC-A"}});
        std::thread::spawn(move || mcp.invoke("settings.mcpUpdate", patch))
    };
    let until = Instant::now() + Duration::from_secs(20);
    let request = loop {
        let pending = a.invoke("sync.pending", json!({})).unwrap()["pending"].clone();
        if !pending.is_null() {
            break pending;
        }
        assert!(Instant::now() < until, "no pending request");
        std::thread::sleep(Duration::from_millis(50));
    };
    step(&env, "desktop sees AI request", &request);
    assert_eq!(request["kind"], "github");
    assert!(request["credentialName"].is_string());
    let answer = a
        .invoke(
            "sync.respond",
            json!({"id": request["id"], "passphrase": passphrase}),
        )
        .unwrap();
    step(&env, "desktop answers with passphrase", &answer);
    let view = waiting.join().unwrap().unwrap();
    step(&env, "AI gets settings view", &view["sync"]);
    assert_eq!(view["sync"]["configured"], true);
    assert_eq!(view["sync"]["lastOutcome"]["pushed"], 3);
    assert!(view["sync"].get("target").is_none());
    assert!(!view.to_string().contains(&passphrase));

    // 2. B already has its own entry and token: a first join with data on both sides.
    let b = device(&env, "b");
    login(&b, "gamma");
    let b_token = token_entry(&b, &env);
    let setup = |passphrase: &str| {
        json!({"target": target(&env, &b_token), "passphrase": passphrase,
               "intervalMinutes": 0, "deviceName": "PC-B"})
    };
    let error = b
        .invoke("sync.configure", setup("wrong passphrase"))
        .unwrap_err();
    step(&env, "B wrong passphrase", &json!({"code": error.code}));
    assert_eq!(error.code, "SYNC_PASSPHRASE");
    assert_eq!(titles(&b), ["GitHub sync token", "gamma"]);
    assert_eq!(
        b.invoke("sync.status", json!({})).unwrap()["configured"],
        false
    );

    let status = b.invoke("sync.configure", setup(&passphrase)).unwrap();
    step(&env, "B joins: first join waits", &status);
    assert_eq!(status["firstJoin"], true);
    assert_eq!(titles(&b), ["GitHub sync token", "gamma"]);

    let status = b
        .invoke("sync.resolve", json!({"choice": "twoWay"}))
        .unwrap();
    step(&env, "B two-way", &status["lastOutcome"]);
    assert_eq!(
        titles(&b),
        [
            "GitHub sync token",
            "GitHub sync token",
            "alpha",
            "beta",
            "gamma"
        ]
    );
    a.invoke("sync.run", json!({})).unwrap();
    assert_eq!(titles(&a), titles(&b));

    // 3. Edits on both sides of different entries merge silently.
    rename(&a, "alpha", "alpha-from-A");
    a.invoke("sync.run", json!({})).unwrap();
    rename(&b, "gamma", "gamma-from-B");
    let status = b.invoke("sync.run", json!({})).unwrap();
    step(&env, "B merges A's edit", &status["lastOutcome"]);
    if let Err(error) = a.invoke("sync.run", json!({})) {
        step(
            &env,
            "A run failed",
            &json!({"code": error.code, "message": error.message, "details": error.details}),
        );
        panic!("A run failed");
    }
    assert!(titles(&a).contains(&"gamma-from-B".to_string()));
    assert!(titles(&b).contains(&"alpha-from-A".to_string()));

    // 3b. Back-to-back syncs right after the other device wrote.
    for round in 0..6 {
        let (writer, reader) = if round % 2 == 0 { (&a, &b) } else { (&b, &a) };
        login(writer, &format!("burst-{round}"));
        writer.invoke("sync.run", json!({})).unwrap();
        if let Err(error) = reader.invoke("sync.run", json!({})) {
            step(
                &env,
                "burst failed",
                &json!({"round": round, "code": error.code}),
            );
            panic!("burst round {round} failed");
        }
    }
    assert_eq!(titles(&a), titles(&b));
    step(&env, "burst rounds", &json!("6 passed"));

    // 4. The same entry changed on both: held until the user chooses.
    rename(&a, "beta", "beta-A");
    a.invoke("sync.run", json!({})).unwrap();
    rename(&b, "beta", "beta-B");
    let status = b.invoke("sync.run", json!({})).unwrap();
    step(&env, "B sees conflict", &status["conflicts"]);
    assert_eq!(status["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(status["conflicts"][0]["localDevice"], "PC-B");
    assert_eq!(status["conflicts"][0]["remoteDevice"], "PC-A");
    // The AI only sees a count.
    let ai = mcp.invoke("sync.mcpRun", json!({})).unwrap();
    assert!(ai["conflicts"].as_array().unwrap().is_empty());
    b.invoke("sync.resolve", json!({"choice": "remoteWins"}))
        .unwrap();
    assert!(titles(&b).contains(&"beta-A".to_string()));
    assert!(!titles(&b).contains(&"beta-B".to_string()));

    // 5. Moving to the trash travels; nothing is purged.
    let gone = entry(&b, "gamma-from-B");
    b.invoke(
        "credentials.delete",
        json!({"id": gone["id"], "expectedRevision": gone["revision"]}),
    )
    .unwrap();
    b.invoke("sync.run", json!({})).unwrap();
    a.invoke("sync.run", json!({})).unwrap();
    assert!(!titles(&a).contains(&"gamma-from-B".to_string()));
    let trash = a.invoke("trash.list", json!({})).unwrap();
    step(
        &env,
        "A trash after B delete",
        &json!(trash.as_array().unwrap().len()),
    );
    assert_eq!(trash.as_array().unwrap().len(), 1);

    // 6. A rotated token is read fresh; a missing one is reported.
    let status = a.invoke("sync.status", json!({})).unwrap();
    step(&env, "A final status", &status);
    let revision = a
        .invoke("credentials.list", json!({}))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == a_token.as_str())
        .unwrap()["revision"]
        .clone();
    a.invoke(
        "credentials.delete",
        json!({"id": a_token, "expectedRevision": revision}),
    )
    .unwrap();
    let error = a.invoke("sync.run", json!({})).unwrap_err();
    step(&env, "A token in trash", &json!({"code": error.code}));
    assert_eq!(error.code, "SYNC_CREDENTIAL_MISSING");

    // 7. Edits sync on their own in the background (about 30 s after an edit).
    b.start_sync_worker().unwrap();
    std::thread::sleep(Duration::from_secs(8));
    login(&b, "delta-background");
    std::thread::sleep(Duration::from_secs(40));
    let status = b.invoke("sync.status", json!({})).unwrap();
    step(&env, "B after background sync", &status["lastOutcome"]);
    let c = device(&env, "c");
    let c_token = token_entry(&c, &env);
    c.invoke(
        "sync.configure",
        json!({"target": target(&env, &c_token), "passphrase": passphrase}),
    )
    .unwrap();
    let status = c
        .invoke("sync.resolve", json!({"choice": "twoWay"}))
        .unwrap_or_else(|_| c.invoke("sync.status", json!({})).unwrap());
    step(&env, "C joined", &status["lastOutcome"]);
    assert!(titles(&c).contains(&"delta-background".to_string()));
    println!("STEP done: all checks passed");
}

/// Folder target with the real store: other files in the folder are never
/// touched, and a file of another format at the sync path is refused.
#[test]
fn folder_sync_touches_only_its_own_file() {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("drive");
    std::fs::create_dir_all(remote.join("vault")).unwrap();
    std::fs::write(remote.join("notes.txt"), b"keep me").unwrap();
    std::fs::write(remote.join("vault").join("other.bin"), b"also mine").unwrap();
    std::fs::write(remote.join("vault").join("foreign.vaultsync"), b"not ours").unwrap();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let session = vault_core::bootstrap::open(&data, false).unwrap();
    login(&session, "alpha");
    let folder = |path: &str| {
        json!({"target": {"kind": "folder", "directory": remote, "path": path},
               "passphrase": PASSPHRASE})
    };

    let error = session
        .invoke("sync.configure", folder("vault/foreign.vaultsync"))
        .unwrap_err();
    assert_eq!(error.code, "SYNC_FOREIGN_FILE");
    assert_eq!(
        std::fs::read(remote.join("vault").join("foreign.vaultsync")).unwrap(),
        b"not ours"
    );
    assert_eq!(
        session.invoke("sync.status", json!({})).unwrap()["configured"],
        false
    );

    let status = session
        .invoke("sync.configure", folder("vault/vault-sync.vaultsync"))
        .unwrap();
    assert_eq!(status["lastOutcome"]["pushed"], 1);
    let synced = std::fs::read(remote.join("vault").join("vault-sync.vaultsync")).unwrap();
    assert!(synced.starts_with(b"VAULTSYN"));
    assert_eq!(std::fs::read(remote.join("notes.txt")).unwrap(), b"keep me");
    assert_eq!(
        std::fs::read(remote.join("vault").join("other.bin")).unwrap(),
        b"also mine"
    );
    let mut names: Vec<_> = std::fs::read_dir(remote.join("vault"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["foreign.vaultsync", "other.bin", "vault-sync.vaultsync"]
    );

    // The vault's own data directory is never a sync location.
    let error = session
        .invoke(
            "sync.configure",
            json!({"target": {"kind": "folder", "directory": data}, "passphrase": PASSPHRASE}),
        )
        .unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
}
