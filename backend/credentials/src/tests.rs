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
    assert_eq!(p.patch(value.clone(), patch).unwrap().groups, value.groups);
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

fn patch(value: serde_json::Value) -> EntryPatch {
    serde_json::from_value(value).unwrap()
}

// 一个网站下可以放生产、测试等多组凭据，每组可同时有密码和 Token；
// 按 id 修改时未传的字段保留，未提到的组不受影响，删除必须显式 removeGroups。
#[test]
fn groups_are_updated_by_id_and_removed_only_explicitly() {
    let mut value: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"password","title":"site","url":"https://site.invalid",
        "groups":[
            {"environment":" 生产 ","account":"prod","password":"prod-pass"},
            {"environment":"测试","account":"test","token":"test-token"},
            {}
        ]
    }))
    .unwrap();
    strict().prepare(&mut value).unwrap();
    assert_eq!(value.groups.len(), 2, "empty groups are dropped");
    assert_eq!(value.groups[0].environment, "生产");
    let (prod, test) = (value.groups[0].id.clone(), value.groups[1].id.clone());
    let updated = strict()
        .patch(
            value.clone(),
            patch(serde_json::json!({"id":"x","expectedRevision":1,
                "groups":[{"id":prod,"token":"prod-token"},{"environment":"预发","account":"pre","password":"pre-pass"}]})),
        )
        .unwrap();
    assert_eq!(updated.groups.len(), 3);
    assert_eq!(updated.groups[0].password, "prod-pass");
    assert_eq!(updated.groups[0].token, "prod-token");
    assert_eq!(updated.groups[1].token, "test-token");
    assert_eq!(updated.groups[2].environment, "预发");
    let removed = strict()
        .patch(
            updated,
            patch(serde_json::json!({"id":"x","expectedRevision":1,"removeGroups":[test]})),
        )
        .unwrap();
    assert_eq!(removed.groups.len(), 2);
    let error = strict()
        .patch(
            value.clone(),
            patch(serde_json::json!({"id":"x","expectedRevision":1,"groups":[{"id":"missing","token":"t"}]})),
        )
        .unwrap_err();
    assert_eq!(error.details.unwrap()[0].field, "groups.id");
    let error = strict()
        .patch(
            value,
            patch(serde_json::json!({"id":"x","expectedRevision":1,"secret":"s"})),
        )
        .unwrap_err();
    assert_eq!(error.details.unwrap()[0].field, "groups");
}

// 旧客户端的单账号写法（account + secret）仍可用，自动变成一组凭据。
#[test]
fn single_login_shorthand_becomes_one_group() {
    let mut api: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"api","title":"key","account":"dev","secret":"sk-1",
        "services":[{"protocol":"openai","url":"https://api.invalid/v1"}]
    }))
    .unwrap();
    strict().prepare(&mut api).unwrap();
    assert_eq!(api.groups.len(), 1);
    assert_eq!(api.groups[0].token, "sk-1");
    let updated = strict()
        .patch(
            api,
            patch(serde_json::json!({"id":"x","expectedRevision":1,"secret":"sk-2"})),
        )
        .unwrap();
    assert_eq!(updated.groups[0].token, "sk-2");
    assert!(
        serde_json::from_value::<EntryInput>(serde_json::json!({
            "kind":"password","title":"t","secret":"s","groups":[{"password":"p"}]
        }))
        .is_err()
    );
}

// AI 常见的“多填”写法不应报错：空数据库块、空服务地址、只传 url 的 API 凭据。
#[test]
fn harmless_padding_is_accepted() {
    let mut value: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"api","title":"key","url":"https://api.invalid/v1",
        "database":{"databaseType":"","host":"","port":"","databaseName":""},
        "services":[{"protocol":"custom","url":"  "}],
        "groups":[{"token":"sk-1"}]
    }))
    .unwrap();
    strict().prepare(&mut value).unwrap();
    assert!(value.database.is_none());
    assert_eq!(value.services.len(), 1);
    assert_eq!(value.services[0].url, "https://api.invalid/v1");
    let moved = strict()
        .patch(
            value,
            patch(serde_json::json!({"id":"x","expectedRevision":1,"url":"https://api2.invalid"})),
        )
        .unwrap();
    assert_eq!(moved.services[0].url, "https://api2.invalid");
    assert_eq!(moved.url, "https://api2.invalid");
}

// 条目按网站显示：没有名称时用网站域名（去掉 www. 与路径），其次是 API 地址、数据库、账号。
#[test]
fn entries_are_shown_by_site_then_title() {
    let mut value: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"password","url":"https://user@www.GitHub.com:443/login?x=1",
        "groups":[{"account":"alice","password":"p"}]
    }))
    .unwrap();
    strict().prepare(&mut value).unwrap();
    assert_eq!(value.display_name(), "github.com:443");
    value.title = "公司代码托管".into();
    assert_eq!(
        value.display_name(),
        "github.com:443",
        "the site wins over a title"
    );
    let db: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"database","database":{"databaseType":"MySQL","host":"10.0.0.8","databaseName":"orders"}
    }))
    .unwrap();
    assert_eq!(db.display_name(), "orders @ 10.0.0.8");
    let generic: EntryInput = serde_json::from_value(serde_json::json!({
        "kind":"generic","groups":[{"account":"home-wifi","password":"p"}]
    }))
    .unwrap();
    assert_eq!(generic.display_name(), "home-wifi");
}
