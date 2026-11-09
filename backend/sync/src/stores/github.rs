//! A file in a GitHub private repository, through the REST contents API.
//! The repository may hold other files; only the configured path is touched.
use super::http::{body, client, rate_limited, send, status_error};
use super::{remote_changed, segments};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::{Method, StatusCode, Url};
use serde_json::{Value, json};
use std::sync::Mutex;
use vault_contracts::{
    errors::{AppError, Result},
    ports::sync::{RemoteObject, RemoteState, RemoteStore},
};
use zeroize::Zeroizing;

const API: &str = "https://api.github.com";

pub struct GithubStore {
    client: Client,
    api: Url,
    owner: String,
    repo: String,
    branch: Mutex<Option<String>>,
    segments: Vec<String>,
    token: Zeroizing<String>,
}

struct RepoInfo {
    private: bool,
    can_push: bool,
    default_branch: String,
}

impl GithubStore {
    pub fn new(
        owner: &str,
        repo: &str,
        branch: Option<&str>,
        path: &str,
        token: &str,
    ) -> Result<Self> {
        let name_ok = |name: &str| {
            !name.is_empty()
                && name.len() <= 100
                && name != "."
                && name != ".."
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !name_ok(owner) || !name_ok(repo) {
            return Err(AppError::invalid().with("repo", "GitHub 用户名或仓库名格式不正确。"));
        }
        if token.trim().is_empty() {
            return Err(AppError::new(
                "SYNC_CREDENTIAL_MISSING",
                "引用的凭据里没有 Token 或密码。",
            ));
        }
        Ok(Self {
            client: client()?,
            api: Url::parse(API).expect("constant URL"),
            owner: owner.into(),
            repo: repo.into(),
            branch: Mutex::new(branch.filter(|b| !b.trim().is_empty()).map(String::from)),
            segments: segments(path)?,
            token: Zeroizing::new(token.trim().into()),
        })
    }

    fn endpoint(&self, parts: &[&str]) -> Url {
        let mut url = self.api.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(parts);
        }
        url
    }
    fn contents_url(&self) -> Url {
        let mut parts = vec!["repos", &self.owner, &self.repo, "contents"];
        parts.extend(self.segments.iter().map(String::as_str));
        self.endpoint(&parts)
    }
    fn call(&self, method: Method, url: Url) -> RequestBuilder {
        self.client
            .request(method, url)
            .bearer_auth(self.token.as_str())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// Send and parse. Rate limiting is reported as such, not as "forbidden",
    /// and a success with an unreadable body is an unexpected response.
    fn exchange(&self, request: RequestBuilder) -> Result<(StatusCode, Value)> {
        let response = send(request)?;
        let status = response.status();
        let limited = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v.as_bytes() == b"0");
        if status == StatusCode::FORBIDDEN && limited {
            return Err(rate_limited());
        }
        let bytes = body(response)?;
        let value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) if status.is_success() => return Err(unexpected()),
            Err(_) => Value::Null,
        };
        Ok((status, value))
    }
    /// The parsed body on success, `None` on 404, otherwise the mapped error.
    fn json_opt(&self, request: RequestBuilder) -> Result<Option<Value>> {
        match self.exchange(request)? {
            (StatusCode::NOT_FOUND, _) => Ok(None),
            (status, value) if status.is_success() => Ok(Some(value)),
            (status, _) => Err(status_error(status)),
        }
    }
    fn json_ok(&self, request: RequestBuilder) -> Result<Value> {
        self.json_opt(request)?
            .ok_or_else(|| status_error(StatusCode::NOT_FOUND))
    }

    fn repo_info(&self) -> Result<Option<RepoInfo>> {
        let url = self.endpoint(&["repos", &self.owner, &self.repo]);
        self.json_opt(self.call(Method::GET, url))?
            .map(|value| parse_repo(&value))
            .transpose()
    }

    /// The repository, created when missing and `create` is set.
    fn ensure_repo(&self, create: bool) -> Result<(RepoInfo, bool)> {
        match self.repo_info()? {
            Some(info) => Ok((info, false)),
            None if create => Ok((self.create_repository()?, true)),
            None => Err(status_error(StatusCode::NOT_FOUND)),
        }
    }

    /// Refuse public or read-only repositories and remember the default branch.
    fn check_writable(&self, info: RepoInfo) -> Result<()> {
        if !info.private {
            return Err(AppError::new(
                "SYNC_REPO_PUBLIC",
                "该仓库是公开仓库。为安全起见只同步到私有仓库，请换一个仓库或把它设为私有。",
            ));
        }
        if !info.can_push {
            return Err(status_error(StatusCode::FORBIDDEN));
        }
        let mut branch = self.branch.lock().map_err(|_| unexpected())?;
        branch.get_or_insert(info.default_branch);
        Ok(())
    }

    fn branch(&self) -> Result<String> {
        if let Some(name) = self.branch.lock().map_err(|_| unexpected())?.clone() {
            return Ok(name);
        }
        let (info, _) = self.ensure_repo(false)?;
        self.check_writable(info)?;
        self.branch
            .lock()
            .map_err(|_| unexpected())?
            .clone()
            .ok_or_else(unexpected)
    }

    fn create_repository(&self) -> Result<RepoInfo> {
        let me = self.json_ok(self.call(Method::GET, self.endpoint(&["user"])))?;
        let login = me["login"].as_str().unwrap_or_default();
        let url = if login.eq_ignore_ascii_case(&self.owner) {
            self.endpoint(&["user", "repos"])
        } else {
            self.endpoint(&["orgs", &self.owner, "repos"])
        };
        let payload = json!({"name": self.repo, "private": true, "auto_init": true});
        match self.exchange(self.call(Method::POST, url).json_body(&payload))? {
            (status, value) if status.is_success() => parse_repo(&value),
            // The name is taken by a repository this token cannot see.
            (StatusCode::UNPROCESSABLE_ENTITY, _) => Err(AppError::new(
                "SYNC_FORBIDDEN",
                "同名仓库已存在，但当前凭据无法访问它。请检查 Token 的仓库权限。",
            )),
            (status, _) => Err(status_error(status)),
        }
    }

    /// The file's sha and, when inlined, its bytes; never downloads a blob.
    fn contents(&self) -> Result<Option<(String, Option<Vec<u8>>)>> {
        let mut url = self.contents_url();
        url.query_pairs_mut().append_pair("ref", &self.branch()?);
        // The contents API is cached; ask for the current version.
        let request = self
            .call(Method::GET, url)
            .header("Cache-Control", "no-cache");
        self.json_opt(request)?
            .map(|value| parse_contents(&value))
            .transpose()
    }
}

