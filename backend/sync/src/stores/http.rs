//! Shared HTTP client and error mapping. Errors carry fixed messages only:
//! never a token, a URL with credentials or a response body.
use super::{MAX_REMOTE_BYTES, too_large};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use std::time::Duration;
use vault_contracts::errors::{AppError, Result};

pub(crate) fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("vault-sync/", env!("CARGO_PKG_VERSION")))
        // Never follow a redirect: it could carry credentials to another address
        // or downgrade to plain http.
        .redirect(Policy::none())
        .build()
        .map_err(|_| network())
}

pub(crate) fn send(request: reqwest::blocking::RequestBuilder) -> Result<Response> {
    request.send().map_err(|_| network())
}

/// Map an unexpected status to a user-facing error. Conflict statuses are
/// interpreted by each write path, not here.
pub(crate) fn status_error(status: StatusCode) -> AppError {
    match status.as_u16() {
        300..=399 => AppError::new(
            "SYNC_REDIRECT",
            "远端要求跳转到其他地址，为安全起见未跟随。请填写最终的地址。",
        ),
        401 => AppError::new(
            "SYNC_AUTH",
            "远端拒绝了登录，请检查同步引用的凭据是否正确或已过期。",
        ),
        403 => AppError::new("SYNC_FORBIDDEN", "当前凭据没有访问或写入该位置的权限。"),
        404 => AppError::new("SYNC_NOT_FOUND", "远端位置不存在，请检查地址或仓库名。"),
        413 => AppError::new("SYNC_TOO_LARGE", "远端拒绝了过大的同步文件。"),
        429 => rate_limited(),
        _ => AppError::new("SYNC_UNAVAILABLE", "远端暂时无法处理请求，请稍后再同步。"),
    }
}

pub(crate) fn rate_limited() -> AppError {
    AppError::new("SYNC_UNAVAILABLE", "远端请求过于频繁，请稍后再同步。")
}

pub(crate) fn network() -> AppError {
    AppError::new("SYNC_NETWORK", "无法连接远端，请检查网络或地址。")
}

pub(crate) fn body(response: Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REMOTE_BYTES)
    {
        return Err(too_large());
    }
    let bytes = response.bytes().map_err(|_| network())?;
    if bytes.len() as u64 > MAX_REMOTE_BYTES {
        return Err(too_large());
    }
    Ok(bytes.to_vec())
}
