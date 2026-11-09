//! MCP descriptions and schemas. Execution stays in the adapter and shared API.
//!
//! Schemas stay inside the JSON Schema subset every mainstream MCP client and model
//! API accepts: a root `type: object`, plain `properties` / `required` / `enum` /
//! `items`, and no `oneOf` / `anyOf` / `allOf` / `const` / `default`. Clients that
//! cannot translate richer constructs silently drop them, and the model then
//! guesses the shape, which is what used to produce invalid-parameter errors.
use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Value, json};
use std::sync::Arc;

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

/// Remote sync settings. One flat target shape with `kind` instead of a union,
/// so every client keeps the constraints.
fn sync_patch() -> Value {
    let credential = |description: &str| {
        let mut reference = object(
            json!({
                "credentialId": text("Id of a vault entry from search_entries; its secret is used locally and never returned"),
                "groupId": text("groups[].id; required when the entry has more than one credential group")
            }),
            &["credentialId"],
        );
        reference["description"] = json!(description);
        reference
    };
    let mut sync = object(
        json!({
            "enabled": {"type": "boolean", "description": "false stops syncing (nothing is deleted locally or remotely); pass it alone"},
            "target": object(
                json!({
                    "kind": {
                        "type": "string",
                        "enum": ["folder", "webdav", "github"],
                        "description": "folder = a local or cloud-drive folder; webdav = a WebDAV server; github = a private GitHub repository"
                    },
                    "directory": text("kind=folder: absolute folder path"),
                    "url": text("kind=webdav: https base URL of the WebDAV folder, without credentials"),
                    "credential": credential("kind=webdav: entry whose group holds the account and password"),
                    "owner": text("kind=github: user or organization that owns the repository"),
                    "repo": text("kind=github: repository name; it must be private"),
                    "branch": text("kind=github: branch; the repository's default branch when omitted"),
                    "token": credential("kind=github: entry whose group holds a token with contents read/write (and administration to create the repository)"),
                    "path": text("File path inside the target; vault/vault-sync.vaultsync when omitted. Only this one file is ever read or written")
                }),
                &["kind"],
            ),
            "create": {"type": "boolean", "description": "Create the missing folder, WebDAV collection or private GitHub repository"},
            "intervalMinutes": {"type": "integer", "minimum": 0, "maximum": 1440, "description": "Minutes between automatic syncs; 0 = only after edits and on demand"},
            "deviceName": {"type": "string", "maxLength": 64, "description": "Name of this computer, shown when resolving conflicts"}
        }),
        &[],
    );
    sync["description"] = json!(
        "Remote sync. Pass target to set or change where the encrypted sync file lives (the desktop user confirms and types the passphrase); pass only intervalMinutes/deviceName to adjust; pass enabled=false to stop."
    );
    sync
}

/// Batch tools take `items` only; one entry is simply a one-item list.
fn batch(item: Value) -> Value {
    object(
        json!({
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": 100,
                "description": "1-100 items; applied atomically and answered in input order.",
                "items": item
            }
        }),
        &["items"],
    )
}

fn revision() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "description": "Current revision from search_entries/get_entries; stale revisions are rejected"
    })
}

fn kind() -> Value {
    json!({
        "type": "string",
        "enum": ["password", "api", "database", "generic"],
        "description": "password = website login; api = API service whose address is in services; database = database connection; generic = anything else"
    })
}

fn group(update: bool) -> Value {
    let mut properties = json!({
        "environment": text("Optional environment label, e.g. production or test"),
        "account": text("Username, email or account id; optional"),
        "password": text("Login password; optional"),
        "token": text("API key or access token; optional")
    });
    let description = if update {
        properties["id"] =
            text("groups[].id from get_entries to change that group; omit it to add a new group");
        "One credential group. With id, only the fields you pass change; without id, a new group is added."
    } else {
        "One credential group: an environment's account with a password and/or token."
    };
    let mut schema = object(properties, &[]);
    schema["description"] = json!(description);
    schema
}

