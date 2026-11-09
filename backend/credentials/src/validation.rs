//! Pure credential rules: no SQL, encryption, native calls or caller identity.
use vault_contracts::{
    credentials::*,
    errors::{AppError, Result},
};

const MAX_SERVICES: usize = 20;
const MAX_GROUPS: usize = 50;
const MAX_SECRET: usize = 131072;

/// Canonical cleanup shared by create, update and import. It only drops values that
/// carry no data (blank services, empty groups, an empty database block on another
/// kind) so harmless client padding never turns into a validation error.
pub(crate) fn normalize(value: &mut EntryInput) {
    normalize_labels(value);
    if value.kind != Kind::Database && value.database.as_ref().is_some_and(blank_database) {
        value.database = None;
    }
    for service in &mut value.services {
        service.url = service.url.trim().into();
    }
    value.services.retain(|service| !service.url.is_empty());
    for service in &mut value.services {
        if service.id.is_empty() {
            service.id = uuid::Uuid::new_v4().to_string();
        }
    }
    if value.kind == Kind::Api {
        value.url = value
            .services
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_default();
    }
    for group in &mut value.groups {
        group.environment = group.environment.trim().into();
    }
    value.groups.retain(|group| !group.is_empty());
    for group in &mut value.groups {
        if group.id.is_empty() {
            group.id = uuid::Uuid::new_v4().to_string();
        }
    }
}

fn blank_database(db: &DatabaseFields) -> bool {
    [&db.database_type, &db.host, &db.port, &db.database_name]
        .iter()
        .all(|value| value.trim().is_empty())
}

/// Legacy imports stay readable; new writes must associate a site.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteRule {
    Required,
    LegacyAllowed,
}

/// Canonical entry preparation shared by the write policy and import preflight.
pub(crate) fn prepare(value: &mut EntryInput, site: SiteRule) -> Result<()> {
    value.title = value.title.trim().into();
    value.url = value.url.trim().into();
    if value.kind == Kind::Api {
        let first = value
            .services
            .iter()
            .map(|s| s.url.trim())
            .find(|url| !url.is_empty());
        match first {
            // An API entry given only `url` uses it as its single address.
            None if !value.url.is_empty() => {
                value.services = vec![ApiService {
                    id: String::new(),
                    protocol: ApiProtocol::Custom,
                    url: value.url.clone(),
                }];
            }
            Some(first) if !value.url.is_empty() && first != value.url => {
                return Err(
                    AppError::invalid().with("url", "API 凭据的 url 必须与第一个服务地址一致。")
                );
            }
            _ => {}
        }
    }
    normalize(value);
    validate(value, site)
}

pub(crate) fn validate(value: &EntryInput, site: SiteRule) -> Result<()> {
    if value.tags.len() > 64 {
        return Err(AppError::invalid().with("tags", "标签数量不能超过 64 个。"));
    }
    if value.tags.iter().any(|t| t.len() > 256) {
        return Err(AppError::invalid().with("tags", "标签长度不能超过 256。"));
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
            return Err(AppError::invalid().with("database.databaseType", "数据库类型不能为空。"));
        }
        if db.database_type.len() > 128 {
            return Err(
                AppError::invalid().with("database.databaseType", "数据库类型长度不能超过 128。")
            );
        }
        if db.host.len() > 4096 {
            return Err(AppError::invalid().with("database.host", "主机地址长度不能超过 4096。"));
        }
        if db.database_name.len() > 4096 {
            return Err(
                AppError::invalid().with("database.databaseName", "数据库名长度不能超过 4096。")
            );
        }
        if !db.port.is_empty() && db.port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
            return Err(AppError::invalid().with("database.port", "端口必须是 1-65535 的数字。"));
        }
    }
    if !matches!(value.kind, Kind::Api | Kind::Password) && !value.services.is_empty() {
        return Err(
            AppError::invalid().with("services", "只有 API 凭据和账号密码可以保存 API 服务地址。")
        );
    }
    validate_services(&value.services)?;
    validate_groups(&value.groups)?;
    if value.display_name().is_empty() {
        return Err(AppError::invalid().with(
            "title",
            "条目按网站显示：请填写网站地址；没有网站时填写 title 或账号。",
        ));
    }
    if matches!(value.kind, Kind::Password | Kind::Api)
        && !value.groups.iter().any(|g| g.has_secret())
    {
        return Err(AppError::invalid().with(
            "groups",
            "账号密码与 API 凭据至少需要一组填写了 password 或 token 的凭据。",
        ));
    }
    if site == SiteRule::Required {
        match value.kind {
            Kind::Password if value.url.is_empty() => {
                return Err(AppError::invalid().with("url", "账号密码必须填写归属网站。"));
            }
            Kind::Api if value.services.is_empty() => {
                return Err(AppError::invalid()
                    .with("services", "API 凭据必须填写至少一个网站 / API 地址。"));
            }
            _ => {}
        }
    }
    if value.title.len() > 1024 {
        return Err(AppError::invalid().with("title", "名称长度不能超过 1024。"));
    }
    if value.documentation_url.len() > 8192 {
        return Err(AppError::invalid().with("documentationUrl", "文档地址长度不能超过 8192。"));
    }
    if value.url.len() > 8192 {
        return Err(AppError::invalid().with("url", "网址长度不能超过 8192。"));
    }
    if value.notes.len() > 65536 {
        return Err(AppError::invalid().with("notes", "备注长度不能超过 65536。"));
    }
    Ok(())
}

