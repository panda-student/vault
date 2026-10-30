use super::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use vault_contracts::contracts::{
    ExportArgs, McpExportArgs, TransferFormat, TransferImportArgs, TransferPathArgs,
    TransferRespondArgs,
};
use vault_contracts::operations::Operation;

fn export_args(path: &std::path::Path, encrypted: bool, passphrase: Option<&str>) -> ExportArgs {
    ExportArgs {
        format: TransferFormat::Json,
        include_trash: false,
        kinds: vec![Kind::Password, Kind::Api, Kind::Database, Kind::Generic],
        encrypted,
        passphrase: passphrase.map(str::to_string),
        path: path.to_string_lossy().into_owned(),
    }
}

fn wait_for_pending(desktop: &CoreSession) -> Value {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let value: Value = desktop.call(Operation::TransferPending).unwrap();
        if !value["pending"].is_null() {
            return value["pending"].clone();
        }
        assert!(
            Instant::now() < until,
            "desktop never saw the export request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn encrypted_export_import_roundtrip_creates_new_entries() {
    let source = tempfile::tempdir().unwrap();
    let desktop = open(source.path(), false).unwrap();
    desktop
        .call::<Vec<EntrySummary>>(Operation::Create(vec![input("导出 A"), input("导出 B")]))
        .unwrap();
    let path = source.path().join("vault-export.vaultenc");
    let exported: Value = desktop
        .call(Operation::Export(export_args(
            &path,
            true,
            Some("correct horse battery"),
        )))
        .unwrap();
    assert_eq!(exported["count"], 2);
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"VAULTEXP"));

    let target = tempfile::tempdir().unwrap();
    let fresh = open(target.path(), false).unwrap();
    let imported: Value = fresh
        .call(Operation::Import(TransferImportArgs {
            path: path.to_string_lossy().into_owned(),
            passphrase: Some("correct horse battery".into()),
        }))
        .unwrap();
    assert_eq!(imported["imported"], 2);
    let titles: Vec<String> = fresh
        .service
        .storage
        .list("", None)
        .unwrap()
        .into_iter()
        .map(|entry| entry.title)
        .collect();
    assert!(titles.contains(&"导出 A".to_string()));
    assert!(titles.contains(&"导出 B".to_string()));
}

#[test]
fn import_reports_file_row_and_rolls_back_every_entry() {
    let target = tempfile::tempdir().unwrap();
    let desktop = open(target.path(), false).unwrap();
    let path = target.path().join("broken.csv");
    std::fs::write(
        &path,
        b"name,url,username,password,note\r\nGood,https://example.invalid,alice,pw,\r\nBad,https://example.invalid,bob,,\r\n",
    )
    .unwrap();
    let error = desktop
        .call::<Value>(Operation::Import(TransferImportArgs {
            path: path.to_string_lossy().into_owned(),
            passphrase: None,
        }))
        .unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
    assert!(error.to_string().contains("第 3 行"), "{error}");
    assert!(desktop.service.storage.list("", None).unwrap().is_empty());
}

#[test]
fn imported_legacy_entry_is_flagged_and_the_next_update_requires_a_site() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    let path = dir.path().join("legacy.csv");
    std::fs::write(
        &path,
        b"name,url,username,password,note\r\nLegacy account,,alice,pw,\r\n",
    )
    .unwrap();
    let imported: Value = desktop
        .call(Operation::Import(TransferImportArgs {
            path: path.to_string_lossy().into_owned(),
            passphrase: None,
        }))
        .unwrap();
    assert_eq!(imported["imported"], 1);
    let entry = desktop.service.storage.list("", None).unwrap().remove(0);
    assert!(entry.url.is_empty());
    let read: Value = desktop
        .mcp_session()
        .execute(Operation::Read(vec![entry.id.clone()]))
        .unwrap();
    assert_eq!(read[0]["siteMissing"], true);
    let patch: vault_contracts::credentials::EntryPatch = serde_json::from_value(json!({
        "id":entry.id,"expectedRevision":entry.revision,"title":"renamed"
    }))
    .unwrap();
    let error = desktop.execute(Operation::Update(vec![patch])).unwrap_err();
    assert_eq!(error.details.unwrap()[0].field, "url");
    let patch: vault_contracts::credentials::EntryPatch = serde_json::from_value(json!({
        "id":entry.id,"expectedRevision":entry.revision,"url":"https://legacy.example.invalid"
    }))
    .unwrap();
    desktop.execute(Operation::Update(vec![patch])).unwrap();
    let read: Value = desktop
        .mcp_session()
        .execute(Operation::Read(vec![entry.id]))
        .unwrap();
    assert_eq!(read[0]["siteMissing"], false);
    assert_eq!(read[0]["url"], "https://legacy.example.invalid");
}

