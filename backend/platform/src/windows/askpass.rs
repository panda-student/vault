//! Askpass channel. git, ssh and similar tools started below a command vault
//! runs ask vault itself for the account or password. The helper is this
//! executable; the answer travels over a same-user pipe and only to a copy of
//! this executable started below the bound process.
use std::{
    collections::HashMap,
    ffi::OsString,
    fs::File,
    io::{self, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use vault_contracts::{
    errors::{AppError, Result},
    filling::{AskpassAnswers, AskpassSession, host_allowed, prompt_host},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};
use zeroize::Zeroizing;

const ENV_PIPE: &str = "VAULT_ASKPASS_PIPE";
const ENV_NONCE: &str = "VAULT_ASKPASS_NONCE";
const PREFIX: &str = r"\\.\pipe\vault-askpass-";
/// A command needs a handful of prompts at most; a loop asking again is refused.
const LIMIT: usize = 8;
const REQUEST_LIMIT: usize = 4096;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Full image path of a same-user process, or `None`.
pub(super) fn image(pid: u32) -> Option<PathBuf> {
    unsafe {
        let process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        if !super::pipes::same_user_process(process.0).ok()? {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(
            &path[..length as usize],
        )))
    }
}
fn is_this_executable(pid: u32, executable: &PathBuf) -> bool {
    image(pid)
        .and_then(|p| std::fs::canonicalize(p).ok())
        .is_some_and(|p| &p == executable)
}
/// Whether `ancestor` is `pid` or one of its parents.
fn descends_from(pid: u32, ancestor: u32) -> bool {
    let parents = unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let snapshot = Handle(snapshot);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut parents = HashMap::new();
        let mut more = Process32FirstW(snapshot.0, &mut entry).is_ok();
        while more {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            more = Process32NextW(snapshot.0, &mut entry).is_ok();
        }
        parents
    };
    let mut current = pid;
    for _ in 0..32 {
        if current == ancestor {
            return true;
        }
        match parents.get(&current) {
            Some(&parent) if parent != 0 && parent != current => current = parent,
            _ => return false,
        }
    }
    false
}

/// What to answer for `prompt`: the account for a user-name prompt, the
/// secret for a password prompt, nothing for anything else (host-key
/// questions, prompts naming another host).
pub(crate) fn decide<'a>(prompt: &str, answers: &'a AskpassAnswers) -> Option<&'a str> {
    let lower = prompt.to_lowercase();
    if let Some(host) = prompt_host(prompt)
        && !host_allowed(&host, &answers.hosts)
    {
        return None;
    }
    if ["yes/no", "fingerprint", "continue connecting"]
        .iter()
        .any(|w| lower.contains(w))
    {
        return None;
    }
    let first = |words: &[&str]| words.iter().filter_map(|w| lower.find(w)).min();
    let user = first(&["username", "user name", "用户名", "账号"]);
    let secret = first(&[
        "password",
        "passphrase",
        "passcode",
        "token",
        "pin",
        "密码",
        "口令",
    ]);
    match (user, secret) {
        (Some(u), Some(s)) if u < s => answers.account.as_deref().map(String::as_str),
        (Some(_), None) => answers.account.as_deref().map(String::as_str),
        (_, Some(_)) => Some(answers.secret.as_str()),
        (None, None) => None,
    }
}

fn instance(name: &str, first: bool) -> io::Result<File> {
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;SY)(A;;GA;;;{})", super::pipes::current_sid()?)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            1,
            &mut descriptor,
            None,
        )
        .map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mode = if first {
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            PIPE_ACCESS_DUPLEX
        };
        let handle = CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            Some(&attributes),
        );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        if handle.is_invalid() {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from_raw_handle(handle.0))
    }
}

