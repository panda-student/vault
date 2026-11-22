//! Transport-neutral input request/result contracts. No native operations live here.
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRequest {
    pub window_title: Option<String>,
    pub process_id: Option<u32>,
    pub field_name: Option<String>,
    pub expected_password: Option<bool>,
}
impl CaptureRequest {
    pub fn validate(&self) -> crate::errors::Result<()> {
        for (field, value, limit) in [
            ("windowTitle", &self.window_title, 512),
            ("fieldName", &self.field_name, 256),
        ] {
            if value.as_ref().is_some_and(|s| {
                s.trim().is_empty() || s.chars().count() > limit || s.chars().any(char::is_control)
            }) {
                return Err(crate::errors::AppError::invalid().with(
                    field,
                    "Use a nonempty literal label within the length limit.",
                ));
            }
        }
        if self.process_id == Some(0)
            || ((self.window_title.is_some() || self.process_id.is_some())
                && self.field_name.is_none())
        {
            return Err(crate::errors::AppError::invalid().with(
                "fieldName",
                "A window selector requires a named input field; processId must be positive.",
            ));
        }
        Ok(())
    }
}
/// A console program that waits for a password. The native side refuses unless
/// the console is really waiting for one (echo off, a password prompt at the
/// cursor, a program other than a shell in front).
pub struct ConsoleRequest {
    pub process_id: Option<u32>,
    pub window_title: Option<String>,
    pub submit: bool,
    /// Hosts the credential belongs to; a prompt naming another host is refused.
    pub hosts: Vec<String>,
}
/// What an askpass helper started under a program vault runs may receive.
pub struct AskpassAnswers {
    pub account: Option<zeroize::Zeroizing<String>>,
    pub secret: zeroize::Zeroizing<String>,
    /// Hosts the credential belongs to; a prompt naming another host is refused.
    pub hosts: Vec<String>,
}
/// A live askpass channel; it closes when dropped.
pub trait AskpassSession: Send {
    /// Environment variables that point git, ssh and similar tools at vault.
    fn env(&self) -> Vec<(String, String)>;
    /// Only helpers started below this process are answered.
    fn bind(&self, process_id: u32);
    /// How many prompts were answered.
    fn answered(&self) -> usize;
}
/// Host of a URL, or of `user@host`, lowercased; `None` when it names none.
pub fn prompt_host(prompt: &str) -> Option<String> {
    let lower = prompt.to_lowercase();
    let rest = if let Some(index) = lower.find("://") {
        let rest = &lower[index + 3..];
        let end = rest
            .find(|c: char| matches!(c, '/' | '\'' | '"' | ' ' | '?' | '#') || c.is_whitespace())
            .unwrap_or(rest.len());
        let authority = &rest[..end];
        authority.rsplit('@').next().unwrap_or("").to_owned()
    } else {
        let at = lower.find('@')?;
        let rest = &lower[at + 1..];
        let end = rest
            .find(|c: char| matches!(c, '\'' | '"' | ':' | ' ') || c.is_whitespace())
            .unwrap_or(rest.len());
        rest[..end].to_owned()
    };
    let host = if let Some(stripped) = rest.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("").to_owned()
    } else {
        rest.split(':').next().unwrap_or("").to_owned()
    };
    (!host.is_empty()).then_some(host)
}
/// Whether `host` is one of `hosts` or a subdomain of one.
pub fn host_allowed(host: &str, hosts: &[String]) -> bool {
    hosts.iter().any(|allowed| {
        let allowed = allowed.trim().to_lowercase();
        !allowed.is_empty() && (host == allowed || host.ends_with(&format!(".{allowed}")))
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FillStatus {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_password: Option<bool>,
}
impl FillStatus {
    pub fn status(status: &'static str) -> Self {
        Self {
            status,
            target_id: None,
            expires_in_seconds: None,
            process_id: None,
            reason: None,
            is_password: None,
        }
    }
}