#[test]
fn inspect_describes_plain_and_encrypted_files_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    desktop
        .call::<Vec<EntrySummary>>(Operation::Create(vec![input("检查")]))
        .unwrap();
    let plain = dir.path().join("plain.json");
    desktop
        .call::<Value>(Operation::Export(export_args(&plain, false, None)))
        .unwrap();
    let inspected: Value = desktop
        .call(Operation::InspectExport(TransferPathArgs {
            path: plain.to_string_lossy().into_owned(),
        }))
        .unwrap();
    assert_eq!(inspected["encrypted"], false);
    assert_eq!(inspected["format"], "json");
    assert_eq!(inspected["count"], 1);
    assert!(!inspected.to_string().contains("FAKE_CORE_TEST_SECRET"));

    let encrypted = dir.path().join("sealed.vaultenc");
    desktop
        .call::<Value>(Operation::Export(export_args(
            &encrypted,
            true,
            Some("pass phrase"),
        )))
        .unwrap();
    let inspected: Value = desktop
        .call(Operation::InspectExport(TransferPathArgs {
            path: encrypted.to_string_lossy().into_owned(),
        }))
        .unwrap();
    assert_eq!(inspected["encrypted"], true);
    assert!(inspected.get("count").is_none());
}

#[test]
fn export_never_overwrites_the_live_database() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    desktop
        .call::<Vec<EntrySummary>>(Operation::Create(vec![input("保护密码库")]))
        .unwrap();
    let error = desktop
        .call::<Value>(Operation::Export(export_args(
            &dir.path().join("vault.db"),
            false,
            None,
        )))
        .unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
    assert!(
        !desktop.service.storage.list("", None).unwrap().is_empty(),
        "the vault must remain readable"
    );
}

#[test]
fn mcp_export_requires_the_desktop_to_be_online() {
    let dir = tempfile::tempdir().unwrap();
    let mcp = open(dir.path(), true).unwrap();
    let error = mcp
        .execute(Operation::McpExport(McpExportArgs {
            include_trash: false,
        }))
        .unwrap_err();
    assert_eq!(error.code, "DESKTOP_OFFLINE");
}

#[test]
fn mcp_export_waits_for_the_desktop_passphrase_and_returns_status_only() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    desktop
        .bind_desktop_metadata(vault_contracts::desktop::DesktopMetadata {
            webview_version: "test".into(),
            smoke_test: false,
            owner_window: 1,
        })
        .unwrap();
    desktop
        .call::<Vec<EntrySummary>>(Operation::Create(vec![
            input("MCP 导出 A"),
            input("MCP 导出 B"),
        ]))
        .unwrap();
    let mcp = desktop.mcp_session();
    let worker = std::thread::spawn(move || {
        mcp.invoke("transfer.mcpExport", json!({"includeTrash": false}))
    });
    let pending = wait_for_pending(&desktop);
    assert_eq!(pending["count"], 2);
    let responded: Value = desktop
        .call(Operation::TransferRespond(TransferRespondArgs {
            id: pending["id"].as_str().unwrap().to_string(),
            passphrase: Some("desktop only passphrase".into()),
            cancel: false,
        }))
        .unwrap();
    assert_eq!(responded["status"], "exported");
    assert_eq!(responded["count"], 2);
    let path = std::path::PathBuf::from(responded["path"].as_str().unwrap());
    assert!(path.starts_with(std::fs::canonicalize(dir.path()).unwrap()));
    assert!(std::fs::read(&path).unwrap().starts_with(b"VAULTEXP"));

    let value = worker.join().unwrap().unwrap();
    assert_eq!(value["status"], "exported");
    assert_eq!(value["count"], 2);
    let wire = value.to_string();
    assert!(!wire.contains("desktop only passphrase"));
    assert!(!wire.contains("FAKE_CORE_TEST_SECRET"));
    assert!(!wire.contains(path.to_string_lossy().as_ref()));
}

#[test]
fn mcp_export_can_be_cancelled_from_the_desktop() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    desktop
        .bind_desktop_metadata(vault_contracts::desktop::DesktopMetadata {
            webview_version: "test".into(),
            smoke_test: false,
            owner_window: 1,
        })
        .unwrap();
    desktop
        .call::<Vec<EntrySummary>>(Operation::Create(vec![input("取消导出")]))
        .unwrap();
    let mcp = desktop.mcp_session();
    let worker = std::thread::spawn(move || mcp.invoke("transfer.mcpExport", json!({})));
    let pending = wait_for_pending(&desktop);
    let responded: Value = desktop
        .call(Operation::TransferRespond(TransferRespondArgs {
            id: pending["id"].as_str().unwrap().to_string(),
            passphrase: None,
            cancel: true,
        }))
        .unwrap();
    assert_eq!(responded["status"], "cancelled");
    let error = worker.join().unwrap().unwrap_err();
    assert_eq!(error.code, "EXPORT_CANCELLED");
}

#[test]
fn mcp_cannot_read_pending_requests_or_run_desktop_exports() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = open(dir.path(), false).unwrap();
    let mcp = desktop.mcp_session();
    assert_eq!(
        mcp.invoke("transfer.pending", json!({})).unwrap_err().code,
        "PERMISSION_DENIED"
    );
    assert_eq!(
        mcp.invoke(
            "transfer.export",
            json!({
                "format": "json",
                "includeTrash": false,
                "kinds": ["password"],
                "encrypted": false,
                "path": dir.path().join("x.json").to_string_lossy(),
            }),
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
}
