use super::fixture::TestVault as Vault;
use super::*;
use rusqlite::Connection;
use vault_contracts::ports::repository::CredentialReader;
#[test]
fn only_current_database_format_is_opened_without_rewriting_old_files() {
    for version in [0, 1, 2, 3, 5] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!("CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('keep'); PRAGMA user_version={version};")).unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            Vault::open(dir.path()).err().unwrap().code,
            "UNSUPPORTED_DATABASE"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.path().join("backups").exists());
    }
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    assert_eq!(
        vault
            .connection()
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert!(!dir.path().join("backups").exists());
}
#[test]
fn obsolete_group_protocol_and_api_url_inputs_are_rejected() {
    let mut old = serde_json::to_value(value()).unwrap();
    old["group"] = serde_json::json!("removed");
    assert!(serde_json::from_value::<EntryInput>(old).is_err());
    assert!(serde_json::from_str::<ApiProtocol>("\"figma\"").is_err());
    assert!(serde_json::from_str::<ApiProtocol>("\"gitea\"").is_err());
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let mut input = value();
    input.services.clear();
    assert!(vault.create(input).is_err());
    assert!(
        serde_json::from_str::<Settings>(r#"{"mcpEnabled":true,"closeToTray":false}"#).is_err()
    );
}
#[test]
fn multiple_service_addresses_roundtrip_without_losing_secret() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let entry = vault.create(value()).unwrap();
    let patch: EntryPatch = serde_json::from_value(serde_json::json!({"id":entry.id,"expectedRevision":1,"services":[{"protocol":"openai","url":"https://first-protocol-fixture.invalid/v1"},{"protocol":"gemini","url":"https://second-protocol-fixture.invalid/v1beta"}]})).unwrap();
    let updated = vault.update(patch).unwrap();
    assert_eq!(updated.services.len(), 2);
    assert_ne!(updated.services[0].id, updated.services[1].id);
    assert_eq!(updated.url, updated.services[0].url);
    assert_eq!(updated.service(None).unwrap_err().code, "SERVICE_REQUIRED");
    let second_id = updated.services[1].id.clone();
    assert_eq!(
        updated.service(Some(&second_id)).unwrap().protocol,
        ApiProtocol::Gemini
    );
    drop(vault);
    let vault = Vault::open(dir.path()).unwrap();
    let current = vault.get(&entry.id).unwrap();
    assert_eq!(current.secret, value().secret);
    assert_eq!(current.summary.services[1].id, second_id);
    assert_eq!(
        current.summary.services[0].url,
        "https://first-protocol-fixture.invalid/v1"
    );
    let bytes = std::fs::read(dir.path().join("vault.db")).unwrap();
    assert!(
        !bytes
            .windows(b"second-protocol-fixture".len())
            .any(|w| w == b"second-protocol-fixture")
    );
    let patch: EntryPatch = serde_json::from_value(
        serde_json::json!({"id":entry.id,"expectedRevision":2,"services":[]}),
    )
    .unwrap();
    let cleared = vault.update(patch).unwrap();
    assert!(cleared.services.is_empty() && cleared.url.is_empty());
}
#[test]
fn invalid_service_sets_do_not_commit_partial_batch() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let mut duplicate = value();
    duplicate.url.clear();
    duplicate.services = vec![
        ApiService {
            id: "same".into(),
            protocol: ApiProtocol::Openai,
            url: "https://a.invalid/v1".into(),
        },
        ApiService {
            id: "same".into(),
            protocol: ApiProtocol::Gemini,
            url: "https://b.invalid/v1beta".into(),
        },
    ];
    assert!(vault.create_many(vec![value(), duplicate]).is_err());
    assert!(vault.list("", None).unwrap().is_empty());
    let mut wrong_kind = value();
    wrong_kind.kind = Kind::Password;
    wrong_kind.services = vec![ApiService {
        id: "one".into(),
        protocol: ApiProtocol::Custom,
        url: "https://a.invalid".into(),
    }];
    assert!(vault.create(wrong_kind).is_err());
}
#[test]
fn batch_writes_roll_back_and_remain_compatible_with_single_operations() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let mut bad = value();
    bad.secret.clear();
    assert!(vault.create_many(vec![value(), bad]).is_err());
    assert!(vault.list("", None).unwrap().is_empty());
    let entries = vault.create_many(vec![value(), value()]).unwrap();
    let patch = |id: &str, revision: i64| {
        serde_json::from_value::<EntryPatch>(serde_json::json!({"id":id,"expectedRevision":revision,"title":"batch-updated","secret":"batch-rotated-value"})).unwrap()
    };
    assert_eq!(
        vault
            .update_many(vec![patch(&entries[0].id, 1), patch(&entries[1].id, 99)])
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    assert_eq!(vault.get(&entries[0].id).unwrap().summary.revision, 1);
    assert_eq!(vault.get(&entries[0].id).unwrap().secret, value().secret);
    assert!(
        !vault
            .protected_values()
            .unwrap()
            .iter()
            .any(|v| v == "batch-rotated-value")
    );
    let versions = |revision| {
        vec![
            EntryVersion {
                id: entries[0].id.clone(),
                expected_revision: 1,
            },
            EntryVersion {
                id: entries[1].id.clone(),
                expected_revision: revision,
            },
        ]
    };
    assert_eq!(
        vault.delete_many(versions(99)).unwrap_err().code,
        "CONFLICT"
    );
    assert_eq!(vault.list("", None).unwrap().len(), 2);
    assert!(
        vault
            .update_many(vec![patch(&entries[0].id, 1), patch(&entries[0].id, 2)])
            .is_err()
    );
    let updated = vault
        .update_many(vec![patch(&entries[0].id, 1), patch(&entries[1].id, 1)])
        .unwrap();
    assert!(updated.iter().all(|v| v.revision == 2));
    vault.delete(&entries[0].id, 2).unwrap();
    assert_eq!(
        vault
            .delete_many(vec![EntryVersion {
                id: entries[1].id.clone(),
                expected_revision: 2
            }])
            .unwrap(),
        vec![entries[1].id.clone()]
    );
    drop(vault);
    let vault = Vault::open(dir.path()).unwrap();
    assert!(vault.list("", None).unwrap().is_empty());
    assert!(
        vault
            .protected_values()
            .unwrap()
            .iter()
            .any(|v| v == "batch-rotated-value")
    );
}
#[test]
fn rotated_values_remain_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let e = vault.create(value()).unwrap();
    let patch: EntryPatch = serde_json::from_value(
        serde_json::json!({"id":e.id,"expectedRevision":1,"secret":"replacement-fixture-secret"}),
    )
    .unwrap();
    vault.update(patch).unwrap();
    drop(vault);
    let vault = Vault::open(dir.path()).unwrap();
    assert!(
        vault
            .protected_values()
            .unwrap()
            .iter()
            .any(|v| v == &value().secret)
    );
    vault.delete(&e.id, 2).unwrap();
    drop(vault);
    let vault = Vault::open(dir.path()).unwrap();
    assert!(
        vault
            .protected_values()
            .unwrap()
            .iter()
            .any(|v| v == "replacement-fixture-secret")
    );
}
fn value() -> EntryInput {
    EntryInput {
        tags: vec![],
        database: None,
        kind: Kind::Api,
        title: "fixture-private-title".into(),
        account: "fixture-account".into(),
        url: "https://fixture.example.com".into(),
        documentation_url: String::new(),
        services: vec![ApiService {
            id: String::new(),
            protocol: ApiProtocol::Custom,
            url: "https://fixture.example.com".into(),
        }],
        secret: "fixture-secret-DO-NOT-LEAK".into(),
        notes: "fixture-private-notes".into(),
    }
}
#[test]
fn encrypted_roundtrip_conflicts_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let created = vault.create(value()).unwrap();
    assert!(
        !serde_json::to_string(&created)
            .unwrap()
            .contains("DO-NOT-LEAK")
    );
    assert_eq!(vault.get(&created.id).unwrap().secret, value().secret);
    let pinned = vault.pin(&created.id, 1, true).unwrap();
    assert_eq!(
        vault.pin(&created.id, 1, false).unwrap_err().code,
        "CONFLICT"
    );
    drop(vault);
    let vault = Vault::open(dir.path()).unwrap();
    assert!(vault.list("fixture", Some(Kind::Api)).unwrap()[0].pinned);
    assert!(vault.list("DO-NOT-LEAK", None).unwrap().is_empty());
    vault.delete(&created.id, pinned.revision).unwrap();
    assert!(vault.list("", None).unwrap().is_empty());
}
#[test]
fn ciphertext_and_key_tampering_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let entry = vault.create(value()).unwrap();
    // Corrupt through a separate connection, as an external file modification.
    let conn = Connection::open(dir.path().join("vault.db")).unwrap();
    conn.execute(
        "UPDATE entries SET encrypted=zeroblob(50) WHERE id=?1",
        [&entry.id],
    )
    .unwrap();
    assert_eq!(vault.get(&entry.id).err().unwrap().code, "DECRYPT_FAILED");
    conn.execute(
        "UPDATE metadata SET value=zeroblob(50) WHERE name='wrapped_key'",
        [],
    )
    .unwrap();
    drop(vault);
    assert_eq!(Vault::open(dir.path()).err().unwrap().code, "UNLOCK_FAILED");
}
#[test]
fn no_plaintext_in_database_or_wal() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let e = value();
    vault.create(e.clone()).unwrap();
    for file in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        for secret in [&e.title, &e.secret, &e.account, &e.url, &e.notes] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
        }
    }
}
#[test]
fn unrelated_and_zero_length_databases_are_not_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.db");
    std::fs::write(&path, b"unrelated data").unwrap();
    assert!(Vault::open(dir.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"unrelated data");
    std::fs::write(&path, []).unwrap();
    assert!(Vault::open(dir.path()).is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}
#[test]
fn partial_update_preserves_secret_and_rejects_blank() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let e = vault.create(value()).unwrap();
    let patch: EntryPatch = serde_json::from_value(
        serde_json::json!({"id":e.id,"expectedRevision":1,"notes":"new-note"}),
    )
    .unwrap();
    vault.update(patch).unwrap();
    assert_eq!(vault.get(&e.id).unwrap().secret, value().secret);
    let patch: EntryPatch =
        serde_json::from_value(serde_json::json!({"id":e.id,"expectedRevision":2,"secret":""}))
            .unwrap();
    assert_eq!(vault.update(patch).err().unwrap().code, "INVALID_INPUT");
}

