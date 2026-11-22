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

// ---- 方案 2：密码、token 只由 vault 自己注入 ----

use crate::command::{CommandSpec, injector};
use crate::inject::Injector;
use base64::Engine;
use std::collections::BTreeMap;

fn execute(injector: Injector<'_>, spec: CommandSpec) -> Result<serde_json::Value> {
    crate::command::execute(injector, spec, None)
}

/// One group with a synthetic password and token.
struct SiteCredential;
impl CredentialReader for SiteCredential {
    fn get(&self, _: &str) -> Result<EntryDetail> {
        let summary = serde_json::from_value(serde_json::json!({
            "id":"site", "kind":"password", "title":"Site",
            "url":"", "notes":"", "pinned":false, "createdAt":0, "updatedAt":0,
            "revision":1, "services":[]
        }))
        .unwrap();
        Ok(EntryDetail {
            summary,
            groups: vec![group("prod", "p@ss \"w0rd\"/+", "tok-123456")],
        })
    }
}

/// A shell line run by the platform's own shell.
fn shell(line: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/d".into(), "/c".into(), line.into()]
    } else {
        vec!["sh".into(), "-c".into(), line.into()]
    }
}

fn spec(command: Vec<String>) -> CommandSpec {
    CommandSpec {
        command,
        cwd: None,
        env: BTreeMap::new(),
        stdin: None,
        timeout_seconds: None,
    }
}

