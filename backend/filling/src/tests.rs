use super::write::resolve_field;
use vault_contracts::{
    credentials::{CredentialGroup, EntryDetail},
    errors::Result,
    ports::repository::CredentialReader,
};

fn group(id: &str, password: &str, token: &str) -> CredentialGroup {
    CredentialGroup {
        id: id.into(),
        environment: id.into(),
        account: format!("{id}-user"),
        password: password.into(),
        token: token.into(),
    }
}

struct ExampleCredential;
impl CredentialReader for ExampleCredential {
    fn get(&self, _: &str) -> Result<EntryDetail> {
        let summary = serde_json::from_value(serde_json::json!({
            "id":"example", "kind":"api", "title":"Example",
            "url":"", "notes":"", "pinned":false, "createdAt":0, "updatedAt":0,
            "revision":1, "services":[
                {"id":"a", "protocol":"openai", "url":"https://example.com/a"},
                {"id":"b", "protocol":"anthropic", "url":"https://example.com/b"}
            ]
        }))
        .unwrap();
        Ok(EntryDetail {
            summary,
            groups: vec![
                group("prod", "", "synthetic-secret"),
                group("test", "test-password", "test-token"),
            ],
        })
    }
}

#[test]
fn multiple_endpoints_require_an_explicit_selection() {
    assert!(resolve_field(&ExampleCredential, "example", "service_url", None, None).is_err());
    assert_eq!(
        resolve_field(
            &ExampleCredential,
            "example",
            "service_url",
            Some("b"),
            None
        )
        .unwrap(),
        "https://example.com/b"
    );
}

#[test]
fn field_resolution_rejects_unknown_fields_and_missing_services() {
    assert!(resolve_field(&ExampleCredential, "example", "unknown", None, None).is_err());
    assert!(
        resolve_field(
            &ExampleCredential,
            "example",
            "service_url",
            Some("missing"),
            None
        )
        .is_err()
    );
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "secret", None, Some("prod")).unwrap(),
        "synthetic-secret"
    );
}

// 多组凭据时必须指定 groupId；secret 优先取密码，没有密码时取 Token；空字段报错而不是写出空值。
#[test]
fn multiple_groups_require_group_selection() {
    let error = resolve_field(&ExampleCredential, "example", "password", None, None).unwrap_err();
    assert_eq!(error.code, "GROUP_REQUIRED");
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "secret", None, Some("test")).unwrap(),
        "test-password"
    );
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "token", None, Some("test")).unwrap(),
        "test-token"
    );
    assert_eq!(
        resolve_field(&ExampleCredential, "example", "account", None, Some("prod")).unwrap(),
        "prod-user"
    );
    let empty = resolve_field(
        &ExampleCredential,
        "example",
        "password",
        None,
        Some("prod"),
    )
    .unwrap_err();
    assert_eq!(empty.code, "FIELD_EMPTY");
    let missing =
        resolve_field(&ExampleCredential, "example", "token", None, Some("nope")).unwrap_err();
    assert_eq!(missing.code, "GROUP_NOT_FOUND");
}
