//! Console injection. A short-lived copy of this executable attaches to the
//! target console, checks that the program in front is really waiting for a
//! password (echo off, a password prompt at the cursor, not a shell), and
//! only then receives the value from the resident process and writes it as
//! key events into the console input buffer.
use std::{
    collections::HashMap,
    ffi::OsString,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    mem::ManuallyDrop,
    os::windows::io::FromRawHandle,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use vault_contracts::{
    errors::{AppError, Result},
    filling::{ConsoleRequest, FillStatus, host_allowed, prompt_host},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::{
            Console::{
                AttachConsole, CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, COORD, ENABLE_ECHO_INPUT,
                FreeConsole, GetConsoleMode, GetConsoleProcessList, GetConsoleScreenBufferInfo,
                GetStdHandle, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD,
                KEY_EVENT_RECORD_0, ReadConsoleOutputCharacterW, STD_INPUT_HANDLE,
                STD_OUTPUT_HANDLE, WriteConsoleInputW,
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        },
    },
    core::BOOL,
};
use zeroize::Zeroizing;

pub(crate) const FLAG: &str = "--console-fill";
const TEXT_LIMIT: usize = 16384;
const SHELLS: [&str; 7] = [
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "bash.exe",
    "sh.exe",
    "wsl.exe",
    "nu.exe",
];
const PASSWORD_WORDS: [&str; 8] = [
    "password",
    "passphrase",
    "passcode",
    "pin:",
    "token",
    "secret",
    "密码",
    "口令",
];
/// Terminal windows whose consoles live in other processes. Typing into them
/// would skip the password check, so they are refused instead of handed to
/// keystrokes; the caller should name the waiting program's processId.
const TERMINAL_HOSTS: [&str; 7] = [
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "tabby.exe",
];
/// Reasons the helper may report; anything else becomes `console_failed`.
const REASONS: [&str; 11] = [
    "terminal_window",
    "not_console",
    "window_not_found",
    "ambiguous_window",
    "echo_on",
    "no_password_prompt",
    "host_mismatch",
    "shell_in_front",
    "prompt_changed",
    "write_failed",
    "invalid_text",
];

fn refused(reason: &'static str) -> FillStatus {
    let mut status = FillStatus::status("unsupported_target");
    status.reason = Some(reason);
    status
}

/// Resident side. `resolve` runs only after the helper confirmed the console
/// waits for a password.
pub(crate) fn fill(
    request: ConsoleRequest,
    resolve: Box<dyn FnOnce() -> Result<String> + '_>,
) -> Result<FillStatus> {
    let failed = || AppError::new("CONSOLE_FAILED", "无法连接目标控制台。");
    if request.process_id == Some(0)
        || request
            .window_title
            .as_ref()
            .is_some_and(|t| t.trim().is_empty() || t.chars().any(char::is_control))
        || (request.process_id.is_none() && request.window_title.is_none())
    {
        return Err(AppError::invalid().with("target", "A console needs processId or windowTitle."));
    }
    let mut command = Command::new(std::env::current_exe().map_err(|_| failed())?);
    command.arg(FLAG);
    if let Some(pid) = request.process_id {
        command.arg("--pid").arg(pid.to_string());
    }
    if let Some(title) = &request.window_title {
        command.arg("--title").arg(title);
    }
    if request.submit {
        command.arg("--submit");
    }
    for host in &request.hosts {
        command.arg("--host").arg(host);
    }
    use std::os::windows::process::CommandExt;
    let mut child = command
        // DETACHED_PROCESS: the helper starts without a console of its own.
        .creation_flags(0x00000008)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failed())?;
    let stdout = child.stdout.take().ok_or_else(failed)?;
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let wait = |lines: &mpsc::Receiver<String>| lines.recv_timeout(Duration::from_secs(10)).ok();
    let outcome = |line: Option<String>| -> FillStatus {
        match line.as_deref() {
            Some("sent") => FillStatus::status("console_typed"),
            Some(line) => match line.strip_prefix("refuse ") {
                Some(reason) => refused(
                    REASONS
                        .iter()
                        .find(|r| **r == reason)
                        .copied()
                        .unwrap_or("console_failed"),
                ),
                None => refused("console_failed"),
            },
            None => refused("console_failed"),
        }
    };
    let first = wait(&lines);
    if first.as_deref() != Some("ready") {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(outcome(first));
    }
    let text = match resolve() {
        Ok(text) => Zeroizing::new(text),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    if text.is_empty() || text.len() > TEXT_LIMIT || text.chars().any(char::is_control) {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(refused("invalid_text"));
    }
    {
        let mut stdin = child.stdin.take().ok_or_else(failed)?;
        let mut payload = Zeroizing::new(Vec::with_capacity(text.len() + 1));
        payload.extend_from_slice(text.as_bytes());
        payload.push(b'\n');
        if stdin.write_all(&payload).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(refused("console_failed"));
        }
    }
    let result = outcome(wait(&lines));
    let _ = child.kill();
    let _ = child.wait();
    Ok(result)
}

