use super::errors::{AppError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Password,
    Api,
    Database,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiProtocol {
    Openai,
    Anthropic,
    Gemini,
    Custom,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiService {
    #[serde(default)]
    pub id: String,
    pub protocol: ApiProtocol,
    pub url: String,
}
impl EntrySummary {
    pub fn service(&self, id: Option<&str>) -> Result<&ApiService> {
        if let Some(id) = id {
            return self
                .services
                .iter()
                .find(|s| s.id == id)
                .ok_or(AppError::new(
                    "SERVICE_NOT_FOUND",
                    "服务地址不存在，请刷新条目。",
                ));
        }
        if self.services.len() > 1 {
            return Err(AppError::new(
                "SERVICE_REQUIRED",
                "此凭据包含多个服务，请指定 serviceId。",
            ));
        }
        self.services.first().ok_or(AppError::new(
            "SERVICE_REQUIRED",
            "请先为此凭据添加服务地址。",
        ))
    }
}
/// One login inside an entry, e.g. the production and the test account of one site.
/// Each group may hold a password, a token, or both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialGroup {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub token: String,
}

/// Id given to the group converted from a record saved before groups existed.
pub const LEGACY_GROUP_ID: &str = "default";

impl CredentialGroup {
    /// Single-login shorthand from records, files and clients without groups.
    pub fn from_secret(kind: Kind, account: String, secret: String) -> Self {
        let mut group = Self {
            account,
            ..Self::default()
        };
        group.set_secret(kind, secret);
        group
    }
    /// The single-secret rule: API keys are tokens, a group that only holds a
    /// token keeps receiving tokens, and every other secret is a password.
    pub fn set_secret(&mut self, kind: Kind, secret: String) {
        if kind == Kind::Api || (self.password.is_empty() && !self.token.is_empty()) {
            self.token = secret;
        } else {
            self.password = secret;
        }
    }
    pub fn is_empty(&self) -> bool {
        [
            &self.environment,
            &self.account,
            &self.password,
            &self.token,
        ]
        .iter()
        .all(|value| value.is_empty())
    }
    pub fn has_secret(&self) -> bool {
        !self.password.is_empty() || !self.token.is_empty()
    }
    pub fn summary(&self) -> GroupSummary {
        GroupSummary {
            id: self.id.clone(),
            environment: self.environment.clone(),
            account: self.account.clone(),
            has_password: !self.password.is_empty(),
            has_token: !self.token.is_empty(),
        }
    }
    /// Password or token as stored; empty values are reported, never written out.
    pub fn field(&self, field: &str) -> Result<String> {
        let value = match field {
            "environment" => &self.environment,
            "account" => &self.account,
            "password" => &self.password,
            "token" => &self.token,
            // Legacy single-secret field: the password, or the token when there is none.
            "secret" if self.password.is_empty() => &self.token,
            "secret" => &self.password,
            _ => return Err(AppError::invalid().with("field", "不支持的字段。")),
        };
        if value.is_empty() && matches!(field, "password" | "token" | "secret") {
            return Err(AppError::new("FIELD_EMPTY", "该凭据组没有保存这个字段。"));
        }
        Ok(value.clone())
    }
}

/// Pick a group by id; without an id the entry must have exactly one group.
pub fn select_group<'a, T>(
    groups: &'a [T],
    id: Option<&str>,
    key: impl Fn(&T) -> &str,
) -> Result<&'a T> {
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        return groups.iter().find(|g| key(g) == id).ok_or(AppError::new(
            "GROUP_NOT_FOUND",
            "凭据组不存在，请重新读取条目获取 groups[].id。",
        ));
    }
    match groups {
        [only] => Ok(only),
        [] => Err(AppError::new("GROUP_NOT_FOUND", "此条目还没有凭据组。")),
        _ => Err(AppError::new(
            "GROUP_REQUIRED",
            "此条目有多组凭据，请用 groupId 指定其中一组。",
        )),
    }
}

/// Secret-free view of a group for listings and MCP metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: String,
    pub environment: String,
    pub account: String,
    pub has_password: bool,
    pub has_token: bool,
}

/// Update or add one group. A known id updates that group and omitted fields keep
/// their values; no id adds a new group. Groups are removed only via `removeGroups`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupPatch {
    pub id: Option<String>,
    pub environment: Option<String>,
    pub account: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "EntryWire")]
pub struct EntryInput {
    pub kind: Kind,
    pub title: String,
    pub tags: Vec<String>,
    pub database: Option<DatabaseFields>,
    pub url: String,
    pub documentation_url: String,
    pub services: Vec<ApiService>,
    pub notes: String,
    pub groups: Vec<CredentialGroup>,
}

/// Wire form of an entry. Records, exports and older clients may still use the
/// single-login `account` + `secret` fields; they become one group.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EntryWire {
    kind: Kind,
    /// Optional: entries are shown by their site when no title is given.
    #[serde(default)]
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    database: Option<DatabaseFields>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    documentation_url: String,
    #[serde(default)]
    services: Vec<ApiService>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    groups: Vec<CredentialGroup>,
    #[serde(default)]
    account: String,
    #[serde(default)]
    secret: String,
}