fn validate_services(services: &[ApiService]) -> Result<()> {
    if services.len() > MAX_SERVICES {
        return Err(AppError::invalid().with("services", "服务地址数量不能超过 20 个。"));
    }
    let mut ids = std::collections::HashSet::new();
    for service in services {
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

fn validate_groups(groups: &[CredentialGroup]) -> Result<()> {
    if groups.len() > MAX_GROUPS {
        return Err(AppError::invalid().with("groups", "凭据组数量不能超过 50 组。"));
    }
    let mut ids = std::collections::HashSet::new();
    for group in groups {
        if group.id.len() > 64 {
            return Err(AppError::invalid().with("groups.id", "凭据组 ID 长度不能超过 64。"));
        }
        if !ids.insert(&group.id) {
            return Err(AppError::invalid().with("groups.id", "凭据组 ID 重复。"));
        }
        if group.environment.len() > 256 {
            return Err(AppError::invalid().with("groups.environment", "环境长度不能超过 256。"));
        }
        if group.account.len() > 4096 {
            return Err(AppError::invalid().with("groups.account", "账号长度不能超过 4096。"));
        }
        if group.password.len() > MAX_SECRET {
            return Err(AppError::invalid().with("groups.password", "密码长度不能超过 131072。"));
        }
        if group.token.len() > MAX_SECRET {
            return Err(AppError::invalid().with("groups.token", "Token 长度不能超过 131072。"));
        }
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
    if let Some(v) = patch.notes {
        value.notes = v;
    }
    let url = patch.url.map(|url| url.trim().to_string());
    match (patch.services, url) {
        (Some(services), url) => {
            if value.kind == Kind::Api
                && url.as_ref().is_some_and(|url| {
                    services
                        .iter()
                        .map(|s| s.url.trim())
                        .find(|u| !u.is_empty())
                        .unwrap_or("")
                        != url
                })
            {
                return Err(AppError::invalid().with(
                    "services",
                    "服务地址与 url 不一致：API 凭据的 url 由第一个服务地址决定，只传 services 即可。",
                ));
            }
            value.services = services;
            if let Some(url) = url.filter(|_| value.kind != Kind::Api) {
                value.url = url;
            }
        }
        // For API entries `url` is the first service address, so changing it alone
        // moves that address (or adds one when there is none).
        (None, Some(url)) if value.kind == Kind::Api => match value.services.first_mut() {
            Some(first) => first.url = url,
            None => value.services.push(ApiService {
                id: String::new(),
                protocol: ApiProtocol::Custom,
                url,
            }),
        },
        (None, Some(url)) => value.url = url,
        (None, None) => {}
    }
    match (patch.account, patch.secret) {
        (None, None) => apply_group_ops(&mut value, patch.groups, patch.remove_groups)?,
        _ if patch.groups.is_some() || patch.remove_groups.is_some() => {
            return Err(AppError::invalid().with(
                "groups",
                "请只用 groups / removeGroups 修改凭据组，不要同时传 account 或 secret。",
            ));
        }
        (account, secret) => apply_single_login(&mut value, account, secret)?,
    }
    normalize(&mut value);
    validate(&value, SiteRule::Required)?;
    Ok(value)
}

/// Single-login shorthand from older clients: edits the entry's only group.
fn apply_single_login(
    value: &mut EntryInput,
    account: Option<String>,
    secret: Option<String>,
) -> Result<()> {
    if value.groups.len() > 1 {
        return Err(AppError::invalid().with(
            "groups",
            "此条目有多组凭据，请用 groups 并指定 id 修改其中一组。",
        ));
    }
    if value.groups.is_empty() {
        value.groups.push(CredentialGroup::default());
    }
    let kind = value.kind;
    let group = &mut value.groups[0];
    if let Some(account) = account {
        group.account = account;
    }
    if let Some(secret) = secret {
        group.set_secret(kind, secret);
    }
    Ok(())
}

/// Remove groups by id, then update groups by id or add groups without one.
fn apply_group_ops(
    value: &mut EntryInput,
    groups: Option<Vec<GroupPatch>>,
    removed: Option<Vec<String>>,
) -> Result<()> {
    for id in removed.into_iter().flatten() {
        let before = value.groups.len();
        value.groups.retain(|group| group.id != id);
        if value.groups.len() == before {
            return Err(missing_group());
        }
    }
    for patch in groups.into_iter().flatten() {
        let index = match patch.id.as_deref().filter(|id| !id.is_empty()) {
            Some(id) => value
                .groups
                .iter()
                .position(|group| group.id == id)
                .ok_or_else(missing_group)?,
            None => {
                value.groups.push(CredentialGroup::default());
                value.groups.len() - 1
            }
        };
        let group = &mut value.groups[index];
        if let Some(v) = patch.environment {
            group.environment = v;
        }
        if let Some(v) = patch.account {
            group.account = v;
        }
        if let Some(v) = patch.password {
            group.password = v;
        }
        if let Some(v) = patch.token {
            group.token = v;
        }
    }
    Ok(())
}

fn missing_group() -> AppError {
    AppError::invalid().with(
        "groups.id",
        "凭据组 id 不存在：请先读取条目获取 groups[].id；新增一组时不要传 id。",
    )
}
