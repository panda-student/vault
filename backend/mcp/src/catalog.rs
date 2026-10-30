//! MCP descriptions and schemas. Execution stays in the adapter and shared API.
use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Value, json};
use std::sync::Arc;
pub(super) fn tools() -> Vec<Tool> {
    let property = |name: &str| {
        json!({
            "type": "string",
            "description": name
        })
    };
    let mut fields = json!({
        "kind": {
            "type": "string",
            "enum": [
                "password",
                "api",
                "database",
                "generic"
            ]
        },
        "title": property("Entry name"),
        "account": property("Account or environment, optional"),
        "url": property("Website URL; required for kind=password, optional for generic. For kind=api it is derived from the first service address."),
        "secret": property("Password, API key or token"),
        "notes": property("Notes, optional")
    });
    fields["documentationUrl"] =
        property("Documentation website URL, optional and separate from API request URLs");
    fields["services"] = json!({
        "type": "array",
        "maxItems": 20,
        "description": "Service addresses sharing this credential's token; required with at least one address for kind=api (url is derived from the first address).",
        "items": {
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Keep existing service id on updates; omit for new addresses"
                },
                "protocol": {
                    "type": "string",
                    "description": "openai/anthropic/gemini for vendor-specific APIs; custom for a standard HTTP/HTTPS endpoint.",
                    "enum": [
                        "openai",
                        "anthropic",
                        "gemini",
                        "custom"
                    ]
                },
                "url": {
                    "type": "string"
                }
            },
            "required": [
                "protocol",
                "url"
            ],
            "additionalProperties": false
        }
    });
    fields["tags"] = json!({
        "type": "array",
        "maxItems": 64,
        "items": {
            "type": "string"
        }
    });
    fields["database"] = json!({
        "type": "object",
        "properties": {
            "databaseType": {
                "type": "string"
            },
            "host": {
                "type": "string"
            },
            "port": {
                "type": "string"
            },
            "databaseName": {
                "type": "string"
            }
        },
        "required": [
            "databaseType"
        ],
        "additionalProperties": false
    });
    let create=Tool::new("create_entries","Create a password or API credential. A password requires url (its website); an API credential requires at least one services address. Returns only id, revision and status; never echoes stored fields or secrets.",json!({
    "type": "object",
    "properties": fields,
    "required": [
        "kind",
        "title",
        "secret"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(false).open_world(false));
    fields.as_object_mut().unwrap().remove("kind");
    fields["id"] = property("Entry ID from search_entries");
    fields["expectedRevision"] = json!({
        "type": "integer",
        "minimum": 1,
        "description": "Current revision; stale revisions are rejected"
    });
    let mut result = vec![
        Tool::new("get_mcp_config","Get this running program's stdio MCP connection configuration, using its actual executable and current vault directory. Returns launch paths only, never credentials or other clients' configuration.",json!({
    "type": "object",
    "properties": {
    },
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).open_world(false)),
        Tool::new("search_entries","Search controlled non-sensitive metadata. Never returns notes or protected values. API keys and tokens both use kind=api. siteMissing=true means the site is unknown: ask the user, never guess.",json!({
    "type": "object",
    "properties": {
        "query": {
            "type": "string"
        },
        "tag": {
            "type": "string"
        },
        "kind": {
            "type": "string",
            "enum": [
                "password",
                "api",
                "database",
                "generic"
            ]
        }
    },
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(true).open_world(false)),
        Tool::new("get_entries","Read controlled non-sensitive metadata only. Never returns a secret or notes. Use write_credential to use the credential locally. siteMissing=true means the site is unknown: ask the user, never guess.",json!({
    "type": "object",
    "properties": {
        "id": {
            "type": "string"
        }
    },
    "required": [
        "id"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(true).open_world(false)),create,
        Tool::new("update_entries","Update specified fields only. Read current revision first. Omitted fields including secret remain unchanged. No deletion or category changes. An entry with siteMissing=true must receive url (password) or services (API) in the same update, or the update is rejected.",json!({
    "type": "object",
    "properties": fields,
    "required": [
        "id",
        "expectedRevision"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(false).open_world(false)),
        Tool::new("delete_entries","Move credentials to trash by ID and expectedRevision. Atomic logical deletion only. Deleted entries cannot be searched, read or used. Restore from trash is available via restore_entries; permanent deletion is never available to MCP. Returns IDs only.",json!({
    "type": "object",
    "properties": {
        "id": {
            "type": "string"
        },
        "expectedRevision": {
            "type": "integer",
            "minimum": 1
        }
    },
    "required": [
        "id",
        "expectedRevision"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(true).open_world(false)),
        Tool::new("restore_entries","Restore one credential from trash by ID and expectedRevision. Requires update permission for the entry's tags. Returns controlled metadata only, never secrets or notes.",json!({
    "type": "object",
    "properties": {
        "id": {
            "type": "string"
        },
        "expectedRevision": {
            "type": "integer",
            "minimum": 1
        }
    },
    "required": [
        "id",
        "expectedRevision"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(false).open_world(false)),
        Tool::new("pin_entries","Pin or unpin one credential by ID and expectedRevision. Returns controlled metadata only, never secrets or notes.",json!({
    "type": "object",
    "properties": {
        "id": {
            "type": "string"
        },
        "expectedRevision": {
            "type": "integer",
            "minimum": 1
        },
        "pinned": {
            "type": "boolean"
        }
    },
    "required": [
        "id",
        "expectedRevision",
        "pinned"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(false).open_world(false)),
        Tool::new("organize","Rename or delete one tag globally across entries, trash and the tag's MCP permission rules. kind must be tag. Pass previous to rename it to name; pass previous with an empty name to delete it. Requires default update permission plus update permission on every affected entry; rejected when the change would grant MCP more permissions than before.",json!({
    "type": "object",
    "properties": {
        "kind": {
            "type": "string",
            "enum": [
                "tag"
            ]
        },
        "name": {
            "type": "string",
            "description": "New tag name; empty string with previous set deletes the tag"
        },
        "previous": {
            "type": "string",
            "description": "Existing tag to rename or delete"
        }
    },
    "required": [
        "kind",
        "name"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(true).open_world(false)),
        Tool::new("write_credential", "Use a stored credential locally without returning plaintext. Unified destinations: input uses a captured targetId; file uses an absolute path, creates a UTF-8 file by default, optionally a template with exactly one {{credential}} marker. mode=replace replaces exactly one placeholder in an existing UTF-8 file and preserves surrounding content; it never overwrites a whole existing file. No app/directory allowlist. Does not open or navigate apps. Acquire a target with capture_fill_target first; use its windowTitle+fieldName+expectedPassword options for browser inputs, then write. input_sent means keyboard events sent only; file_written means the file was written. Never read the resulting sensitive file through this tool.", json!({
    "type": "object",
    "properties": {
        "credentialId": {
            "type": "string"
        },
        "field": {
            "type": "string",
            "enum": [
                "account",
                "secret",
                "url",
                "service_url",
                "host",
                "port",
                "database_name",
                "database_type"
            ]
        },
        "serviceId": {
            "type": "string"
        },
        "target": {
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "type": {
                            "const": "input"
                        },
                        "targetId": {
                            "type": "string"
                        }
                    },
                    "required": [
                        "type",
                        "targetId"
                    ],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "type": {
                            "const": "file"
                        },
                        "path": {
                            "type": "string",
                            "description": "Absolute destination file path"
                        },
                        "mode": {
                            "type": "string",
                            "enum": [
                                "create",
                                "replace"
                            ],
                            "default": "create"
                        },
                        "template": {
                            "type": "string",
                            "description": "For create only; default {{credential}}"
                        },
                        "placeholder": {
                            "type": "string",
                            "description": "Exactly one occurrence required; default {{credential}}"
                        }
                    },
                    "required": [
                        "type",
                        "path"
                    ],
                    "additionalProperties": false
                }
            ]
        }
    },
    "required": [
        "credentialId",
        "field",
        "target"
    ],
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(true).open_world(false)),
        Tool::new("export_entries","Export an encrypted vault file through the desktop app. The desktop user reviews the request and types the passphrase; this tool never receives the passphrase, target path, plaintext or ciphertext, and returns status plus entry count only. Encryption is always applied. Requires write_file permission and an open desktop window; ask again later if the user does not respond within two minutes.",json!({
    "type": "object",
    "properties": {
        "includeTrash": {
            "type": "boolean",
            "default": false
        }
    },
    "additionalProperties": false
}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::new().read_only(false).destructive(false).open_world(false)),
        Tool::new("capture_fill_target","Capture a Windows input target. For Edge/Chrome specify windowTitle (literal substring), fieldName (exact accessible label) and expectedPassword=true for passwords. The unique named editable control is focused locally; ambiguous, readonly or mismatched targets are rejected. Optional processId narrows the window. Empty arguments capture the current native caret; DOM focus alone is insufficient. No field values are read. Returns a one-use reference valid for 30 seconds.",json!({
    "type": "object",
    "properties": {
        "windowTitle": {
            "type": "string",
            "maxLength": 512
        },
        "processId": {
            "type": "integer",
            "minimum": 1
        },
        "fieldName": {
            "type": "string",
            "maxLength": 256
        },
        "expectedPassword": {
            "type": "boolean"
        }
    },
    "additionalProperties": false
}).as_object().unwrap().clone()),
    ];
    for tool in &mut result {
        if matches!(
            tool.name.as_ref(),
            "search_entries"
                | "get_entries"
                | "create_entries"
                | "update_entries"
                | "delete_entries"
        ) {
            let single = Value::Object((*tool.input_schema).clone());
            let mut properties = single["properties"].clone();
            properties["items"] = json!({
                "type": "array",
                "minItems": 1,
                "maxItems": 100,
                "items": single
            });
            tool.input_schema = Arc::new(
                json!({
                    "type": "object",
                    "properties": properties,
                    "oneOf": [
                        single,
                        {
                            "type": "object",
                            "properties": {
                                "items": properties[
                                    "items"
                                ]
                            },
                            "required": [
                                "items"
                            ],
                            "additionalProperties": false
                        }
                    ],
                    "additionalProperties": false
                })
                .as_object()
                .unwrap()
                .clone(),
            );
            tool.description = Some(format!("Batch-first: pass items with 1-100 objects, or one object's fields directly. Batch responses have results in input order; direct single input preserves its single response. Batch writes are atomic; duplicate update/delete IDs are rejected. {}", tool.description.as_deref().unwrap_or("")).into());
        }
    }
    result
}
