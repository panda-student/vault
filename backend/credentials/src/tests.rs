use super::*;
use vault_contracts::ports::repository::MutationPolicy;
struct Allow;
impl ChangeAuthorizer for Allow {
    fn authorize(&self, _: &Settings, _: Option<&EntrySummary>, _: &EntrySummary) -> Result<()> {
        Ok(())
    }
}
fn input() -> EntryInput {
    serde_json::from_value(serde_json::json!({
        "kind":"password","title":"fixture","url":"https://fixture.example.invalid",
        "secret":"FAKE_DOMAIN_ONLY"
    }))
    .unwrap()
}
fn strict() -> policy::CredentialPolicy<'static> {
    policy::CredentialPolicy {
        authorization: &Allow,
        site: validation::SiteRule::Required,
    }
}
#[test]
fn patch_preserves_omitted_secret_and_rejects_empty_replacement() {
    let value = input();
    let p = strict();
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
    value.url = " https://fixture.example.invalid ".into();
    let p = strict();
    p.prepare(&mut value).unwrap();
    assert_eq!(value.title, "fixture");
    assert_eq!(value.url, "https://fixture.example.invalid");
    let mut database=serde_json::from_value(serde_json::json!({"kind":"database","title":"fixture","secret":"","database":{"databaseType":"Oracle","host":"fixture.invalid","port":"65536","databaseName":"ORCL"}})).unwrap();
    assert!(p.prepare(&mut database).is_err());
}
#[test]
fn legacy_missing_site_imports_but_the_next_update_requires_it() {
    let mut legacy = serde_json::from_value(
        serde_json::json!({"kind":"password","title":"legacy","secret":"FAKE_LEGACY_ONLY"}),
    )
    .unwrap();
    validation::prepare(&mut legacy, validation::SiteRule::LegacyAllowed).unwrap();
    let patch = serde_json::from_value(
        serde_json::json!({"id":"legacy","expectedRevision":1,"notes":"keep"}),
    )
    .unwrap();
    let error = strict().patch(legacy.clone(), patch).unwrap_err();
    assert_eq!(error.details.unwrap()[0].field, "url");
    let patch = serde_json::from_value(
        serde_json::json!({"id":"legacy","expectedRevision":1,"url":" https://legacy.example.invalid "}),
    )
    .unwrap();
    assert_eq!(
        strict().patch(legacy, patch).unwrap().url,
        "https://legacy.example.invalid"
    );
}
#[test]
fn api_site_requires_a_service_address_unless_importing() {
    let mut api = serde_json::from_value(
        serde_json::json!({"kind":"api","title":"legacy api","secret":"FAKE_API_ONLY"}),
    )
    .unwrap();
    let error = strict().prepare(&mut api).unwrap_err();
    assert_eq!(error.details.unwrap()[0].field, "services");
    validation::prepare(&mut api, validation::SiteRule::LegacyAllowed).unwrap();
    assert!(api.services.is_empty() && api.url.is_empty());
    let patch = serde_json::from_value(serde_json::json!({
        "id":"legacy-api","expectedRevision":1,
        "services":[{"protocol":"custom","url":"https://api.example.invalid"}]
    }))
    .unwrap();
    let updated = strict().patch(api, patch).unwrap();
    assert_eq!(updated.url, "https://api.example.invalid");
}
