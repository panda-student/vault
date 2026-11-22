//! Puts stored values where placeholders such as `{{password}}` stand and
//! remembers every form of what it put in, so output can be masked before it
//! goes back to the caller. The secret never passes through the caller.
use base64::Engine;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use vault_contracts::{
    credentials::EntryDetail,
    errors::{AppError, Result},
};
use zeroize::Zeroizing;

const TEMPLATE_LIMIT: usize = 4 * 1024 * 1024;
const MASK: &str = "***";

fn placeholder_error(reason: &str) -> AppError {
    AppError::invalid().with("placeholder", reason)
}

/// Renders templates and remembers every value it injected, in each form that
/// could come back in a response, so the response can be masked.
pub(crate) struct Injector<'a> {
    entry: &'a EntryDetail,
    group_id: Option<&'a str>,
    /// The value of the write's `field`, for `{{credential}}`.
    credential: Option<(Zeroizing<String>, bool)>,
    masks: Vec<Zeroizing<String>>,
}

impl<'a> Injector<'a> {
    pub(crate) fn new(entry: &'a EntryDetail, group_id: Option<&'a str>) -> Self {
        Self {
            entry,
            group_id,
            credential: None,
            masks: Vec::new(),
        }
    }

    /// Sets what `{{credential}}` stands for; a secret one is masked in output.
    pub(crate) fn with_credential(mut self, value: Zeroizing<String>, secret: bool) -> Self {
        if secret {
            self.remember_forms(&value);
        }
        self.credential = Some((value, secret));
        self
    }

    fn remember(&mut self, value: &str) {
        if value.is_empty() || self.masks.iter().any(|mask| mask.as_str() == value) {
            return;
        }
        self.masks.push(Zeroizing::new(value.to_owned()));
    }

    fn remember_forms(&mut self, value: &str) {
        let encoded = Zeroizing::new(serde_json::to_string(value).unwrap_or_default());
        let escaped = encoded
            .get(1..encoded.len().saturating_sub(1))
            .unwrap_or("");
        for form in [
            Zeroizing::new(value.to_owned()),
            Zeroizing::new(escaped.to_owned()),
            Zeroizing::new(utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()),
            Zeroizing::new(form_encode(value)),
            Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(value)),
            Zeroizing::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)),
            Zeroizing::new(hex(value, false)),
            Zeroizing::new(hex(value, true)),
        ] {
            self.remember(&form);
        }
    }

    fn value(&mut self, name: &str) -> Result<Zeroizing<String>> {
        let group = self.entry.group(self.group_id)?;
        let value = Zeroizing::new(match name {
            "basic" => {
                let account = Zeroizing::new(group.field("account")?);
                let secret = Zeroizing::new(group.field("secret")?);
                self.remember_forms(&secret);
                base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    account.as_str(),
                    secret.as_str()
                ))
            }
            "credential" => {
                let (value, secret) = self
                    .credential
                    .as_ref()
                    .ok_or_else(|| placeholder_error("没有可用的 {{credential}} 值。"))?;
                if !secret {
                    return Ok(Zeroizing::new(value.to_string()));
                }
                value.to_string()
            }
            _ => group.field(name)?,
        });
        if name != "account" {
            self.remember_forms(&value);
        }
        Ok(value)
    }

    /// Replaces `{{account}}`, `{{password}}`, `{{token}}`, `{{secret}}`,
    /// `{{credential}}` and `{{basic}}`, each optionally followed by filters `|json`, `|url` or
    /// `|base64`. Any other `{{...}}` is left as written.
    pub(crate) fn render(&mut self, template: &str) -> Result<Zeroizing<String>> {
        if template.len() > TEMPLATE_LIMIT {
            return Err(placeholder_error("单项内容不能超过 4 MB。"));
        }
        let mut out = Zeroizing::new(String::with_capacity(template.len()));
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            let Some(length) = rest[start + 2..].find("}}") else {
                break;
            };
            let inner = &rest[start + 2..start + 2 + length];
            let mut parts = inner.split('|').map(str::trim);
            let name = parts.next().unwrap_or("").to_ascii_lowercase();
            if !matches!(
                name.as_str(),
                "account" | "password" | "token" | "secret" | "credential" | "basic"
            ) {
                out.push_str(&rest[..start + 2 + length + 2]);
                rest = &rest[start + 2 + length + 2..];
                continue;
            }
            let secret = match name.as_str() {
                "account" => false,
                "credential" => self.credential.as_ref().is_some_and(|(_, secret)| *secret),
                _ => true,
            };
            let mut value = self.value(&name)?;
            for filter in parts {
                value = Zeroizing::new(match filter.to_ascii_lowercase().as_str() {
                    "json" => {
                        let encoded = Zeroizing::new(
                            serde_json::to_string(value.as_str())
                                .map_err(|_| AppError::invalid())?,
                        );
                        encoded[1..encoded.len() - 1].to_owned()
                    }
                    "url" => utf8_percent_encode(&value, NON_ALPHANUMERIC).to_string(),
                    "base64" => base64::engine::general_purpose::STANDARD.encode(value.as_str()),
                    _ => {
                        return Err(placeholder_error(
                            "占位符过滤器只支持 json、url、base64，例如 {{password|json}}。",
                        ));
                    }
                });
                if secret {
                    self.remember(&value);
                }
            }
            out.push_str(&rest[..start]);
            out.push_str(&value);
            rest = &rest[start + 2 + length + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }

    /// Remembers every secret the group holds, so a stream can be masked
    /// before anything was injected into it. Returns how many forms are known.
    pub(crate) fn prime(&mut self) -> usize {
        for name in ["password", "token", "secret"] {
            let _ = self.value(name);
        }
        self.masks.len()
    }

    /// Masks every injected value in program output, longest first so a value
    /// is not left half masked by a shorter one it contains. Output cut at the
    /// size limit may end with the first part of a value; that tail is dropped.
    pub(crate) fn mask_output(&self, bytes: &[u8], truncated: bool) -> String {
        let mut masks: Vec<&[u8]> = self.masks.iter().map(|mask| mask.as_bytes()).collect();
        masks.sort_by_key(|mask| std::cmp::Reverse(mask.len()));
        let mut out = Zeroizing::new(Vec::with_capacity(bytes.len()));
        let mut index = 0;
        'scan: while index < bytes.len() {
            for mask in &masks {
                if bytes[index..].starts_with(mask) {
                    out.extend_from_slice(MASK.as_bytes());
                    index += mask.len();
                    continue 'scan;
                }
            }
            if truncated
                && masks.iter().any(|mask| {
                    mask.len() > bytes.len() - index && mask.starts_with(&bytes[index..])
                })
            {
                break;
            }
            out.push(bytes[index]);
            index += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

fn hex(value: &str, upper: bool) -> String {
    value
        .bytes()
        .map(|byte| {
            if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect()
}

fn form_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
