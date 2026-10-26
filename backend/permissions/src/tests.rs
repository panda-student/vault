use super::*;
#[test]
fn denying_tag_wins_and_explicit_entry_rule_can_override_it() {
    let mut settings = Settings::default();
    settings.mcp_tag_rules.insert(
        "production".into(),
        serde_json::from_value(json!({"writeFile":false})).unwrap(),
    );
    assert!(!permitted(
        &settings,
        Some("fixture"),
        &["production".into()],
        Access::File
    ));
    settings.mcp_entry_rules.insert(
        "fixture".into(),
        serde_json::from_value(json!({"writeFile":true})).unwrap(),
    );
    assert!(permitted(
        &settings,
        Some("fixture"),
        &["production".into()],
        Access::File
    ));
    settings.mcp_enabled = false;
    assert!(!permitted(&settings, Some("fixture"), &[], Access::File));
}
#[test]
fn desktop_only_capability_cannot_be_granted_by_any_rule() {
    let settings = Settings::default();
    assert!(!permitted(&settings, None, &[], Access::DesktopOnly));
}
