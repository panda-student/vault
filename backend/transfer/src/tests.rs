use super::*;
use vault_contracts::credentials::{ApiProtocol, ApiService, DatabaseFields, Kind};

fn password() -> EntryInput {
    EntryInput {
        kind: Kind::Password,
        title: "公司邮箱".into(),
        tags: vec!["工作".into(), "常用".into()],
        database: None,
        account: "alice@example.com".into(),
        url: "https://mail.example.com".into(),
        documentation_url: "https://docs.example.com/mail".into(),
        services: vec![],
        secret: "p@ss,\"word\"\n第二行".into(),
        notes: "备注包含逗号, 引号\" 和换行\r\n下一行".into(),
    }
}

fn api() -> EntryInput {
    EntryInput {
        kind: Kind::Api,
        title: "OpenAI".into(),
        tags: vec!["ai".into()],
        database: None,
        account: "team".into(),
        url: "https://api.openai.com/v1".into(),
        documentation_url: "https://platform.openai.com/docs".into(),
        services: vec![
            ApiService {
                id: "svc-1".into(),
                protocol: ApiProtocol::Openai,
                url: "https://api.openai.com/v1".into(),
            },
            ApiService {
                id: "svc-2".into(),
                protocol: ApiProtocol::Custom,
                url: "https://gateway.example.com/v1".into(),
            },
        ],
        secret: "sk-test-123".into(),
        notes: String::new(),
    }
}

fn database() -> EntryInput {
    EntryInput {
        kind: Kind::Database,
        title: "订单库".into(),
        tags: vec![],
        database: Some(DatabaseFields {
            database_type: "MySQL".into(),
            host: "10.0.0.8".into(),
            port: "3306".into(),
            database_name: "orders".into(),
        }),
        account: "readonly".into(),
        url: String::new(),
        documentation_url: String::new(),
        services: vec![],
        secret: "db-secret".into(),
        notes: "仅限内网".into(),
    }
}

fn generic() -> EntryInput {
    EntryInput {
        kind: Kind::Generic,
        title: "Wi-Fi 密码".into(),
        tags: vec!["家庭".into()],
        database: None,
        account: String::new(),
        url: String::new(),
        documentation_url: String::new(),
        services: vec![],
        secret: "wifi-secret".into(),
        notes: String::new(),
    }
}

fn all() -> Vec<EntryInput> {
    vec![password(), api(), database(), generic()]
}

fn value(entry: &EntryInput) -> serde_json::Value {
    serde_json::to_value(entry).unwrap()
}

fn reason(error: &AppError) -> String {
    error.details.as_ref().unwrap()[0].reason.clone()
}

fn cheap() -> Kdf {
    Kdf {
        m_cost: 8192,
        t_cost: 1,
        p_cost: 1,
    }
}

#[test]
fn json_export_roundtrips_every_field() {
    let source = all();
    let bytes = encode(Format::Json, &source).unwrap();
    let decoded = decode(&bytes, None).unwrap();
    assert_eq!(decoded.format, Format::Json);
    assert_eq!(decoded.entries.len(), source.len());
    for (index, imported) in decoded.entries.iter().enumerate() {
        assert_eq!(imported.row, index + 1);
        assert_eq!(value(&imported.value), value(&source[index]));
    }
}

#[test]
fn json_export_declares_format_and_version() {
    let bytes = encode(Format::Json, &all()).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(envelope["format"], "vault-export");
    assert_eq!(envelope["version"], 1);
    assert!(envelope["exportedAt"].as_i64().unwrap() > 0);
    assert_eq!(envelope["entries"].as_array().unwrap().len(), 4);
}

#[test]
fn json_export_rejects_unknown_version() {
    let bytes = encode(Format::Json, &all()).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    envelope["version"] = 2.into();
    let error = decode(&serde_json::to_vec(&envelope).unwrap(), None).unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED_EXPORT_VERSION");
}

