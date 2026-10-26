use super::write::resolve_field;
use vault_contracts::{
    credentials::EntryDetail, errors::Result, ports::repository::CredentialReader,
};

struct ExampleCredential;
impl CredentialReader for ExampleCredential {
    fn get(&self, _: &str) -> Result<EntryDetail> {
        let summary = serde_json::from_value(serde_json::json!({
            "id":"example", "kind":"api", "title":"Example", "account":"user",
            "url":"", "notes":"", "pinned":false, "createdAt":0, "updatedAt":0,
            "revision":1, "services":[
                {"id":"a", "protocol":"openai", "url":"https://example.com/a"},
                {"id":"b", "protocol":"anthropic", "url":"https://example.com/b"}
            ]
        }))
        .unwrap();
        Ok(EntryDetail {
            summary,
            secret: "synthetic-secret".into(),
        })
    }
}

#[test]
fn multiple_endpoints_require_an_explicit_selection() {
    assert!(resolve_field(&ExampleCredential, "example", "service_url", None).is_err());
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "service_url", Some("b")).unwrap(),
        "https://example.com/b"
    );
}

#[test]
fn field_resolution_rejects_unknown_fields_and_missing_services() {
    assert!(resolve_field(&ExampleCredential, "example", "unknown", None).is_err());
    assert!(
        resolve_field(
            &ExampleCredential,
            "example",
            "service_url",
            Some("missing")
        )
        .is_err()
    );
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "secret", None).unwrap(),
        "synthetic-secret"
    );
}
