use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Policy {
    calls: AtomicUsize,
    deny_second: bool,
}
impl MutationPolicy for Policy {
    fn prepare(&self, _: &mut EntryInput) -> Result<()> {
        Ok(())
    }
    fn patch(&self, _: EntryInput, _: EntryPatch) -> Result<EntryInput> {
        Err(AppError::invalid())
    }
    fn authorize(&self, _: &Settings, _: Option<&EntrySummary>, _: &EntrySummary) -> Result<()> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 1 && self.deny_second {
            Err(AppError::new("TEST_DENIED", "fixture policy"))
        } else {
            Ok(())
        }
    }
}
fn policy(deny_second: bool) -> Policy {
    Policy {
        calls: AtomicUsize::new(0),
        deny_second,
    }
}
fn input() -> EntryInput {
    serde_json::from_value(serde_json::json!({"kind":"password","title":"isolated record","secret":"FAKE_STORAGE_MODULE_ONLY"})).unwrap()
}
#[test]
fn transaction_policy_failure_rolls_back_the_entire_batch() {
    let directory = tempfile::tempdir().unwrap();
    let store = Vault::open(directory.path()).unwrap();
    assert_eq!(
        store
            .create_many_checked(vec![input(), input()], &policy(true))
            .unwrap_err()
            .code,
        "TEST_DENIED"
    );
    assert!(store.list("", None).unwrap().is_empty());
}
#[test]
fn persisted_bytes_never_contain_secret_and_tampering_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let store = Vault::open(directory.path()).unwrap();
    let value = input();
    let secret = value.secret.clone();
    let entry = store
        .create_many_checked(vec![value], &policy(false))
        .unwrap()
        .remove(0);
    assert_eq!(store.get(&entry.id).unwrap().secret, secret);
    for file in std::fs::read_dir(directory.path()).unwrap() {
        let bytes = std::fs::read(file.unwrap().path()).unwrap();
        assert!(!bytes.windows(secret.len()).any(|v| v == secret.as_bytes()));
    }
    store
        .connection()
        .unwrap()
        .execute(
            "UPDATE entries SET encrypted=zeroblob(50) WHERE id=?1",
            [&entry.id],
        )
        .unwrap();
    assert_eq!(store.get(&entry.id).err().unwrap().code, "DECRYPT_FAILED");
}
#[test]
fn incompatible_database_is_rejected_without_modification() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA user_version=3; CREATE TABLE sentinel(value TEXT);")
        .unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();
    assert!(Vault::open(directory.path()).is_err());
    assert_eq!(before, std::fs::read(path).unwrap());
}
