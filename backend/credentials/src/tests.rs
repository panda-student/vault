use super::*;
use vault_contracts::ports::repository::MutationPolicy;
struct Allow;
impl ChangeAuthorizer for Allow {
    fn authorize(&self, _: &Settings, _: Option<&EntrySummary>, _: &EntrySummary) -> Result<()> {
        Ok(())
    }
}
fn input() -> EntryInput {
    serde_json::from_value(
        serde_json::json!({"kind":"password","title":"fixture","secret":"FAKE_DOMAIN_ONLY"}),
    )
    .unwrap()
}
#[test]
fn patch_preserves_omitted_secret_and_rejects_empty_replacement() {
    let value = input();
    let p = policy::CredentialPolicy {
        authorization: &Allow,
    };
    let patch = serde_json::from_value(
        serde_json::json!({"id":"fixture","expectedRevision":1,"notes":"new"}),
    )
    .unwrap();
    assert_eq!(p.patch(value.clone(), patch).unwrap().secret, value.secret);
    let patch = serde_json::from_value(
        serde_json::json!({"id":"fixture","expectedRevision":1,"secret":""}),
    )
    .unwrap();
    assert!(p.patch(value, patch).is_err());
}
#[test]
fn domain_preparation_trims_title_and_rejects_invalid_database_port() {
    let mut value = input();
    value.title = " fixture ".into();
    let p = policy::CredentialPolicy {
        authorization: &Allow,
    };
    p.prepare(&mut value).unwrap();
    assert_eq!(value.title, "fixture");
    let mut database=serde_json::from_value(serde_json::json!({"kind":"database","title":"fixture","secret":"","database":{"databaseType":"Oracle","host":"fixture.invalid","port":"65536","databaseName":"ORCL"}})).unwrap();
    assert!(p.prepare(&mut database).is_err());
}