trait JsonBody {
    fn json_body(self, value: &Value) -> Self;
}
impl JsonBody for RequestBuilder {
    fn json_body(self, value: &Value) -> Self {
        self.header("Content-Type", "application/json")
            .body(value.to_string())
    }
}

impl RemoteStore for GithubStore {
    fn read(&self) -> Result<Option<RemoteObject>> {
        let Some((sha, inline)) = self.contents()? else {
            return Ok(None);
        };
        let bytes = match inline {
            Some(bytes) => bytes,
            // Files over 1 MB come without content: fetch the blob instead.
            None => {
                let url = self.endpoint(&["repos", &self.owner, &self.repo, "git", "blobs", &sha]);
                decode_base64(&self.json_ok(self.call(Method::GET, url))?["content"])?
            }
        };
        Ok(Some(RemoteObject {
            bytes,
            version: sha,
        }))
    }

    fn write(&self, bytes: &[u8], expected: Option<&str>) -> Result<String> {
        // Checked on every write: the repository may have been made public
        // since the last sync.
        let (info, _) = self.ensure_repo(false)?;
        self.check_writable(info)?;
        let mut payload = json!({
            "message": "vault sync",
            "content": STANDARD.encode(bytes),
            "branch": self.branch()?,
        });
        if let Some(sha) = expected {
            payload["sha"] = json!(sha);
        }
        let request = self
            .call(Method::PUT, self.contents_url())
            .json_body(&payload);
        match self.exchange(request)? {
            (status, value) if status.is_success() => value["content"]["sha"]
                .as_str()
                .map(String::from)
                .ok_or_else(unexpected),
            // A stale or missing sha: the file changed since it was read.
            (StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY, _) => Err(remote_changed()),
            (status, _) => Err(status_error(status)),
        }
    }

