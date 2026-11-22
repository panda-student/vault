//! Fills a field of a page in a local Chromium-based browser (Edge, Chrome)
//! started with `--remote-debugging-port`. vault itself talks to the browser's
//! DevTools endpoint on this computer, checks that the selected field is a
//! focused editable element of the page, then types the value there. The value
//! never passes through the caller.
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};
use tungstenite::{Message, WebSocket};
use url::Url;
use vault_contracts::errors::{AppError, Result};
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(10);

fn unavailable() -> AppError {
    AppError::new(
        "BROWSER_UNAVAILABLE",
        "连不上本机浏览器的调试端口。请用 --remote-debugging-port 启动 Edge/Chrome，并传入同一个端口。",
    )
}

fn failed() -> AppError {
    AppError::new(
        "BROWSER_FILL_FAILED",
        "浏览器没有完成填写，页面内容未返回。",
    )
}

#[derive(Clone)]
struct Page {
    id: String,
    socket: String,
}

/// Lists the browser's pages; exactly one must match.
fn page(port: u16, target_id: Option<&str>) -> Result<Page> {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| unavailable())?;
    let list: Vec<Value> = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(unavailable)?;
    let pages: Vec<Page> = list
        .iter()
        .filter(|item| item["type"] == "page")
        .filter(|item| target_id.is_none_or(|id| item["id"] == id))
        .filter_map(|item| {
            Some(Page {
                id: item["id"].as_str()?.to_owned(),
                socket: item["webSocketDebuggerUrl"].as_str()?.to_owned(),
            })
        })
        .collect();
    match pages.as_slice() {
        [page] => Ok(page.clone()),
        [] => Err(AppError::new(
            "PAGE_NOT_FOUND",
            "浏览器里没有找到页面（或 targetId 不对）。",
        )),
        _ => Err(AppError::new(
            "PAGE_AMBIGUOUS",
            "浏览器里打开了多个页面，请传入 targetId 指定其中一个。",
        )
        .with(
            "targetId",
            "可从 http://127.0.0.1:<端口>/json/list 的 id 取得。",
        )),
    }
}

struct DevTools {
    socket: WebSocket<TcpStream>,
    next: u64,
}

impl DevTools {
    /// Only the browser's own endpoint on this computer, on the given port.
    fn connect(port: u16, address: &str) -> Result<Self> {
        let url = Url::parse(address).map_err(|_| unavailable())?;
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
            None => false,
        };
        if url.scheme() != "ws" || !loopback || url.port() != Some(port) {
            return Err(unavailable());
        }
        let ip = match url.host() {
            Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip),
            _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
        };
        let stream = TcpStream::connect_timeout(&SocketAddr::new(ip, port), TIMEOUT)
            .map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(TIMEOUT))
            .and_then(|_| stream.set_write_timeout(Some(TIMEOUT)))
            .map_err(|_| unavailable())?;
        let (socket, _) = tungstenite::client(url.as_str(), stream).map_err(|_| unavailable())?;
        Ok(Self { socket, next: 0 })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let message =
            Zeroizing::new(json!({"id": id, "method": method, "params": params}).to_string());
        self.socket
            .send(Message::text(message.as_str()))
            .map_err(|_| failed())?;
        loop {
            match self.socket.read().map_err(|_| failed())? {
                Message::Text(text) => {
                    let reply: Value = serde_json::from_str(text.as_str()).map_err(|_| failed())?;
                    if reply["id"] != id {
                        continue;
                    }
                    if reply.get("error").is_some() {
                        return Err(failed());
                    }
                    return Ok(reply["result"].clone());
                }
                Message::Close(_) => return Err(failed()),
                _ => {}
            }
        }
    }

    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let result = self.call(
            "Runtime.evaluate",
            json!({"expression": expression, "returnByValue": true, "awaitPromise": false}),
        )?;
        if result.get("exceptionDetails").is_some() {
            return Err(failed());
        }
        Ok(result["result"]["value"].clone())
    }
}

fn focus_script(selector: &str) -> Result<String> {
    let selector = serde_json::to_string(selector).map_err(|_| AppError::invalid())?;
    Ok(format!(
        r#"(() => {{
  const el = document.querySelector({selector});
  if (!el) return {{origin: location.origin, found: false}};
  const editable = (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el.isContentEditable)
    && !el.disabled && !el.readOnly;
  if (!editable) return {{origin: location.origin, found: true, editable: false}};
  el.focus();
  if (typeof el.select === "function") el.select();
  return {{origin: location.origin, found: true, editable: true, focused: document.activeElement === el}};
}})()"#
    ))
}

const CHANGE_SCRIPT: &str = r#"(() => {
  const el = document.activeElement;
  if (el) el.dispatchEvent(new Event("change", {bubbles: true}));
  return {origin: location.origin};
})()"#;

pub(crate) fn fill(
    port: u16,
    selector: &str,
    target_id: Option<&str>,
    resolve: impl FnOnce() -> Result<String>,
) -> Result<Value> {
    if selector.trim().is_empty() || selector.len() > 1024 {
        return Err(AppError::invalid().with("selector", "请填写输入框的 CSS 选择器。"));
    }
    let page = page(port, target_id)?;
    let mut tools = DevTools::connect(port, &page.socket)?;
    let state = tools.evaluate(&focus_script(selector)?)?;
    if state["found"] != true {
        return Err(AppError::new(
            "FIELD_NOT_FOUND",
            "页面上没有找到该选择器对应的元素。",
        ));
    }
    if state["editable"] != true || state["focused"] != true {
        return Err(AppError::new(
            "FIELD_NOT_EDITABLE",
            "选择器对应的元素不是可编辑的输入框，或无法获得焦点。",
        ));
    }
    let value = Zeroizing::new(resolve()?);
    tools.call("Input.insertText", json!({"text": value.as_str()}))?;
    drop(value);
    tools.evaluate(CHANGE_SCRIPT)?;
    Ok(json!({"status": "input_sent", "targetId": page.id}))
}