impl TryFrom<EntryWire> for EntryInput {
    type Error = String;
    fn try_from(wire: EntryWire) -> std::result::Result<Self, String> {
        let mut groups = wire.groups;
        if !wire.account.is_empty() || !wire.secret.is_empty() {
            if !groups.is_empty() {
                return Err(
                    "use either groups or the single-login account/secret fields, not both".into(),
                );
            }
            let mut group = CredentialGroup::from_secret(wire.kind, wire.account, wire.secret);
            group.id = LEGACY_GROUP_ID.into();
            groups.push(group);
        }
        Ok(Self {
            kind: wire.kind,
            title: wire.title,
            tags: wire.tags,
            database: wire.database,
            url: wire.url,
            documentation_url: wire.documentation_url,
            services: wire.services,
            notes: wire.notes,
            groups,
        })
    }
}

/// Host of a URL without scheme, credentials, path or a leading `www.`.
pub fn site_host(url: &str) -> Option<String> {
    let rest = url.trim();
    let rest = rest.split_once("://").map_or(rest, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.strip_prefix("www.").unwrap_or(host).to_lowercase();
    (!host.is_empty()).then_some(host)
}

impl EntryInput {
    /// Name shown for the entry. Entries are shown by the site they belong to;
    /// the stored title only names entries without one.
    pub fn display_name(&self) -> String {
        if let Some(host) =
            site_host(&self.url).or_else(|| self.services.iter().find_map(|s| site_host(&s.url)))
        {
            return host;
        }
        let title = self.title.trim();
        if !title.is_empty() {
            return title.to_string();
        }
        if let Some(db) = &self.database {
            let (name, host) = (db.database_name.trim(), db.host.trim());
            match (name.is_empty(), host.is_empty()) {
                (false, false) => return format!("{name} @ {host}"),
                (true, false) => return host.to_string(),
                (false, true) => return name.to_string(),
                _ => {}
            }
        }
        self.groups
            .iter()
            .map(|g| g.account.trim())
            .find(|account| !account.is_empty())
            .unwrap_or("")
            .to_string()
    }
    /// Every stored password and token, for output protection.
    pub fn secrets(&self) -> impl Iterator<Item = &String> {
        self.groups
            .iter()
            .flat_map(|g| [&g.password, &g.token])
            .filter(|value| !value.is_empty())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPatch {
    pub id: String,
    pub expected_revision: i64,
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub database: Option<DatabaseFields>,
    pub url: Option<String>,
    pub documentation_url: Option<String>,
    pub services: Option<Vec<ApiService>>,
    pub notes: Option<String>,
    pub groups: Option<Vec<GroupPatch>>,
    pub remove_groups: Option<Vec<String>>,
    /// Single-login shorthand; only valid while the entry has at most one group.
    pub account: Option<String>,
    pub secret: Option<String>,
}

impl EntryPatch {
    /// New passwords and tokens carried by this patch, for output protection.
    pub fn secrets(&self) -> Vec<&String> {
        let mut values: Vec<&String> = self.secret.iter().collect();
        for group in self.groups.iter().flatten() {
            values.extend(group.password.iter().chain(group.token.iter()));
        }
        values.retain(|value| !value.is_empty());
        values
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryVersion {
    pub id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub database: Option<DatabaseFields>,
    /// Name shown for the entry: its website host, else the stored title.
    #[serde(default)]
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub documentation_url: String,
    pub services: Vec<ApiService>,
    #[serde(default)]
    pub groups: Vec<GroupSummary>,
    pub notes: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
}

/// Internal read model with secrets; never serialized to a client.
pub struct EntryDetail {
    pub summary: EntrySummary,
    pub groups: Vec<CredentialGroup>,
}

impl EntryDetail {
    pub fn group(&self, id: Option<&str>) -> Result<&CredentialGroup> {
        select_group(&self.groups, id, |g| g.id.as_str())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub mcp_enabled: bool,
    pub close_to_tray: bool,
    pub mcp_permissions: McpPermissions,
    pub mcp_access: bool,
    pub mcp_tag_rules: std::collections::BTreeMap<String, PermissionRule>,
    pub mcp_entry_rules: std::collections::BTreeMap<String, PermissionRule>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_metadata: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_input: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_file: Option<bool>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mcp_enabled: true,
            close_to_tray: false,
            mcp_permissions: McpPermissions::default(),
            mcp_access: true,
            mcp_tag_rules: Default::default(),
            mcp_entry_rules: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpPermissions {
    pub read_metadata: bool,
    pub create: bool,
    pub update: bool,
    pub delete: bool,
    pub fill_input: bool,
    pub write_file: bool,
    /// Read every setting and change non-permission settings. Off unless the user enables it.
    #[serde(default)]
    pub settings: bool,
}
impl Default for McpPermissions {
    fn default() -> Self {
        Self {
            read_metadata: true,
            create: true,
            update: true,
            delete: true,
            fill_input: true,
            write_file: true,
            settings: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseFields {
    #[serde(default)]
    pub database_type: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub database_name: String,
}