#[test]
fn json_export_rejects_broken_document() {
    let error = decode(br#"{"format":"vault-export","version":1"#, None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
}

#[test]
fn json_export_accepts_empty_vault() {
    let bytes = encode(Format::Json, &[]).unwrap();
    let decoded = decode(&bytes, None).unwrap();
    assert!(decoded.entries.is_empty());
}

#[test]
fn csv_export_roundtrips_every_field() {
    let source = all();
    let bytes = encode(Format::Csv, &source).unwrap();
    let decoded = decode(&bytes, None).unwrap();
    assert_eq!(decoded.format, Format::Csv);
    assert_eq!(decoded.entries.len(), source.len());
    for (index, imported) in decoded.entries.iter().enumerate() {
        assert_eq!(imported.row, index + 2);
        assert_eq!(value(&imported.value), value(&source[index]));
    }
}

#[test]
fn csv_export_starts_with_chrome_columns_then_extra_fields() {
    let bytes = encode(Format::Csv, &all()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let header = text.lines().next().unwrap();
    assert_eq!(
        header,
        "name,url,username,password,note,type,tags,services,documentation_url,database_type,host,port,database_name"
    );
}

#[test]
fn csv_import_accepts_chrome_export_rows() {
    let bytes = b"name,url,username,password,note\r\nGitHub,https://github.com,alice,s3cret,\"line1\r\nline2\"\r\n";
    let decoded = decode(bytes, None).unwrap();
    assert_eq!(decoded.format, Format::Csv);
    assert_eq!(decoded.entries.len(), 1);
    let entry = &decoded.entries[0];
    assert_eq!(entry.row, 2);
    assert_eq!(entry.value.kind, Kind::Password);
    assert_eq!(entry.value.title, "GitHub");
    assert_eq!(entry.value.url, "https://github.com");
    assert_eq!(entry.value.account, "alice");
    assert_eq!(entry.value.secret, "s3cret");
    assert_eq!(entry.value.notes, "line1\r\nline2");
    assert!(entry.value.tags.is_empty());
    assert!(entry.value.services.is_empty());
    assert!(entry.value.database.is_none());
}

#[test]
fn csv_import_ignores_unknown_columns() {
    let bytes = b"name,url,username,password,note,extra\r\nExample,https://example.com,bob,pw,note,ignored\r\n";
    let decoded = decode(bytes, None).unwrap();
    assert_eq!(decoded.entries[0].value.title, "Example");
    assert_eq!(decoded.entries[0].value.secret, "pw");
}

#[test]
fn csv_import_reports_physical_row_number() {
    let bytes =
        b"name,url,username,password,note\r\ngood,https://example.com,a,b,\r\nbroken,only-two\r\n";
    let error = decode(bytes, None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
    assert!(reason(&error).contains("第 3 行"), "{:?}", error.details);
}

#[test]
fn csv_import_reports_invalid_kind_row() {
    let bytes = b"name,url,username,password,note,type\r\nX,,,,,telepathy\r\n";
    let error = decode(bytes, None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
    assert!(reason(&error).contains("第 2 行"));
}

#[test]
fn csv_import_rejects_unclosed_quote() {
    let bytes = b"name,url,username,password,note\r\nX,https://x,a,b,\"unterminated\r\n";
    let error = decode(bytes, None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
    assert!(reason(&error).contains("第 2 行"));
}

#[test]
fn csv_import_accepts_bom_and_lf_line_endings() {
    let bytes = b"\xEF\xBB\xBFname,url,username,password,note\nX,https://x,a,b,\n";
    let decoded = decode(bytes, None).unwrap();
    assert_eq!(decoded.entries[0].value.title, "X");
}

#[test]
fn csv_import_rejects_missing_name_column() {
    let bytes = b"url,username,password\r\nhttps://x,a,b\r\n";
    let error = decode(bytes, None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
    assert!(reason(&error).contains("name"));
}

#[test]
fn encrypted_json_roundtrip_requires_passphrase() {
    let source = all();
    let bytes = encode_encrypted_with(Format::Json, &source, "correct horse", cheap()).unwrap();
    let inspection = inspect(&bytes).unwrap();
    assert!(inspection.encrypted);
    assert_eq!(inspection.format, Format::Json);
    let missing = decode(&bytes, None).unwrap_err();
    assert_eq!(missing.code, "PASSPHRASE_REQUIRED");
    let decoded = decode(&bytes, Some("correct horse")).unwrap();
    assert_eq!(decoded.format, Format::Json);
    assert_eq!(decoded.entries.len(), source.len());
    assert_eq!(value(&decoded.entries[0].value), value(&source[0]));
}

#[test]
fn encrypted_csv_roundtrip() {
    let source = all();
    let bytes = encode_encrypted_with(Format::Csv, &source, "口令 passphrase", cheap()).unwrap();
    let decoded = decode(&bytes, Some("口令 passphrase")).unwrap();
    assert_eq!(decoded.format, Format::Csv);
    assert_eq!(decoded.entries.len(), source.len());
    assert_eq!(value(&decoded.entries[2].value), value(&source[2]));
}

#[test]
fn encrypted_export_rejects_empty_passphrase() {
    let error = encode_encrypted(Format::Json, &all(), "").unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
}

#[test]
fn encrypted_import_rejects_wrong_passphrase() {
    let bytes = encode_encrypted_with(Format::Json, &all(), "right", cheap()).unwrap();
    let error = decode(&bytes, Some("wrong")).unwrap_err();
    assert_eq!(error.code, "DECRYPT_FAILED");
}

#[test]
fn encrypted_import_rejects_tampered_ciphertext() {
    let mut bytes = encode_encrypted_with(Format::Json, &all(), "right", cheap()).unwrap();
    if let Some(last) = bytes.last_mut() {
        *last ^= 1;
    }
    let error = decode(&bytes, Some("right")).unwrap_err();
    assert_eq!(error.code, "DECRYPT_FAILED");
}

#[test]
fn encrypted_import_rejects_tampered_header() {
    let mut bytes = encode_encrypted_with(Format::Json, &all(), "right", cheap()).unwrap();
    bytes[24] ^= 1; // salt is authenticated as AAD
    let error = decode(&bytes, Some("right")).unwrap_err();
    assert_eq!(error.code, "DECRYPT_FAILED");
}

#[test]
fn inspect_distinguishes_plain_formats() {
    let json = encode(Format::Json, &all()).unwrap();
    assert_eq!(
        inspect(&json).unwrap(),
        Inspection {
            encrypted: false,
            format: Format::Json
        }
    );
    let csv = encode(Format::Csv, &all()).unwrap();
    assert_eq!(
        inspect(&csv).unwrap(),
        Inspection {
            encrypted: false,
            format: Format::Csv
        }
    );
}

#[test]
fn decode_rejects_empty_file() {
    let error = decode(b"", None).unwrap_err();
    assert_eq!(error.code, "INVALID_EXPORT");
}

#[test]
fn export_rejects_entry_count_over_limit() {
    let entries: Vec<EntryInput> = (0..=MAX_ENTRIES).map(|_| generic()).collect();
    let error = encode(Format::Json, &entries).unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
}

#[test]
fn encrypted_container_uses_versioned_magic() {
    let bytes = encode_encrypted_with(Format::Json, &[], "pw", cheap()).unwrap();
    assert!(bytes.starts_with(b"VAULTEXP"));
    assert_eq!(bytes[8], 1);
    assert_eq!(bytes[9], Format::Json.code());
}
