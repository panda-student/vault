use super::*;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn manifest_json(version: &str, bytes: &[u8]) -> Manifest {
    Manifest {
        schema: 3,
        version: version.into(),
        platform: "windows-x86_64".into(),
        url: format!(
            "https://github.com/panda-student/vault/releases/download/v{version}/vault-{version}-setup.exe"
        ),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
        notes: "test".into(),
    }
}
fn sign_manifest(manifest: &Manifest) -> Envelope {
    let key = SigningKey::from_bytes(&[27; 32]);
    let payload = serde_json::to_vec(manifest).unwrap();
    Envelope {
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
    }
}
fn signed(version: &str, bytes: &[u8]) -> (Envelope, String) {
    let key = SigningKey::from_bytes(&[27; 32]);
    let envelope = sign_manifest(&manifest_json(version, bytes));
    (envelope, STANDARD.encode(key.verifying_key().to_bytes()))
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
    let (_, key) = signed("1.2.3", b"MZtest");
    let mut manifest = manifest_json("1.2.3", b"MZtest");
    for repository in ["local-password-manager", "untrusted"] {
        manifest.url = format!(
            "https://github.com/panda-student/{repository}/releases/download/v1.2.3/vault-1.2.3-setup.exe"
        );
        assert!(decode(&sign_manifest(&manifest), &key, "1.0.0").is_err());
    }
    manifest.url = manifest_json("1.2.3", b"MZtest").url;
    manifest.schema = 2;
    assert!(decode(&sign_manifest(&manifest), &key, "1.0.0").is_err());
    manifest.schema = 1;
    assert!(decode(&sign_manifest(&manifest), &key, "1.0.0").is_err());
}
#[test]
fn signed_installer_rejects_incomplete_metadata_and_wrong_asset() {
    let (_, key) = signed("1.2.3", b"MZtest");
    let manifest = manifest_json("1.2.3", b"MZtest");
    let mut wrong_asset = manifest.clone();
    wrong_asset.url =
        "https://github.com/panda-student/vault/releases/download/v1.2.3/vault.exe".into();
    let mut empty_hash = manifest.clone();
    empty_hash.sha256 = "x".repeat(64);
    let mut tiny = manifest.clone();
    tiny.size = 1;
    for variant in [wrong_asset, empty_hash, tiny] {
        assert!(decode(&sign_manifest(&variant), &key, "1.0.0").is_err());
    }
    assert!(decode(&sign_manifest(&manifest), &key, "1.0.0").is_ok());
}
#[test]
fn signature_binds_installer_hash_size_and_stable_channel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault-1.2.3-setup.exe");
    fs::write(&path, b"MZtest").unwrap();
    let (envelope, key) = signed("1.2.3", b"MZtest");
    let manifest = decode(&envelope, &key, "1.0.0").unwrap();
    assert!(verify_binary(&path, &manifest).is_ok());
    fs::write(&path, b"MZevil").unwrap();
    assert!(verify_binary(&path, &manifest).is_err());
    fs::write(&path, b"XXtest").unwrap();
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
#[test]
fn older_manifest_schemas_read_as_a_version_not_as_an_error() {
    let key = SigningKey::from_bytes(&[27; 32]);
    let public = STANDARD.encode(key.verifying_key().to_bytes());
    // The previous release line published schema 2 with archive fields.
    let payload = serde_json::to_vec(&serde_json::json!({
        "schema": 2, "version": "1.0.0", "platform": "windows-x86_64",
        "archive_sha256": "a".repeat(64), "archive_size": 1, "notes": "old"
    }))
    .unwrap();
    let envelope = Envelope {
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
    };
    assert_eq!(
        signed_version(&envelope, &public).unwrap(),
        semver::Version::new(1, 0, 0)
    );
    assert!(decode(&envelope, &public, "0.9.0").is_err());
    // An unsigned or tampered payload is never read as a version.
    let tampered = Envelope {
        payload: STANDARD.encode(b"{\"version\":\"9.9.9\"}"),
        signature: envelope.signature.clone(),
    };
    assert!(signed_version(&tampered, &public).is_err());
}
