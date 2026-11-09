//! Exercises the WebDAV store against a tiny in-process WebDAV server.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Dav {
    files: HashMap<String, (Vec<u8>, u32)>,
    dirs: HashSet<String>,
    etags: bool,
    weak: bool,
    paths: Vec<String>,
}

/// Serve requests until the test ends; returns the base URL.
fn serve(etags: bool) -> (String, Arc<Mutex<Dav>>) {
    serve_with(etags, false)
}

fn serve_with(etags: bool, weak: bool) -> (String, Arc<Mutex<Dav>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = Arc::new(Mutex::new(Dav {
        etags,
        weak,
        dirs: HashSet::from(["/dav".to_string()]),
        ..Default::default()
    }));
    state
        .lock()
        .unwrap()
        .files
        .insert("/dav/other.txt".into(), (b"keep me".to_vec(), 1));
    let shared = state.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or_default().to_string();
            let path = parts
                .next()
                .unwrap_or_default()
                .trim_end_matches('/')
                .to_string();
            let mut headers = HashMap::new();
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                if let Some((k, v)) = header.split_once(':') {
                    headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                }
            }
            let length: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let reply = handle(&shared, &method, &path, &headers, body);
            let _ = stream.write_all(&reply);
        }
    });
    (format!("http://127.0.0.1:{port}/dav/"), state)
}

fn handle(
    state: &Mutex<Dav>,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: Vec<u8>,
) -> Vec<u8> {
    let mut dav = state.lock().unwrap();
    dav.paths.push(path.to_string());
    let auth = format!("Basic {}", STANDARD.encode("me:secret"));
    if headers.get("authorization") != Some(&auth) {
        return response(401, None, b"");
    }
    let weak = dav.weak;
    let etag_of = move |n: u32| format!("{}\"{n}\"", if weak { "W/" } else { "" });
    match method {
        "PROPFIND" if dav.dirs.contains(path) => response(207, None, b""),
        "PROPFIND" => response(404, None, b""),
        "MKCOL" => {
            dav.dirs.insert(path.to_string());
            response(201, None, b"")
        }
        "GET" | "HEAD" => match dav.files.get(path) {
            Some((bytes, n)) => {
                let etag = dav.etags.then(|| etag_of(*n));
                let content: &[u8] = if method == "GET" { bytes } else { b"" };
                response(200, etag.as_deref(), content)
            }
            None => response(404, None, b""),
        },
        "PUT" => {
            let current = dav.files.get(path).map(|(_, n)| *n);
            let blocked = match (headers.get("if-none-match"), headers.get("if-match")) {
                (Some(_), _) => current.is_some(),
                (_, Some(expected)) => current.map(etag_of).as_ref() != Some(expected),
                _ => false,
            };
            if blocked {
                return response(412, None, b"");
            }
            let next = current.unwrap_or(0) + 1;
            dav.files.insert(path.to_string(), (body, next));
            let etag = dav.etags.then(|| etag_of(next));
            response(201, etag.as_deref(), b"")
        }
        _ => response(405, None, b""),
    }
}

fn response(status: u16, etag: Option<&str>, body: &[u8]) -> Vec<u8> {
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(etag) = etag {
        head.push_str(&format!("ETag: {etag}\r\n"));
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

fn store(url: &str, password: &str) -> WebdavStore {
    WebdavStore::new(url, "vault/s.vaultsync", "me", password).unwrap()
}

#[test]
fn creates_folders_and_writes_with_etag_checks() {
    let (url, state) = serve(true);
    let dav = store(&url, "secret");
    assert_eq!(dav.prepare(false).unwrap_err().code, "SYNC_NOT_FOUND");
    assert_eq!(
        dav.prepare(true).unwrap(),
        RemoteState {
            exists: false,
            created: true
        }
    );
    assert!(dav.read().unwrap().is_none());
    let first = dav.write(b"one", None).unwrap();
    assert!(first.starts_with(ETAG_PREFIX));
    assert_eq!(
        dav.write(b"x", None).unwrap_err().code,
        "SYNC_REMOTE_CHANGED"
    );
    let second = dav.write(b"two", Some(&first)).unwrap();
    assert_eq!(
        dav.write(b"three", Some(&first)).unwrap_err().code,
        "SYNC_REMOTE_CHANGED"
    );
    let read = dav.read().unwrap().unwrap();
    assert_eq!((read.bytes.as_slice(), read.version), (&b"two"[..], second));
    assert!(dav.prepare(false).unwrap().exists);
    // Only the sync file and its folders were ever requested.
    let dav = state.lock().unwrap();
    assert!(dav.paths.iter().all(|p| p != "/dav/other.txt"));
    assert_eq!(dav.files["/dav/other.txt"].0, b"keep me");
}

#[test]
fn servers_without_etags_fall_back_to_content_checks() {
    let (url, _) = serve(false);
    let dav = store(&url, "secret");
    dav.prepare(true).unwrap();
    let first = dav.write(b"one", None).unwrap();
    assert!(first.starts_with(HASH_PREFIX));
    assert_eq!(dav.read().unwrap().unwrap().version, first);
    let second = dav.write(b"two", Some(&first)).unwrap();
    assert_eq!(
        dav.write(b"three", Some(&first)).unwrap_err().code,
        "SYNC_REMOTE_CHANGED"
    );
    assert_eq!(dav.read().unwrap().unwrap().version, second);
}

#[test]
fn wrong_password_is_an_auth_error() {
    let (url, _) = serve(true);
    assert_eq!(store(&url, "wrong").read().unwrap_err().code, "SYNC_AUTH");
}

#[test]
fn only_https_or_local_http_without_credentials_in_the_url() {
    for bad in [
        "http://example.com/dav/",
        "https://user:pw@example.com/dav/",
        "https://example.com/dav/?token=1",
        "ftp://example.com/",
        "not a url",
    ] {
        assert!(
            WebdavStore::new(bad, "a.vaultsync", "u", "p").is_err(),
            "{bad}"
        );
    }
    assert!(WebdavStore::new("https://dav.jianguoyun.com/dav/", "a.vaultsync", "u", "p").is_ok());
}

#[test]
fn weak_etags_are_not_used_for_preconditions() {
    let (url, _) = serve_with(true, true);
    let dav = store(&url, "secret");
    dav.prepare(true).unwrap();
    let first = dav.write(b"one", None).unwrap();
    assert!(first.starts_with(HASH_PREFIX), "{first}");
    assert_eq!(dav.read().unwrap().unwrap().version, first);
    // Without the fix every later write would fail with a 412.
    let second = dav.write(b"two", Some(&first)).unwrap();
    assert_eq!(dav.read().unwrap().unwrap().version, second);
}

#[test]
fn versions_after_writing_match_the_next_read() {
    let (url, _) = serve(true);
    let dav = store(&url, "secret");
    dav.prepare(true).unwrap();
    let written = dav.write(b"one", None).unwrap();
    assert_eq!(dav.read().unwrap().unwrap().version, written);
}