    fn prepare(&self, create: bool) -> Result<RemoteState> {
        let (info, created) = self.ensure_repo(create)?;
        self.check_writable(info)?;
        Ok(RemoteState {
            exists: self.contents()?.is_some(),
            created,
        })
    }
}

fn parse_repo(value: &Value) -> Result<RepoInfo> {
    Ok(RepoInfo {
        private: value["private"].as_bool().ok_or_else(unexpected)?,
        // Missing permissions (e.g. a fresh repository) means the owner created it.
        can_push: value["permissions"]["push"].as_bool().unwrap_or(true),
        default_branch: value["default_branch"]
            .as_str()
            .filter(|b| !b.is_empty())
            .unwrap_or("main")
            .into(),
    })
}

/// The file's sha and, when GitHub inlined it, its bytes.
fn parse_contents(value: &Value) -> Result<(String, Option<Vec<u8>>)> {
    if value["type"].as_str() != Some("file") {
        return Err(AppError::new(
            "SYNC_FOREIGN_FILE",
            "同步路径在仓库里不是一个文件，未做任何改动。请换一个路径。",
        ));
    }
    let sha = value["sha"].as_str().ok_or_else(unexpected)?.to_string();
    let inline = match (value["encoding"].as_str(), value["content"].as_str()) {
        (Some("base64"), Some(content)) if !content.is_empty() || value["size"] == 0 => {
            Some(decode_base64(&value["content"])?)
        }
        _ => None,
    };
    Ok((sha, inline))
}

fn decode_base64(value: &Value) -> Result<Vec<u8>> {
    let text: String = value
        .as_str()
        .ok_or_else(unexpected)?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    STANDARD.decode(text).map_err(|_| unexpected())
}

fn unexpected() -> AppError {
    AppError::new(
        "SYNC_UNAVAILABLE",
        "GitHub 返回了无法识别的响应，请稍后再同步。",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_responses_are_parsed_and_folders_refused() {
        let inline = json!({"type": "file", "sha": "abc", "size": 3, "encoding": "base64", "content": "b25l\n"});
        assert_eq!(
            parse_contents(&inline).unwrap(),
            ("abc".into(), Some(b"one".to_vec()))
        );
        // Large files come without content.
        let large = json!({"type": "file", "sha": "big", "size": 2_000_000, "encoding": "none", "content": ""});
        assert_eq!(parse_contents(&large).unwrap(), ("big".into(), None));
        let empty =
            json!({"type": "file", "sha": "e", "size": 0, "encoding": "base64", "content": ""});
        assert_eq!(parse_contents(&empty).unwrap(), ("e".into(), Some(vec![])));
        assert_eq!(
            parse_contents(&json!([{"type": "file"}])).unwrap_err().code,
            "SYNC_FOREIGN_FILE"
        );
    }

    #[test]
    fn repository_checks_read_privacy_push_and_branch() {
        let repo = parse_repo(
            &json!({"private": true, "default_branch": "trunk", "permissions": {"push": false}}),
        )
        .unwrap();
        assert!(repo.private && !repo.can_push);
        assert_eq!(repo.default_branch, "trunk");
        assert!(parse_repo(&json!({})).is_err());
    }

    #[test]
    fn names_and_paths_are_validated_before_any_request() {
        for (owner, repo) in [("", "r"), ("o", ".."), ("o/x", "r"), ("o", "r?x")] {
            assert!(GithubStore::new(owner, repo, None, "a.vaultsync", "t").is_err());
        }
        assert_eq!(
            GithubStore::new("o", "r", None, "a.vaultsync", " ")
                .err()
                .unwrap()
                .code,
            "SYNC_CREDENTIAL_MISSING"
        );
        let store = GithubStore::new("me", "my-sync", None, "vault/a b.vaultsync", "t").unwrap();
        assert_eq!(
            store.contents_url().as_str(),
            "https://api.github.com/repos/me/my-sync/contents/vault/a%20b.vaultsync"
        );
    }
}