fn entry_fields() -> Value {
    json!({
        "title": text("Optional name, shown only for entries without a website; entries are listed by their website host"),
        "url": text("Website address; required for kind=password. For kind=api it mirrors the first services address."),
        "services": {
            "type": "array",
            "maxItems": 20,
            "description": "API addresses used with the tokens; required (at least one) for kind=api, optional for password/generic.",
            "items": object(json!({
                "id": text("Existing service id to keep on updates; omit for new addresses"),
                "protocol": {
                    "type": "string",
                    "enum": ["openai", "anthropic", "gemini", "custom"],
                    "description": "openai/anthropic/gemini for vendor-specific APIs; custom for any other HTTP(S) endpoint"
                },
                "url": text("Base URL")
            }), &["protocol", "url"])
        },
        "documentationUrl": text("Documentation website, optional and separate from API addresses"),
        "tags": {"type": "array", "maxItems": 64, "items": {"type": "string"}},
        "notes": text("Notes, optional"),
        "database": object(json!({
            "databaseType": text("e.g. MySQL, PostgreSQL, Redis"),
            "host": text("Host name or address"),
            "port": text("Port number as text, e.g. 3306"),
            "databaseName": text("Database name, service name or file path")
        }), &["databaseType"])
    })
}

fn create_item() -> Value {
    let mut properties = entry_fields();
    properties["kind"] = kind();
    properties["database"]["description"] = json!("Only for kind=database");
    properties["groups"] = json!({
        "type": "array",
        "minItems": 1,
        "maxItems": 50,
        "description": "Credential groups of this site, e.g. one per environment. kind=password and kind=api need at least one group with a password or token.",
        "items": group(false)
    });
    object(properties, &["kind", "groups"])
}

fn update_item() -> Value {
    let mut properties = entry_fields();
    properties["id"] = text("Entry id from search_entries");
    properties["expectedRevision"] = revision();
    properties["groups"] = json!({
        "type": "array",
        "maxItems": 50,
        "description": "Groups to change (with id) or add (without id). Groups you do not list stay unchanged.",
        "items": group(true)
    });
    properties["removeGroups"] = json!({
        "type": "array",
        "maxItems": 50,
        "description": "groups[].id values to delete from this entry",
        "items": {"type": "string"}
    });
    object(properties, &["id", "expectedRevision"])
}

fn version_item() -> Value {
    object(
        json!({"id": text("Entry id"), "expectedRevision": revision()}),
        &["id", "expectedRevision"],
    )
}

fn tool(name: &'static str, description: &'static str, schema: Value) -> Tool {
    Tool::new(
        name,
        description,
        Arc::new(
            schema
                .as_object()
                .expect("tool schemas are objects")
                .clone(),
        ),
    )
}

