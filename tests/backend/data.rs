use super::*;
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

/// Tests use the production domain policy while exercising the concrete repository.
pub(super) fn policy() -> impl crate::definitions::ports::repository::MutationPolicy {
    crate::services::credentials::policy::CredentialPolicy {
        caller: crate::services::permissions::Caller::Desktop,
        access: crate::services::permissions::Access::Create,
    }
}