struct Options {
    process_id: Option<u32>,
    title: Option<String>,
    submit: bool,
    hosts: Vec<String>,
}
fn options(args: &[OsString]) -> Option<Options> {
    let mut options = Options {
        process_id: None,
        title: None,
        submit: false,
        hosts: vec![],
    };
    let mut args = args.iter().map(|a| a.to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pid" => options.process_id = Some(args.next()?.parse().ok()?),
            "--title" => options.title = Some(args.next()?),
            "--submit" => options.submit = true,
            "--host" => options.hosts.push(args.next()?),
            _ => return None,
        }
    }
    (options.process_id.is_some() || options.title.is_some()).then_some(options)
}

struct TitleSearch {
    title: String,
    found: Vec<u32>,
}
unsafe extern "system" fn by_title(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let search = unsafe { &mut *(parameter.0 as *mut TitleSearch) };
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            let mut buffer = [0u16; 2048];
            let count = GetWindowTextW(hwnd, &mut buffer);
            if count > 0
                && String::from_utf16_lossy(&buffer[..count as usize])
                    .to_lowercase()
                    .contains(&search.title)
            {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid != 0 && !search.found.contains(&pid) {
                    search.found.push(pid);
                }
            }
        }
    }
    true.into()
}
fn process_names() -> HashMap<u32, String> {
    let mut names = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return names;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            names.insert(
                entry.th32ProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase(),
            );
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
    }
    names
}

/// The attached console's state, read without touching its input.
struct Console {
    input: File,
    output: File,
}
impl Console {
    fn open() -> Option<Self> {
        let open = |name: &str| {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(name)
                .ok()
        };
        Some(Self {
            input: open("CONIN$")?,
            output: open("CONOUT$")?,
        })
    }
    fn input(&self) -> windows::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        windows::Win32::Foundation::HANDLE(self.input.as_raw_handle())
    }
    fn output(&self) -> windows::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        windows::Win32::Foundation::HANDLE(self.output.as_raw_handle())
    }
    /// The text of the cursor line and the line above it.
    fn prompt(&self) -> Option<String> {
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        unsafe { GetConsoleScreenBufferInfo(self.output(), &mut info) }.ok()?;
        let width = info.dwSize.X.max(1) as usize;
        let row = info.dwCursorPosition.Y;
        let mut text = String::new();
        for y in [row - 1, row] {
            if y < 0 {
                continue;
            }
            let mut buffer = vec![0u16; width];
            let mut read = 0;
            unsafe {
                ReadConsoleOutputCharacterW(
                    self.output(),
                    &mut buffer,
                    COORD { X: 0, Y: y },
                    &mut read,
                )
            }
            .ok()?;
            let line = String::from_utf16_lossy(&buffer[..read as usize]);
            let line = if y == row {
                line.chars()
                    .take(info.dwCursorPosition.X.max(0) as usize)
                    .collect()
            } else {
                line
            };
            text.push_str(line.trim_end());
            text.push('\n');
        }
        Some(text)
    }
    fn echo_on(&self) -> bool {
        let mut mode = CONSOLE_MODE::default();
        unsafe { GetConsoleMode(self.input(), &mut mode) }.is_err()
            || mode.contains(ENABLE_ECHO_INPUT)
    }
    /// Whether every other process on the console is a shell: then nothing
    /// but the shell itself can be reading, and a shell line is no password.
    fn only_shells(&self) -> bool {
        let mut ids = vec![0u32; 64];
        let count = unsafe { GetConsoleProcessList(&mut ids) } as usize;
        if count == 0 || count > ids.len() {
            return true;
        }
        let me = std::process::id();
        let names = process_names();
        !ids[..count].iter().filter(|id| **id != me).any(|id| {
            names
                .get(id)
                .is_some_and(|name| !SHELLS.contains(&name.as_str()) && name != "conhost.exe")
        })
    }
    /// The first reason this console is not waiting for a password for `hosts`.
    fn check(&self, hosts: &[String]) -> std::result::Result<String, &'static str> {
        if self.echo_on() {
            return Err("echo_on");
        }
        let prompt = self.prompt().ok_or("not_console")?;
        let last = prompt
            .trim_end()
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let text = if last.trim().is_empty() {
            prompt.to_lowercase()
        } else {
            last
        };
        if !PASSWORD_WORDS.iter().any(|w| text.contains(w)) {
            return Err("no_password_prompt");
        }
        if let Some(host) = prompt_host(&text)
            && !host_allowed(&host, hosts)
        {
            return Err("host_mismatch");
        }
        if self.only_shells() {
            return Err("shell_in_front");
        }
        Ok(text)
    }
}
fn key(unit: u16, virtual_key: u16, down: bool) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: down.into(),
                wRepeatCount: 1,
                wVirtualKeyCode: virtual_key,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                dwControlKeyState: 0,
            },
        },
    }
}