#[test]
fn placeholders_render_with_filters() {
    let entry = SiteCredential.get("site").unwrap();
    let mut injector = Injector::new(&entry, None);
    let body = injector
        .render(r#"{"user":"{{account}}","pass":"{{password|json}}","x":"{{other}}"}"#)
        .unwrap();
    assert_eq!(
        body.as_str(),
        r#"{"user":"prod-user","pass":"p@ss \"w0rd\"/+","x":"{{other}}"}"#
    );
    assert!(!injector.render("{{basic}}").unwrap().contains("p@ss"));
    assert!(
        !injector
            .render("{{password|url}}")
            .unwrap()
            .contains("p@ss")
    );
    assert_eq!(
        injector.render("{{password|rot13}}").unwrap_err().code,
        "INVALID_INPUT"
    );
    // {{credential}} 只在写入给了 field 时可用。
    assert_eq!(
        injector.render("{{credential}}").unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn output_masks_every_form_of_a_secret() {
    let entry = SiteCredential.get("site").unwrap();
    let mut injector = Injector::new(&entry, None);
    injector.prime();
    let secret = "p@ss \"w0rd\"/+";
    let b64 = base64::engine::general_purpose::STANDARD.encode(secret);
    let hex: String = secret.bytes().map(|b| format!("{b:02x}")).collect();
    let echoed = format!(
        "{secret} {b64} {hex} {} p%40ss%20%22w0rd%22%2F%2B prod-user tok-123456",
        hex.to_uppercase()
    );
    let masked = injector.mask_output(echoed.as_bytes(), false);
    assert_eq!(masked, "*** *** *** *** *** prod-user ***");
    // 截断在半个密码处：尾巴丢掉，不漏出前缀。
    let cut = injector.mask_output(b"done tok-12", true);
    assert_eq!(cut, "done ");
}

#[test]
fn command_gets_secret_in_env_args_and_stdin_and_output_is_masked() {
    let entry = SiteCredential.get("site").unwrap();
    // 占位符可以写在参数里（这里是整行命令）、环境变量值里和标准输入里。
    let command = shell(if cfg!(windows) {
        "echo env=%VAULT_T% arg={{token}} & more"
    } else {
        "echo env=$VAULT_T arg={{token}}; cat"
    });
    let mut spec = spec(command);
    spec.env.insert("VAULT_T".into(), "{{token}}".into());
    spec.stdin = Some("in={{token}} who={{account}}\n".into());
    let result = execute(injector(&entry, None, None, false), spec).unwrap();
    assert_eq!(result["status"], "command_finished");
    assert_eq!(result["exitCode"], 0);
    let stdout = result["stdout"].as_str().unwrap();
    assert!(!stdout.contains("tok-123456"), "{stdout}");
    assert!(stdout.contains("env=*** arg=***"), "{stdout}");
    assert!(stdout.contains("in=*** who=prod-user"), "{stdout}");
}

#[test]
fn command_credential_placeholder_uses_the_field() {
    let entry = SiteCredential.get("site").unwrap();
    let mut spec = spec(shell(if cfg!(windows) {
        "echo [%VAULT_C%]"
    } else {
        "echo [$VAULT_C]"
    }));
    spec.env.insert("VAULT_C".into(), "{{credential}}".into());
    let value = zeroize::Zeroizing::new("tok-123456".to_string());
    let result = execute(injector(&entry, None, Some(value), true), spec).unwrap();
    assert!(result["stdout"].as_str().unwrap().contains("[***]"));
}

#[test]
fn command_is_stopped_at_the_timeout() {
    let entry = SiteCredential.get("site").unwrap();
    let command = if cfg!(windows) {
        vec!["ping".into(), "-n".into(), "30".into(), "127.0.0.1".into()]
    } else {
        vec!["sleep".into(), "30".into()]
    };
    let mut spec = spec(command);
    spec.timeout_seconds = Some(1);
    let result = execute(injector(&entry, None, None, false), spec).unwrap();
    assert_eq!(result["timedOut"], true);
    assert!(result["elapsedMs"].as_u64().unwrap() < 10_000);
}

#[test]
fn command_input_is_checked() {
    let entry = SiteCredential.get("site").unwrap();
    let missing = execute(
        injector(&entry, None, None, false),
        spec(vec!["no-such-program-vault-test".into()]),
    )
    .unwrap_err();
    assert_eq!(missing.code, "COMMAND_NOT_FOUND");
    let empty = execute(injector(&entry, None, None, false), spec(vec![])).unwrap_err();
    assert_eq!(empty.code, "INVALID_INPUT");
    let mut relative = spec(shell("echo x"));
    relative.cwd = Some("relative/dir".into());
    assert_eq!(
        execute(injector(&entry, None, None, false), relative)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    let mut bad_env = spec(shell("echo x"));
    bad_env.env.insert("A=B".into(), "x".into());
    assert_eq!(
        execute(injector(&entry, None, None, false), bad_env)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}

/// Records what the command step does with an askpass channel.
struct FakeAskpass {
    bound: std::sync::Arc<std::sync::atomic::AtomicU32>,
}
impl vault_contracts::filling::AskpassSession for FakeAskpass {
    fn env(&self) -> Vec<(String, String)> {
        vec![
            ("VAULT_FAKE_ASKPASS".into(), "from-vault".into()),
            ("VAULT_CALLER_WINS".into(), "from-vault".into()),
        ]
    }
    fn bind(&self, process_id: u32) {
        self.bound
            .store(process_id, std::sync::atomic::Ordering::SeqCst);
    }
    fn answered(&self) -> usize {
        2
    }
}

#[test]
fn command_attaches_askpass_and_reports_answers() {
    let entry = SiteCredential.get("site").unwrap();
    let bound = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let mut spec = spec(shell(if cfg!(windows) {
        "echo [%VAULT_FAKE_ASKPASS%] [%VAULT_CALLER_WINS%]"
    } else {
        "echo [$VAULT_FAKE_ASKPASS] [$VAULT_CALLER_WINS]"
    }));
    spec.env
        .insert("VAULT_CALLER_WINS".into(), "from-caller".into());
    let result = crate::command::execute(
        injector(&entry, None, None, false),
        spec,
        Some(Box::new(FakeAskpass {
            bound: bound.clone(),
        })),
    )
    .unwrap();
    let stdout = result["stdout"].as_str().unwrap();
    assert!(stdout.contains("[from-vault] [from-caller]"), "{stdout}");
    assert_ne!(bound.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(result["askpassAnswered"], 2);
    // 没有 askpass 时不出现这个字段。
    let plain = execute(injector(&entry, None, None, false), spec_echo()).unwrap();
    assert!(plain.get("askpassAnswered").is_none());
}

fn spec_echo() -> CommandSpec {
    spec(shell("echo x"))
}
