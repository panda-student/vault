use super::*;
use crate::common::files::copy_move;
use ed25519_dalek::{Signer, SigningKey};
#[test]
fn zip_reader_rejects_noncanonical_member_name() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("noncanonical.zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
    zip.start_file(
        "vault-0.5.5/Vault.exe",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"MZtest").unwrap();
    zip.finish().unwrap();
    let output = dir.path().join("payload.exe");
    assert!(extract_update_payload(&archive, &output, "vault-0.5.5/vault.exe", 6).is_err());
    assert!(!output.exists());
}
#[test]
fn redirected_volume_fallback_preserves_bytes_and_does_not_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("old.exe");
    let target = directory.path().join("new.exe");
    fs::write(&source, b"MZtest").unwrap();
    fs::write(&target, b"existing").unwrap();
    assert!(copy_move(&source, &target).is_err());
    assert_eq!(fs::read(&source).unwrap(), b"MZtest");
    fs::remove_file(&target).unwrap();
    copy_move(&source, &target).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), b"MZtest");
}
fn signed(version: &str, bytes: &[u8]) -> (Envelope, String) {
    let key = SigningKey::from_bytes(&[27; 32]);
    let payload = serde_json::to_vec(&Manifest { schema: 2, version: version.into(), platform: "windows-x86_64".into(), url: format!("https://github.com/panda-student/vault/releases/download/v{version}/vault-{version}.zip"), sha256: format!("{:x}", Sha256::digest(bytes)), size: bytes.len() as u64, notes: "test".into(), archive_sha256: "a".repeat(64), archive_size: 100 }).unwrap();
    (
        Envelope {
            payload: STANDARD.encode(&payload),
            signature: STANDARD.encode(key.sign(&payload).to_bytes()),
        },
        STANDARD.encode(key.verifying_key().to_bytes()),
    )
}
#[test]
fn signed_payload_rejects_tampering_wrong_key_and_downgrade() {
    let (mut envelope, key) = signed("1.2.3", b"MZtest");
    assert!(decode(&envelope, &key, "1.2.2").is_ok());
    assert!(decode(&envelope, &key, "1.2.3").is_err());
    assert!(decode(&envelope, &STANDARD.encode([9; 32]), "1.0.0").is_err());
    envelope.payload = STANDARD.encode(b"tampered");
    assert!(decode(&envelope, &key, "1.0.0").is_err());
}
#[test]
fn updates_reject_previous_repositories_and_manifest_formats() {
    let (mut envelope, key) = signed("1.2.3", b"MZtest");
    let signing_key = SigningKey::from_bytes(&[27; 32]);
    let mut manifest = decode(&envelope, &key, "1.0.0").unwrap();
    for (repository, accepted) in [("local-password-manager", false), ("untrusted", false)] {
        manifest.url = format!(
            "https://github.com/panda-student/{repository}/releases/download/v1.2.3/vault.exe"
        );
        let payload = serde_json::to_vec(&manifest).unwrap();
        envelope.payload = STANDARD.encode(&payload);
        envelope.signature = STANDARD.encode(signing_key.sign(&payload).to_bytes());
        assert_eq!(decode(&envelope, &key, "1.0.0").is_ok(), accepted);
    }
    manifest.schema = 1;
    let payload = serde_json::to_vec(&manifest).unwrap();
    envelope.payload = STANDARD.encode(&payload);
    envelope.signature = STANDARD.encode(signing_key.sign(&payload).to_bytes());
    assert!(decode(&envelope, &key, "1.0.0").is_err());
}
#[test]
fn signed_zip_rejects_incomplete_metadata_and_wrong_asset() {
    let (mut envelope, key) = signed("1.2.3", b"MZtest");
    let signing_key = SigningKey::from_bytes(&[27; 32]);
    let manifest = decode(&envelope, &key, "1.0.0").unwrap();
    let mut invalid = serde_json::to_value(&manifest).unwrap();
    let variants = [
        {
            let mut v = invalid.clone();
            v.as_object_mut().unwrap().remove("archive_size");
            v
        },
        {
            let mut v = invalid.clone();
            v["archive_sha256"] = serde_json::json!("x".repeat(64));
            v
        },
        {
            let mut v = invalid.clone();
            v["archive_size"] = serde_json::json!(0);
            v
        },
        {
            invalid["url"] = serde_json::json!(
                "https://github.com/panda-student/vault/releases/download/v1.2.3/vault.exe"
            );
            invalid
        },
    ];
    for value in variants {
        let payload = serde_json::to_vec(&value).unwrap();
        envelope.payload = STANDARD.encode(&payload);
        envelope.signature = STANDARD.encode(signing_key.sign(&payload).to_bytes());
        assert!(decode(&envelope, &key, "1.0.0").is_err());
    }
}
#[test]
fn zip_extraction_only_writes_expected_member_and_enforces_size() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.zip");
    let output = dir.path().join("payload.exe");
    let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
    for name in ["vault-1.2.3/vault.exe", "../outside.txt"] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"MZtest").unwrap();
    }
    zip.finish().unwrap();
    assert!(extract_update_payload(&archive, &output, "vault-1.2.3/vault.exe", 6).is_ok());
    assert_eq!(fs::read(&output).unwrap(), b"MZtest");
    assert!(!dir.path().parent().unwrap().join("outside.txt").exists());
    assert!(extract_update_payload(&archive, &output, "vault-1.2.3/vault.exe", 6).is_err());
    assert!(
        extract_update_payload(
            &archive,
            &dir.path().join("bad.exe"),
            "vault-1.2.3/vault.exe",
            7
        )
        .is_err()
    );
}
#[test]
fn signature_binds_binary_hash_size_and_stable_channel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("payload.exe");
    fs::write(&path, b"MZtest").unwrap();
    let (envelope, key) = signed("1.2.3", b"MZtest");
    let manifest = decode(&envelope, &key, "1.0.0").unwrap();
    assert!(verify_binary(&path, &manifest).is_ok());
    fs::write(&path, b"MZevil").unwrap();
    assert!(verify_binary(&path, &manifest).is_err());
    let (preview, key) = signed("1.2.4-beta.1", b"MZtest");
    assert!(decode(&preview, &key, "1.0.0").is_err());
}
#[test]
fn other_sessions_prevent_replacement_and_recover_after_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.exe");
    fs::write(&path, b"test").unwrap();
    let one = UpdateRuntime::acquire(&path).unwrap();
    let two = UpdateRuntime::acquire(&path).unwrap();
    assert_eq!(one.claim().unwrap_err().code, "UPDATE_CLIENTS_ACTIVE");
    drop(two);
    one.claim().unwrap();
    assert!(UpdateRuntime::acquire(&path).is_err());
    one.release_claim();
    assert!(UpdateRuntime::acquire(&path).is_ok());
}
