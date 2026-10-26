//! Pure credential rules: no SQL, encryption, native calls or caller identity.
use vault_contracts::{
    credentials::*,
    errors::{AppError, Result},
};
pub(crate) fn normalize_services(value: &mut EntryInput) -> Result<()> {
    normalize_labels(value);
    value.tags = value.tags.iter().map(|t| t.trim().to_string()).collect();
    let mut tags = std::collections::HashSet::new();
    value.tags.retain(|t| tags.insert(t.clone()));
    if value.kind == Kind::Api {
        for service in &mut value.services {
            service.url = service.url.trim().into();
            if service.id.is_empty() {
                service.id = uuid::Uuid::new_v4().to_string();
            }
        }
        value.url = value
            .services
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_default();
    }
    if value.services.len() > 20 {
        return Err(AppError::invalid().with("services", "服务地址数量不能超过 20 个。"));
    }
    if value.kind != Kind::Api && !value.services.is_empty() {
        return Err(AppError::invalid().with("services", "仅 API 凭据可以保存服务地址。"));
    }
    let mut ids = std::collections::HashSet::new();
    for service in &value.services {
        if service.url.is_empty() {
            return Err(AppError::invalid().with("services", "服务地址不能为空。"));
        }
        if service.url.len() > 8192 {
            return Err(AppError::invalid().with("services", "服务地址长度不能超过 8192。"));
        }
        if service.id.len() > 64 {
            return Err(AppError::invalid().with("services", "服务 ID 长度不能超过 64。"));
        }
        if !ids.insert(&service.id) {
            return Err(AppError::invalid().with("services", "服务 ID 重复。"));
        }
    }
    Ok(())
}
pub(crate) fn validate(value: &EntryInput) -> Result<()> {
    if value.tags.len() > 64 {
        return Err(AppError::invalid().with("tags", "标签数量不能超过 64 个。"));
    }
    if value
        .tags
        .iter()
        .any(|t| t.trim().is_empty() || t.len() > 256)
    {
        return Err(AppError::invalid().with("tags", "标签不能为空且长度不能超过 256。"));
    }
    if value.kind != Kind::Database && value.database.is_some() {
        return Err(AppError::invalid().with("database", "仅数据库凭据可以保存数据库连接信息。"));
    }
    if value.kind == Kind::Database {
        let db = value
            .database
            .as_ref()
            .ok_or_else(|| AppError::invalid().with("database", "数据库凭据缺少连接信息。"))?;
        if db.database_type.trim().is_empty() {
            return Err(AppError::invalid().with("database.type", "数据库类型不能为空。"));
        }
        if db.database_type.len() > 128 {
            return Err(AppError::invalid().with("database.type", "数据库类型长度不能超过 128。"));
        }
        if db.host.len() > 4096 {
            return Err(AppError::invalid().with("database.host", "主机地址长度不能超过 4096。"));
        }
        if db.database_name.len() > 4096 {
            return Err(AppError::invalid().with("database.name", "数据库名长度不能超过 4096。"));
        }
        if !db.port.is_empty() && db.port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
            return Err(AppError::invalid().with("database.port", "端口必须是 1-65535 的数字。"));
        }
    }
    if value.title.trim().is_empty() {
        return Err(AppError::invalid().with("title", "名称不能为空。"));
    }
    if value.secret.is_empty() && matches!(value.kind, Kind::Password | Kind::Api) {
        return Err(AppError::invalid().with("secret", "账号密码与 API 凭据必须设置密钥。"));
    }
    if value.title.len() > 1024 {
        return Err(AppError::invalid().with("title", "名称长度不能超过 1024。"));
    }
    if value.account.len() > 4096 {
        return Err(AppError::invalid().with("account", "账号长度不能超过 4096。"));
    }
    if value.documentation_url.len() > 8192 {
        return Err(AppError::invalid().with("documentationUrl", "文档地址长度不能超过 8192。"));
    }
    if value.url.len() > 8192 {
        return Err(AppError::invalid().with("url", "网址长度不能超过 8192。"));
    }
    if value.secret.len() > 131072 {
        return Err(AppError::invalid().with("secret", "密钥长度不能超过 131072。"));
    }
    if value.notes.len() > 65536 {
        return Err(AppError::invalid().with("notes", "备注长度不能超过 65536。"));
    }
    Ok(())
}

pub(crate) fn normalize_labels(value: &mut EntryInput) {
    value.tags = value
        .tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let mut seen = std::collections::HashSet::new();
    value.tags.retain(|t| seen.insert(t.clone()));
}

/// Apply only supplied fields, then validate the complete candidate.
pub(crate) fn apply_patch(mut value: EntryInput, patch: EntryPatch) -> Result<EntryInput> {
    if let Some(v) = patch.title {
        value.title = v.trim().into();
    }
    if let Some(v) = patch.tags {
        value.tags = v;
    }
    if let Some(v) = patch.database {
        value.database = Some(v);
    }
    if let Some(v) = patch.documentation_url {
        value.documentation_url = v;
    }
    if let Some(v) = patch.account {
        value.account = v;
    }
    if let Some(services) = patch.services {
        if patch
            .url
            .as_ref()
            .is_some_and(|url| services.first().map(|s| s.url.trim()).unwrap_or("") != url.trim())
        {
            return Err(AppError::invalid().with(
                        "services",
                        "服务地址与 url 不一致：API 凭据的 url 必须等于第一个服务地址；非 API 凭据请勿传 services。",
                    ));
        }
        value.services = services;
        value.url.clear();
    } else if let Some(v) = patch.url {
        if value.kind == Kind::Api {
            return Err(AppError::invalid().with("services", "API 服务地址请通过 services 修改。"));
        }
        value.url = v;
    }
    normalize_services(&mut value)?;
    if let Some(v) = patch.secret {
        value.secret = v;
    }
    if let Some(v) = patch.notes {
        value.notes = v;
    }
    validate(&value)?;

    Ok(value)
}
