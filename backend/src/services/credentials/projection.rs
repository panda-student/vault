//! AI-safe projection: redact before filtering to prevent a search oracle.
use super::CredentialService;
use crate::definitions::{
    credentials::Settings,
    errors::{AppError, Result},
    operations::Search,
};
use crate::services::permissions::output::SharedGuard;

use serde_json::{Value, json};
pub(crate) fn metadata(core: &CredentialService, guard: &SharedGuard, id: &str) -> Result<Value> {
    let summary = core.summary(id)?;
    crate::services::permissions::output::scrub(
        guard,
        json!({
            "id":summary.id,
            "kind":summary.kind,
            "title":summary.title,
            "account":summary.account,
            "url":summary.url,
            "documentationUrl":summary.documentation_url,
            "services":summary.services,
            "tags":summary.tags,
            "database":summary.database,
            "revision":summary.revision,
            "pinned":summary.pinned,
            "createdAt":summary.created_at,
            "updatedAt":summary.updated_at
        }),
    )
}
pub(crate) fn search(
    core: &CredentialService,
    guard: &SharedGuard,
    args: Search,
    settings: &Settings,
) -> Result<Value> {
    let query = args.query.to_lowercase();
    if query.len() > 4096 {
        return Err(AppError::invalid().with("query", "搜索词长度不能超过 4096。"));
    }
    let mut entries = Vec::new();
    for summary in core.list("", args.kind)? {
        if !crate::services::permissions::permitted(
            settings,
            Some(&summary.id),
            &summary.tags,
            crate::services::permissions::Access::Read,
        ) {
            continue;
        }
        let item = metadata(core, guard, &summary.id)?;
        if args.tag.as_ref().is_some_and(|tag| {
            !item["tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag.as_str())))
        }) {
            continue;
        }
        // Searching redacted fields would become an oracle for secrets
        // accidentally placed in names, accounts or URLs.
        if query.is_empty()
            || ["title", "account", "url", "documentationUrl"]
                .iter()
                .any(|field| {
                    item[*field]
                        .as_str()
                        .is_some_and(|v| v.to_lowercase().contains(&query))
                })
            || item["tags"].as_array().is_some_and(|tags| {
                tags.iter().any(|t| {
                    t.as_str()
                        .is_some_and(|s| s.to_lowercase().contains(&query))
                })
            })
            || item["database"].as_object().is_some_and(|db| {
                db.values().any(|v| {
                    v.as_str()
                        .is_some_and(|s| s.to_lowercase().contains(&query))
                })
            })
            || item["services"].as_array().is_some_and(|services| {
                services.iter().any(|service| {
                    ["protocol", "url"].iter().any(|field| {
                        service[*field]
                            .as_str()
                            .is_some_and(|v| v.to_lowercase().contains(&query))
                    })
                })
            })
        {
            entries.push(item);
        }
    }
    let has_more = entries.len() > 100;
    entries.truncate(100);
    Ok(json!({
        "entries":entries,
        "hasMore":has_more
    }))
}