fn annotate(tool: Tool, read_only: bool, destructive: bool) -> Tool {
    let annotations = ToolAnnotations::new()
        .read_only(read_only)
        .destructive(destructive)
        .open_world(false);
    tool.with_annotations(annotations)
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        annotate(
            tool(
                "get_mcp_config",
                "Get this running program's stdio MCP connection configuration, using its actual executable and current vault directory. Returns launch paths only, never credentials or other clients' configuration.",
                object(json!({}), &[]),
            ),
            true,
            false,
        ),
        annotate(
            tool(
                "search_entries",
                "Search controlled non-sensitive metadata. Never returns notes, passwords or tokens. Each entry has a display name (its website host, else its title) and lists its credential groups (id, environment, account, hasPassword, hasToken). siteMissing=true means the site is unknown: ask the user, never guess.",
                batch(object(
                    json!({
                        "query": text("Matches title, website, API addresses, tags, database fields, accounts and environments; empty lists everything"),
                        "tag": text("Only entries with exactly this tag"),
                        "kind": kind()
                    }),
                    &[],
                )),
            ),
            true,
            false,
        ),
        annotate(
            tool(
                "get_entries",
                "Read controlled non-sensitive metadata of entries by id, including credential groups (id, environment, account, hasPassword, hasToken). Never returns notes, passwords or tokens; use write_credential to use them locally. siteMissing=true means the site is unknown: ask the user, never guess.",
                batch(object(
                    json!({"id": text("Entry id from search_entries")}),
                    &["id"],
                )),
            ),
            true,
            false,
        ),
        annotate(
            tool(
                "create_entries",
                "Create entries. One entry is one website or service and can hold several credential groups (e.g. production and test accounts), each with a password and/or token. Returns only id, revision and status; never echoes stored fields or secrets.",
                batch(create_item()),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "update_entries",
                "Update entries. Only the fields you pass change; omitted passwords and tokens are kept. Change a group by its id, add one without an id, delete groups with removeGroups. The kind cannot change. An entry with siteMissing=true must receive url (password) or services (api) in the same update.",
                batch(update_item()),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "delete_entries",
                "Move entries to trash by id and expectedRevision. Deleted entries cannot be searched, read or used until restore_entries; permanent deletion is never available to MCP. Returns ids only.",
                batch(version_item()),
            ),
            false,
            true,
        ),
        annotate(
            tool(
                "restore_entries",
                "Restore one entry from trash by id and expectedRevision. Requires update permission for the entry's tags. Returns controlled metadata only, never secrets or notes.",
                version_item(),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "pin_entries",
                "Pin or unpin one entry by id and expectedRevision. Returns controlled metadata only, never secrets or notes.",
                object(
                    json!({
                        "id": text("Entry id"),
                        "expectedRevision": revision(),
                        "pinned": {"type": "boolean"}
                    }),
                    &["id", "expectedRevision", "pinned"],
                ),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "organize",
                "Rename or delete one tag globally across entries, trash and the tag's MCP permission rules. Pass previous to rename it to name; pass previous with an empty name to delete it. Requires default update permission plus update permission on every affected entry; rejected when the change would grant MCP more permissions than before.",
                object(
                    json!({
                        "kind": {"type": "string", "enum": ["tag"], "description": "Always tag"},
                        "name": text("New tag name; empty string with previous set deletes the tag"),
                        "previous": text("Existing tag to rename or delete")
                    }),
                    &["kind", "name"],
                ),
            ),
            false,
            true,
        ),
        annotate(
            tool(
                "write_credential",
                "Use a stored value locally without returning plaintext. target.type=input fills a field captured with capture_fill_target (needs targetId). target.type=file writes to an absolute path: mode=create (default) makes a new UTF-8 file, optionally from a template with exactly one {{credential}} marker; mode=replace replaces exactly one placeholder in an existing UTF-8 file and keeps the rest. It never overwrites a whole existing file, opens apps or navigates. input_sent means keyboard events were sent; file_written means the file was written. Never read the resulting sensitive file.",
                object(
                    json!({
                        "credentialId": text("Entry id"),
                        "field": {
                            "type": "string",
                            "enum": ["password", "token", "account", "environment", "secret", "url", "service_url", "host", "port", "database_name", "database_type"],
                            "description": "password/token/account/environment come from one credential group; secret means the password, or the token when the group has none"
                        },
                        "groupId": text("groups[].id; required when the entry has more than one credential group"),
                        "serviceId": text("services[].id for field=service_url when the entry has several addresses"),
                        "target": object(json!({
                            "type": {"type": "string", "enum": ["input", "file"]},
                            "targetId": text("For type=input: id returned by capture_fill_target"),
                            "path": text("For type=file: absolute destination path"),
                            "mode": {"type": "string", "enum": ["create", "replace"], "description": "For type=file; create when omitted"},
                            "template": text("For type=file with mode=create; default {{credential}}"),
                            "placeholder": text("For type=file: marker that must occur exactly once; default {{credential}}")
                        }), &["type"])
                    }),
                    &["credentialId", "field", "target"],
                ),
            ),
            false,
            true,
        ),
        annotate(
            tool(
                "export_entries",
                "Export an encrypted vault file through the desktop app. The desktop user reviews the request and types the passphrase; this tool never receives the passphrase, target path, plaintext or ciphertext, and returns status plus entry count only. Encryption is always applied. Requires write_file permission and an open desktop window; ask again later if the user does not respond within two minutes.",
                object(
                    json!({
                        "includeTrash": {"type": "boolean", "description": "Also export trashed entries; false when omitted"}
                    }),
                    &[],
                ),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "get_settings",
                "Read this program's settings: closeToTray, startupEnabled, the MCP permission defaults, tag rules and entry rules, version, data directory and `sync` (remote sync status: target, interval, device name, last result and error, number of conflicts waiting; never the passphrase or token). `writable` lists the fields update_settings can change; permission rules are read-only and only the desktop user can change them. Requires the settings permission.",
                object(json!({}), &[]),
            ),
            true,
            false,
        ),
        annotate(
            tool(
                "update_settings",
                "Change non-permission settings. Pass only the fields to change; at least one. Applied all-or-nothing and returns the updated settings like get_settings. Change `sync` on its own, not together with other fields. Setting or changing sync.target waits up to two minutes while the desktop user reviews the location and types the sync passphrase; the desktop app must be open. Interval and device name change without asking. Permission rules cannot be changed here (SETTINGS_READ_ONLY): ask the user to change them on the desktop. Requires the settings permission.",
                object(
                    json!({
                        "closeToTray": {"type": "boolean", "description": "Keep running in the system tray when the window is closed"},
                        "startupEnabled": {"type": "boolean", "description": "Start the program when the user signs in to Windows"},
                        "sync": sync_patch()
                    }),
                    &[],
                ),
            ),
            false,
            false,
        ),
        annotate(
            tool(
                "sync_now",
                "Run one remote sync now and return the sync status like get_settings.sync. Entries changed on both sides are left for the desktop user to settle; the result only counts them. Requires the settings permission and a configured sync target.",
                object(json!({}), &[]),
            ),
            false,
            false,
        ),
        tool(
            "capture_fill_target",
            "Capture a Windows input target. For Edge/Chrome pass windowTitle (literal substring), fieldName (exact accessible label) and expectedPassword=true for password fields. The unique named editable control is focused locally; ambiguous, readonly or mismatched targets are rejected. Optional processId narrows the window. Empty arguments capture the current native caret; DOM focus alone is insufficient. No field values are read. Returns a one-use reference valid for 30 seconds.",
            object(
                json!({
                    "windowTitle": {"type": "string", "maxLength": 512},
                    "processId": {"type": "integer", "minimum": 1},
                    "fieldName": {"type": "string", "maxLength": 256},
                    "expectedPassword": {"type": "boolean"}
                }),
                &[],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(value: &Value, path: &str, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    if ["oneOf", "anyOf", "allOf", "const", "default", "$ref"]
                        .contains(&key.as_str())
                    {
                        found.push(format!("{path}/{key}"));
                    }
                    walk(item, &format!("{path}/{key}"), found);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, path, found)),
            _ => {}
        }
    }

    // 工具参数只能使用所有主流客户端都支持的 JSON Schema 子集，否则客户端会丢弃约束，
    // 模型只能猜格式，这正是“参数错误”的来源。
    #[test]
    fn schemas_stay_in_the_portable_subset() {
        for tool in tools() {
            let schema = Value::Object((*tool.input_schema).clone());
            assert_eq!(schema["type"], "object", "{}", tool.name);
            let mut found = Vec::new();
            walk(&schema, &tool.name, &mut found);
            assert!(found.is_empty(), "unsupported keywords: {found:?}");
            let properties = schema["properties"].as_object().unwrap();
            for required in schema["required"].as_array().into_iter().flatten() {
                assert!(
                    properties.contains_key(required.as_str().unwrap()),
                    "{}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn batch_tools_declare_only_items() {
        let catalog = tools();
        for name in [
            "search_entries",
            "get_entries",
            "create_entries",
            "update_entries",
            "delete_entries",
        ] {
            let tool = catalog.iter().find(|tool| tool.name == name).unwrap();
            let schema = &tool.input_schema;
            assert_eq!(schema["properties"].as_object().unwrap().len(), 1, "{name}");
            assert_eq!(schema["required"], json!(["items"]), "{name}");
            let items = &schema["properties"]["items"];
            assert_eq!(items["type"], "array", "{name}");
            assert_eq!(items["minItems"], 1, "{name}");
            assert_eq!(items["maxItems"], 100, "{name}");
            assert_eq!(items["items"]["type"], "object", "{name}");
        }
    }
}