#[cfg(test)]
mod tags {
    use super::*;
    fn database() -> EntryInput {
        serde_json::from_value(serde_json::json!({
            "kind":"database","title":"orders","tags":["orders-system","UVT"],
            "database":{"databaseType":"Oracle","host":"fixture.invalid","port":"1521","databaseName":"ORCL"},
            "secret":"","account":"fixture"
        })).unwrap()
    }
    #[test]
    fn database_organization_and_renames_persist_including_trash() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.organize("tag", "empty-tag", None).unwrap();
        let a = vault.create(database()).unwrap();
        let b = vault.create(database()).unwrap();
        vault.delete(&b.id, b.revision).unwrap();
        vault
            .organize("tag", "renamed-system", Some("orders-system"))
            .unwrap();
        vault.organize("tag", "staging", Some("UVT")).unwrap();
        assert!(
            vault
                .get(&a.id)
                .unwrap()
                .summary
                .tags
                .contains(&"renamed-system".into())
        );
        assert!(vault.delete(&a.id, a.revision).is_err());
        let deleted = vault.list_trash().unwrap();
        assert!(deleted[0].summary.tags.contains(&"staging".into()));
        assert_eq!(
            deleted[0].summary.database.as_ref().unwrap().database_name,
            "ORCL"
        );
        assert_eq!(vault.list("fixture.invalid", None).unwrap().len(), 1);
        assert_eq!(vault.list("staging", None).unwrap().len(), 1);
        drop(vault);
        let reopened = Vault::open(dir.path()).unwrap();
        assert!(
            reopened
                .organization()
                .unwrap()
                .tags
                .contains(&"empty-tag".into())
        );
        assert_eq!(reopened.get(&a.id).unwrap().secret, "");
        let bytes = std::fs::read(dir.path().join("vault.db")).unwrap();
        assert!(
            !bytes
                .windows(b"renamed-system".len())
                .any(|w| w == b"renamed-system")
        );
    }
    #[test]
    fn invalid_database_fields_and_duplicate_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let mut value = database();
        value.database.as_mut().unwrap().port = "65536".into();
        assert!(vault.create(value).is_err());
        vault.organize("tag", "one", None).unwrap();
        vault.organize("tag", "two", None).unwrap();
        assert!(vault.organize("tag", "two", Some("one")).is_err());
        assert!(vault.organization().unwrap().tags.contains(&"one".into()));
    }
}

