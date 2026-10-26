//! Wire-level tests exercise the public API, not private operation helpers.
use serde_json::{Value, json};
use vault_core::{api::CoreSession, bootstrap};

#[test]
fn parallel_rpc_reads_and_writes_share_one_consistent_repository() {
    let directory = tempfile::tempdir().unwrap();
    let core = bootstrap::open(directory.path(), false).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let core = core.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                for index in 0..15 {
                    let (method, params) = if worker < 4 {
                        ("credentials.create", json!({"kind":"password","title":format!("fixture-{worker}-{index}"),"secret":"FAKE_PARALLEL_ONLY"}))
                    } else {
                        (["credentials.list", "settings.get", "tags.list"][index % 3], json!({}))
                    };
                    let response = call(&core, json!(index), method, params);
                    assert!(response.get("error").is_none(), "{method}: {response}");
                }
            });
        }
    });
    assert_eq!(
        call(&core, json!(99), "credentials.list", json!({}))["result"]
            .as_array()
            .unwrap()
            .len(),
        60
    );
}
fn call(core: &CoreSession, id: Value, method: &str, params: Value) -> Value {
    core.handle_rpc(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        .unwrap()
}
#[test]
fn rpc_preserves_ids_and_distinguishes_protocol_errors() {
    let directory = tempfile::tempdir().unwrap();
    let core = bootstrap::open(directory.path(), false).unwrap();
    let response = call(&core, json!("request-9"), "credentials.list", json!({}));
    assert_eq!(response["id"], "request-9");
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["result"], json!([]));
    assert_eq!(
        call(&core, json!(2), "missing.method", json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        call(&core, json!(2), "credentials.create", json!({}))["error"]["code"],
        -32602
    );
    assert_eq!(core.handle_json("{").unwrap()["error"]["code"], -32700);
    for invalid in [
        json!([]),
        json!({"jsonrpc":"1.0","id":1,"method":"credentials.list"}),
        json!({"jsonrpc":"2.0","id":true,"method":"credentials.list"}),
    ] {
        assert_eq!(core.handle_rpc(invalid).unwrap()["error"]["code"], -32600);
    }
}
#[test]
fn notifications_execute_once_and_batches_omit_notification_results() {
    let directory = tempfile::tempdir().unwrap();
    let core = bootstrap::open(directory.path(), false).unwrap();
    let create = json!({"jsonrpc":"2.0","method":"credentials.create","params":{"kind":"password","title":"notification fixture","secret":"FAKE_RPC_ONLY"}});
    assert!(core.handle_rpc(create).is_none());
    let response = core
        .handle_rpc(json!([
            {"jsonrpc":"2.0","method":"credentials.list"},
            {"jsonrpc":"2.0","id":null,"method":"credentials.list"},
            {"jsonrpc":"2.0","id":"bad","method":"missing"}
        ]))
        .unwrap();
    assert_eq!(response.as_array().unwrap().len(), 2);
    assert_eq!(response[0]["id"], Value::Null);
    assert_eq!(response[0]["result"].as_array().unwrap().len(), 1);
    assert_eq!(response[1]["error"]["code"], -32601);
}
#[test]
fn rpc_identity_is_bound_and_mcp_never_reads_plaintext() {
    let directory = tempfile::tempdir().unwrap();
    let core = bootstrap::open(directory.path(), false).unwrap();
    let mcp = core.mcp_session();
    let saved = call(
        &core,
        json!(1),
        "credentials.create",
        json!({"kind":"password","title":"fixture","secret":"FAKE_RPC_SECRET_429"}),
    )["result"]
        .clone();
    let args = json!({"id":saved["id"],"expectedRevision":saved["revision"]});
    assert_eq!(
        call(&core, json!(2), "credentials.readSecret", args.clone())["result"],
        "FAKE_RPC_SECRET_429"
    );
    for (method, params) in [
        ("credentials.readSecret", args),
        ("trash.empty", json!({})),
        ("settings.get", json!({})),
    ] {
        let error = call(&mcp, json!(3), method, params);
        assert_eq!(error["error"]["data"]["code"], "PERMISSION_DENIED");
        assert!(!error.to_string().contains("FAKE_RPC_SECRET_429"));
    }
    let forged = core
        .handle_rpc(json!({"jsonrpc":"2.0","id":4,"method":"credentials.list","caller":"desktop"}))
        .unwrap();
    assert_eq!(forged["error"]["code"], -32600);
    let metadata = call(&mcp, json!(5), "credentials.get", json!({"id":saved["id"]}));
    assert!(!metadata.to_string().contains("FAKE_RPC_SECRET_429"));
    let mut settings = call(&core, json!(6), "settings.get", json!({}))["result"].clone();
    settings["mcpPermissions"]["create"] = json!(false);
    call(
        &core,
        json!(7),
        "settings.save",
        json!({"settings":settings}),
    );
    assert_eq!(
        call(
            &mcp,
            json!(8),
            "credentials.create",
            json!({"kind":"password","title":"denied","secret":"FAKE_DENIED_ONLY"})
        )["error"]["data"]["code"],
        "PERMISSION_DENIED"
    );
    assert!(
        call(
            &core,
            json!(9),
            "credentials.create",
            json!({"kind":"password","title":"allowed","secret":"FAKE_ALLOWED_ONLY"})
        )
        .get("result")
        .is_some()
    );
}