/// Helper mode entry: `--console-fill (--pid N | --title T) [--submit] [--host H]...`.
pub(crate) fn helper(args: &[OsString]) -> i32 {
    // Keep the pipes to the resident process; attaching a console replaces
    // the standard handles.
    let (Ok(input), Ok(output)) = (unsafe { GetStdHandle(STD_INPUT_HANDLE) }, unsafe {
        GetStdHandle(STD_OUTPUT_HANDLE)
    }) else {
        return 1;
    };
    let mut from_resident = ManuallyDrop::new(unsafe { File::from_raw_handle(input.0) });
    let mut to_resident = ManuallyDrop::new(unsafe { File::from_raw_handle(output.0) });
    let mut say = |line: &str| {
        let _ = to_resident.write_all(format!("{line}\n").as_bytes());
        let _ = to_resident.flush();
    };
    let Some(options) = options(args) else {
        say("refuse not_console");
        return 1;
    };
    let process_id = match (options.process_id, &options.title) {
        (Some(pid), _) => pid,
        (None, Some(title)) => {
            let mut search = TitleSearch {
                title: title.to_lowercase(),
                found: vec![],
            };
            let _ = unsafe {
                EnumWindows(
                    Some(by_title),
                    LPARAM(&mut search as *mut TitleSearch as isize),
                )
            };
            match search.found.as_slice() {
                [one] => *one,
                [] => {
                    say("refuse window_not_found");
                    return 1;
                }
                _ => {
                    say("refuse ambiguous_window");
                    return 1;
                }
            }
        }
        (None, None) => {
            say("refuse not_console");
            return 1;
        }
    };
    let _ = unsafe { FreeConsole() };
    if unsafe { AttachConsole(process_id) }.is_err() {
        let terminal = process_names()
            .get(&process_id)
            .is_some_and(|name| TERMINAL_HOSTS.contains(&name.as_str()));
        say(if terminal {
            "refuse terminal_window"
        } else {
            "refuse not_console"
        });
        return 1;
    }
    let Some(console) = Console::open() else {
        say("refuse not_console");
        return 1;
    };
    let before = match console.check(&options.hosts) {
        Ok(prompt) => prompt,
        Err(reason) => {
            say(&format!("refuse {reason}"));
            return 1;
        }
    };
    say("ready");
    let mut secret = Zeroizing::new(Vec::new());
    if (&mut *from_resident)
        .take(TEXT_LIMIT as u64 + 2)
        .read_to_end(&mut secret)
        .is_err()
    {
        say("refuse write_failed");
        return 1;
    }
    if secret.last() == Some(&b'\n') {
        secret.pop();
    }
    let Ok(text) = std::str::from_utf8(&secret) else {
        say("refuse invalid_text");
        return 1;
    };
    if text.is_empty() || text.len() > TEXT_LIMIT || text.chars().any(char::is_control) {
        say("refuse invalid_text");
        return 1;
    }
    // The console may have moved on while the value travelled.
    if console.check(&options.hosts).as_deref() != Ok(before.as_str()) {
        say("refuse prompt_changed");
        return 1;
    }
    let mut records = Vec::with_capacity(text.len() * 2 + 2);
    for unit in text.encode_utf16() {
        records.push(key(unit, 0, true));
        records.push(key(unit, 0, false));
    }
    if options.submit {
        records.push(key(u16::from(b'\r'), 0x0D, true));
        records.push(key(u16::from(b'\r'), 0x0D, false));
    }
    let mut written = 0;
    let ok = unsafe { WriteConsoleInputW(console.input(), &records, &mut written) }.is_ok()
        && written as usize == records.len();
    // INPUT_RECORD carries the value; clear it before the memory is released.
    unsafe { std::ptr::write_bytes(records.as_mut_ptr(), 0, records.len()) };
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    drop(records);
    say(if ok { "sent" } else { "refuse write_failed" });
    if ok { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_options_need_a_target_and_reject_unknown_flags() {
        let parse = |a: &[&str]| options(&a.iter().map(OsString::from).collect::<Vec<_>>());
        assert!(
            parse(&["--pid", "42", "--submit", "--host", "a.com"])
                .is_some_and(|o| o.process_id == Some(42) && o.submit && o.hosts == ["a.com"])
        );
        assert!(parse(&["--title", "ssh"]).is_some());
        assert!(parse(&["--submit"]).is_none());
        assert!(parse(&["--pid", "x"]).is_none());
        assert!(parse(&["--pid", "1", "--evil"]).is_none());
    }
}
