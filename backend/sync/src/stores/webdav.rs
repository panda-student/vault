//! WebDAV (Nutstore, Nextcloud, Synology, rclone, alist and similar).
use super::http::{body, client, network, send, status_error};
use super::{remote_changed, segments};
use crate::sha_hex;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{ETAG, HeaderMap, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Method, StatusCode, Url};
use vault_contracts::{
    errors::{AppError, Result},
    ports::sync::{RemoteObject, RemoteState, RemoteStore},
};
use zeroize::Zeroizing;

pub struct WebdavStore {
    client: Client,
    base: Url,
    segments: Vec<String>,
    username: String,
    password: Zeroizing<String>,
}

/// Versions carry their kind so a server without ETags still gets a check.
const ETAG_PREFIX: &str = "etag:";
const HASH_PREFIX: &str = "sha:";

impl WebdavStore {
    pub fn new(url: &str, path: &str, username: &str, password: &str) -> Result<Self> {
        let base = Url::parse(url.trim()).map_err(|_| bad_url())?;
        let local = matches!(base.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(base.scheme() == "https" || (base.scheme() == "http" && local))
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
        {
            return Err(bad_url());
        }
        Ok(Self {
            client: client()?,
            base,
            segments: segments(path)?,
            username: username.into(),
            password: Zeroizing::new(password.into()),
        })
    }

    /// URL of the sync file, or of its `depth`-th parent collection.
    fn url(&self, parts: &[String], collection: bool) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(parts);
            if collection {
                path.push("");
            }
        }
        url
    }

    fn request(&self, method: Method, url: Url) -> reqwest::blocking::RequestBuilder {
        self.client
            .request(method, url)
            .basic_auth(&self.username, Some(self.password.as_str()))
    }

    fn file_url(&self) -> Url {
        self.url(&self.segments, false)
    }
}

impl WebdavStore {
    /// Version from response headers: a strong ETag when present, otherwise the
    /// content hash. Weak ETags (`W/"..."`) never match `If-Match`, so skip them.
    fn version_of(headers: &HeaderMap, bytes: &[u8]) -> String {
        match headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .filter(|etag| !etag.starts_with("W/"))
        {
            Some(etag) => format!("{ETAG_PREFIX}{etag}"),
            None => format!("{HASH_PREFIX}{}", sha_hex(bytes)),
        }
    }

    /// The PUT request carrying the precondition for `expected`.
    fn conditional_put(&self, expected: Option<&str>) -> Result<RequestBuilder> {
        let request = self.request(Method::PUT, self.file_url());
        Ok(match expected {
            None => request.header(IF_NONE_MATCH, "*"),
            Some(version) => match version.strip_prefix(ETAG_PREFIX) {
                Some(etag) => request.header(IF_MATCH, etag),
                // No usable ETag: compare content right before writing. Best
                // effort, a concurrent write in between is not detected.
                None => {
                    if self.read()?.map(|o| o.version).as_deref() != Some(version) {
                        return Err(remote_changed());
                    }
                    request
                }
            },
        })
    }

    /// Check every folder above the sync file; with `create`, make missing ones.
    fn ensure_collections(&self, create: bool) -> Result<bool> {
        let propfind = Method::from_bytes(b"PROPFIND").map_err(|_| network())?;
        let mkcol = Method::from_bytes(b"MKCOL").map_err(|_| network())?;
        let mut created = false;
        for depth in 0..self.segments.len() {
            let url = self.url(&self.segments[..depth], true);
            let response = send(
                self.request(propfind.clone(), url.clone())
                    .header("Depth", "0"),
            )?;
            match response.status() {
                status if status.is_success() => continue,
                StatusCode::NOT_FOUND if create && depth > 0 => {
                    let made = send(self.request(mkcol.clone(), url))?;
                    if !made.status().is_success() {
                        return Err(status_error(made.status()));
                    }
                    created = true;
                }
                status => return Err(status_error(status)),
            }
        }
        Ok(created)
    }

    fn file_exists(&self) -> Result<bool> {
        let head = send(self.request(Method::HEAD, self.file_url()))?;
        match head.status() {
            StatusCode::NOT_FOUND => Ok(false),
            status if status.is_success() => Ok(true),
            status => Err(status_error(status)),
        }
    }
}

impl RemoteStore for WebdavStore {
    fn read(&self) -> Result<Option<RemoteObject>> {
        let response = send(self.request(Method::GET, self.file_url()))?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => {
                let headers = response.headers().clone();
                let bytes = body(response)?;
                let version = Self::version_of(&headers, &bytes);
                Ok(Some(RemoteObject { bytes, version }))
            }
            status => Err(status_error(status)),
        }
    }

    fn write(&self, bytes: &[u8], expected: Option<&str>) -> Result<String> {
        let response = send(self.conditional_put(expected)?.body(bytes.to_vec()))?;
        match response.status() {
            StatusCode::PRECONDITION_FAILED => return Err(remote_changed()),
            // 409 on PUT means a missing parent folder.
            StatusCode::CONFLICT => return Err(status_error(StatusCode::NOT_FOUND)),
            status if !status.is_success() => return Err(status_error(status)),
            _ => {}
        }
        // Many servers omit the ETag on PUT: ask with HEAD so the version has
        // the same form as the next read.
        let head = send(self.request(Method::HEAD, self.file_url()))?;
        if !head.status().is_success() {
            return Err(status_error(head.status()));
        }
        Ok(Self::version_of(head.headers(), bytes))
    }

    fn prepare(&self, create: bool) -> Result<RemoteState> {
        let created = self.ensure_collections(create)?;
        Ok(RemoteState {
            exists: self.file_exists()?,
            created,
        })
    }
}

fn bad_url() -> AppError {
    AppError::invalid().with(
        "url",
        "WebDAV 地址须为 https://（本机测试可用 http://localhost），且不能包含用户名、密码或查询参数。",
    )
}

#[cfg(test)]
mod tests;