struct Shared {
    answers: AskpassAnswers,
    executable: PathBuf,
    nonce: String,
    bound: AtomicU32,
    answered: AtomicUsize,
    asked: AtomicUsize,
    stop: AtomicBool,
}
pub(crate) struct Session {
    name: String,
    shared: Arc<Shared>,
}
impl Session {
    pub(crate) fn open(answers: AskpassAnswers) -> Result<Self> {
        let failed = || AppError::new("ASKPASS_FAILED", "无法建立本机密码应答通道。");
        let executable = std::env::current_exe()
            .and_then(std::fs::canonicalize)
            .map_err(|_| failed())?;
        let name = format!("{PREFIX}{}", uuid::Uuid::new_v4().simple());
        let first = instance(&name, true).map_err(|_| failed())?;
        let shared = Arc::new(Shared {
            answers,
            executable,
            nonce: uuid::Uuid::new_v4().simple().to_string(),
            bound: AtomicU32::new(0),
            answered: AtomicUsize::new(0),
            asked: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        let worker = shared.clone();
        let pipe_name = name.clone();
        std::thread::Builder::new()
            .name("vault-askpass".into())
            .spawn(move || listen(worker, pipe_name, first))
            .map_err(|_| failed())?;
        Ok(Self { name, shared })
    }
}
impl AskpassSession for Session {
    fn env(&self) -> Vec<(String, String)> {
        let program = self.shared.executable.to_string_lossy().into_owned();
        // canonicalize() yields a verbatim path; tools spawn it more reliably plain.
        let program = program
            .strip_prefix(r"\\?\")
            .map(str::to_owned)
            .unwrap_or(program);
        vec![
            ("GIT_ASKPASS".into(), program.clone()),
            ("SSH_ASKPASS".into(), program.clone()),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            ("SUDO_ASKPASS".into(), program),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (ENV_PIPE.into(), self.name.clone()),
            (ENV_NONCE.into(), self.shared.nonce.clone()),
        ]
    }
    fn bind(&self, process_id: u32) {
        self.shared.bound.store(process_id, Ordering::SeqCst);
    }
    fn answered(&self) -> usize {
        self.shared.answered.load(Ordering::SeqCst)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Wake a listener blocked waiting for a client; it then sees `stop`.
        let _ = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.name);
    }
}
fn listen(shared: Arc<Shared>, name: String, first: File) {
    let mut next = Some(first);
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let pipe = match next.take() {
            Some(pipe) => pipe,
            None => match instance(&name, false) {
                Ok(pipe) => pipe,
                Err(_) => return,
            },
        };
        if let Err(error) = unsafe { ConnectNamedPipe(HANDLE(pipe.as_raw_handle()), None) }
            && error.code() != ERROR_PIPE_CONNECTED.to_hresult()
        {
            continue;
        }
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let _ = serve(&shared, pipe);
    }
}
fn verified(shared: &Shared, pipe: &File) -> bool {
    let bound = shared.bound.load(Ordering::SeqCst);
    let mut pid = 0;
    bound != 0
        && unsafe { GetNamedPipeClientProcessId(HANDLE(pipe.as_raw_handle()), &mut pid) }.is_ok()
        && is_this_executable(pid, &shared.executable)
        && descends_from(pid, bound)
}
fn serve(shared: &Shared, mut pipe: File) -> io::Result<()> {
    let reply = if verified(shared, &pipe) {
        let mut length = [0u8; 4];
        pipe.read_exact(&mut length)?;
        let length = u32::from_le_bytes(length) as usize;
        if length > REQUEST_LIMIT {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut body = vec![0u8; length];
        pipe.read_exact(&mut body)?;
        let text = String::from_utf8(body).map_err(|_| io::ErrorKind::InvalidData)?;
        let (nonce, prompt) = text.split_once('\n').unwrap_or(("", ""));
        if nonce == shared.nonce && shared.asked.fetch_add(1, Ordering::SeqCst) < LIMIT {
            decide(prompt, &shared.answers)
        } else {
            None
        }
    } else {
        None
    };
    match reply {
        Some(value) => {
            let mut out = Zeroizing::new(Vec::with_capacity(value.len() + 1));
            out.push(b'1');
            out.extend_from_slice(value.as_bytes());
            pipe.write_all(&out)?;
            shared.answered.fetch_add(1, Ordering::SeqCst);
        }
        None => pipe.write_all(b"0")?,
    }
    // Wait until the helper read the reply; closing first could discard it.
    pipe.sync_all()
}

/// Helper mode: git or ssh started this executable with one prompt argument.
/// Prints the answer on stdout and exits 0, or exits 1 when refused.
pub(crate) fn client(prompt: &OsString) -> i32 {
    let (Ok(name), Ok(nonce)) = (std::env::var(ENV_PIPE), std::env::var(ENV_NONCE)) else {
        return 1;
    };
    if !name.starts_with(PREFIX) || name.len() > 128 || nonce.contains('\n') {
        return 1;
    }
    let Ok(executable) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return 1;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut pipe = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
        {
            Ok(pipe) => break pipe,
            // ERROR_PIPE_BUSY: another prompt is being answered.
            Err(error) if error.raw_os_error() == Some(231) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return 1,
        }
    };
    let mut server = 0;
    if unsafe { GetNamedPipeServerProcessId(HANDLE(pipe.as_raw_handle()), &mut server) }.is_err()
        || !is_this_executable(server, &executable)
    {
        return 1;
    }
    let prompt = prompt.to_string_lossy();
    let body = format!("{nonce}\n{prompt}");
    if body.len() > REQUEST_LIMIT {
        return 1;
    }
    let mut request = (body.len() as u32).to_le_bytes().to_vec();
    request.extend_from_slice(body.as_bytes());
    if pipe.write_all(&request).is_err() {
        return 1;
    }
    let mut reply = Zeroizing::new(Vec::new());
    if pipe.take(65536).read_to_end(&mut reply).is_err() || reply.first() != Some(&b'1') {
        return 1;
    }
    let mut stdout = io::stdout().lock();
    if stdout.write_all(&reply[1..]).is_err()
        || stdout.write_all(b"\n").is_err()
        || stdout.flush().is_err()
    {
        return 1;
    }
    0
}
/// Whether this process was started as an askpass helper.
pub(crate) fn is_client(args: &[OsString]) -> bool {
    args.len() == 1
        && !args[0].to_string_lossy().starts_with("--")
        && std::env::var_os(ENV_PIPE).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn answers(hosts: &[&str]) -> AskpassAnswers {
        AskpassAnswers {
            account: Some(Zeroizing::new("alice".into())),
            secret: Zeroizing::new("s3cret".into()),
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
        }
    }
    #[test]
    fn answers_only_matching_prompts() {
        let a = answers(&["gitlab.example.com"]);
        assert_eq!(
            decide("Username for 'https://gitlab.example.com': ", &a),
            Some("alice")
        );
        assert_eq!(
            decide("Password for 'https://alice@gitlab.example.com': ", &a),
            Some("s3cret")
        );
        assert_eq!(
            decide("alice@gitlab.example.com's password: ", &a),
            Some("s3cret")
        );
        assert_eq!(
            decide("Password for 'https://evil.example.org': ", &a),
            None
        );
        assert_eq!(
            decide(
                "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
                &a
            ),
            None
        );
        assert_eq!(
            decide("Enter passphrase for key 'id_ed25519': ", &a),
            Some("s3cret")
        );
        assert_eq!(decide("Continue? ", &a), None);
    }
    #[test]
    fn a_prompt_naming_a_host_needs_a_known_host() {
        assert_eq!(
            decide("Password for 'https://x.example.com': ", &answers(&[])),
            None
        );
        assert_eq!(decide("Password: ", &answers(&[])), Some("s3cret"));
    }
    #[test]
    fn only_a_single_prompt_argument_is_helper_mode() {
        assert!(!is_client(&["--mcp".into()]));
        assert!(!is_client(&["a".into(), "b".into()]));
    }
}