#[cfg(test)]
mod trash {
    use super::*;
    fn input() -> EntryInput {
        serde_json::from_value(serde_json::json!({
            "kind":"password","title":"trash fixture","secret":"trash-fixture-secret",
            "account":"fixture-user","notes":"retained note"
        }))
        .unwrap()
    }
    #[test]
    fn soft_delete_blocks_normal_access_and_restores_with_new_revision() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let entry = vault.create(input()).unwrap();
        vault.delete(&entry.id, entry.revision).unwrap();
        assert!(vault.get(&entry.id).is_err());
        assert!(vault.list("", None).unwrap().is_empty());
        assert!(vault.pin(&entry.id, entry.revision, true).is_err());
        assert!(
            vault
                .protected_values()
                .unwrap()
                .iter()
                .any(|s| s == "trash-fixture-secret")
        );
        drop(vault);
        let vault = Vault::open(dir.path()).unwrap();
        let trash = vault.list_trash().unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].summary.notes, "retained note");
        let restored = vault.restore(&entry.id, entry.revision).unwrap();
        assert!(restored.revision > entry.revision);
        assert_eq!(vault.get(&entry.id).unwrap().secret, "trash-fixture-secret");
        assert!(vault.delete(&entry.id, entry.revision).is_err());
        assert!(vault.list_trash().unwrap().is_empty());
    }

    #[test]
    fn purge_requires_exact_snapshot_and_leaves_active_entries() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let first = vault.create(input()).unwrap();
        let active = vault.create(input()).unwrap();
        vault.delete(&first.id, 1).unwrap();
        assert!(vault.empty_trash(vec![]).is_err());
        let count = vault
            .empty_trash(vec![EntryVersion {
                id: first.id.clone(),
                expected_revision: 1,
            }])
            .unwrap();
        assert_eq!(count, 1);
        assert!(vault.restore(&first.id, 1).is_err());
        assert!(vault.get(&active.id).is_ok());
    }
    #[test]
    fn purge_removes_owned_retired_secret_material() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let entry = vault.create(input()).unwrap();
        let patch: EntryPatch = serde_json::from_value(serde_json::json!({
            "id":entry.id,"expectedRevision":1,"secret":"rotated-trash-fixture"
        }))
        .unwrap();
        vault.update(patch).unwrap();
        vault.delete(&entry.id, 2).unwrap();
        vault
            .empty_trash(vec![EntryVersion {
                id: entry.id,
                expected_revision: 2,
            }])
            .unwrap();
        assert!(vault.protected_values().unwrap().is_empty());
    }
}
