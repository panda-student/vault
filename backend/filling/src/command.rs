//! Programs vault runs itself. The caller writes placeholders such as
//! `{{password}}` in the arguments, environment values or standard input; vault
//! puts the stored values in, runs the program, waits for it and returns what it
//! printed with every injected value masked. The secret never passes through
//! the caller, so this works the same for any tool that takes a password.
use crate::inject::Injector;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use vault_contracts::{
    credentials::EntryDetail,
    errors::{AppError, Result},
    filling::AskpassSession,
};
use zeroize::Zeroizing;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const DEFAULT_SECONDS: u64 = 120;
const MAX_SECONDS: u64 = 600;

pub(crate) struct CommandSpec {
    pub command: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub stdin: Option<String>,
    pub timeout_seconds: Option<u64>,
}

fn invalid(field: &str, reason: &str) -> AppError {
    AppError::invalid().with(field, reason)
}

type Output = (Zeroizing<Vec<u8>>, bool);

/// Reads a pipe to its end, keeping at most the output limit, so the program
/// never blocks on a full pipe.
fn collect(mut pipe: impl Read + Send + 'static) -> Receiver<Output> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Zeroizing::new(Vec::new());
        let mut truncated = false;
        let mut buffer = Zeroizing::new([0u8; 8192]);
        loop {
            match pipe.read(&mut buffer[..]) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let room = OUTPUT_LIMIT.saturating_sub(kept.len());
                    if read > room {
                        truncated = true;
                    }
                    kept.extend_from_slice(&buffer[..read.min(room)]);
                }
            }
        }
        let _ = sender.send((kept, truncated));
    });
    receiver
}

/// A program it started may keep the pipes open after it exits; do not wait
/// for those longer than a moment.
fn gather(receiver: &Receiver<Output>) -> Output {
    receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default()
}

/// `askpass`, when present, answers git/ssh/sudo password prompts of this
/// program and its children; variables the caller set itself win.
pub(crate) fn execute(
    mut injector: Injector<'_>,
    spec: CommandSpec,
    askpass: Option<Box<dyn AskpassSession>>,
) -> Result<Value> {
    let Some((program, arguments)) = spec.command.split_first() else {
        return Err(invalid(
            "command",
            "请给出要运行的程序，例如 [\"python\", \"script.py\"]。",
        ));
    };
    if program.trim().is_empty() || spec.command.len() > 256 {
        return Err(invalid("command", "程序名不能为空，参数最多 256 个。"));
    }
    if let Some(cwd) = &spec.cwd
        && (!cwd.is_absolute() || !cwd.is_dir())
    {
        return Err(invalid("cwd", "工作目录必须是本机已存在的绝对路径。"));
    }
    let seconds = spec
        .timeout_seconds
        .unwrap_or(DEFAULT_SECONDS)
        .clamp(1, MAX_SECONDS);
    // Every secret of the group is masked in the output, injected or not.
    injector.prime();

    let mut command = Command::new(injector.render(program)?.as_str());
    for argument in arguments {
        command.arg(injector.render(argument)?.as_str());
    }
    for (name, value) in &spec.env {
        if name.is_empty() || name.contains(['=', '\0']) {
            return Err(invalid("env", "环境变量名不能为空，也不能含有 = 。"));
        }
        command.env(name, injector.render(value)?.as_str());
    }
    if let Some(session) = &askpass {
        for (name, value) in session.env() {
            if !spec.env.contains_key(&name) {
                command.env(name, value);
            }
        }
    }
    let input = match &spec.stdin {
        Some(text) => Some(injector.render(text)?),
        None => None,
    };
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window pops up for a program started by the background service.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let started = Instant::now();
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::new(
                "COMMAND_NOT_FOUND",
                "找不到要运行的程序。请写完整路径，或确认它在 PATH 里（.cmd/.ps1 脚本请用 cmd /c 或 pwsh -File 启动）。",
            )
        } else {
            AppError::new("COMMAND_FAILED_TO_START", "程序没能启动。")
        }
    })?;
    // Drop the rendered command so the plaintext lives only in the child.
    drop(command);
    if let Some(session) = &askpass {
        session.bind(child.id());
    }
    let writer = match (input, child.stdin.take()) {
        (Some(text), Some(mut pipe)) => Some(thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        })),
        _ => None,
    };
    let stdout = collect(child.stdout.take().expect("piped stdout"));
    let stderr = collect(child.stderr.take().expect("piped stderr"));

    let deadline = Duration::from_secs(seconds);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    // The writer finishes when the program reads its input or exits.
    drop(writer);
    let (out, out_cut) = gather(&stdout);
    let (err, err_cut) = gather(&stderr);
    let answered = askpass.as_ref().map(|session| session.answered());
    // Closing the channel stops any helper the program left behind.
    drop(askpass);
    let mut result = json!({
        "status": "command_finished",
        "exitCode": status.and_then(|status| status.code()),
        "timedOut": timed_out,
        "elapsedMs": started.elapsed().as_millis() as u64,
        "stdout": injector.mask_output(&out, out_cut),
        "stderr": injector.mask_output(&err, err_cut),
        "truncated": out_cut || err_cut,
    });
    if let Some(answered) = answered {
        result["askpassAnswered"] = json!(answered);
    }
    Ok(result)
}

/// Builds the injector for a command write: `{{credential}}` stands for `field`.
pub(crate) fn injector<'a>(
    entry: &'a EntryDetail,
    group_id: Option<&'a str>,
    credential: Option<Zeroizing<String>>,
    secret: bool,
) -> Injector<'a> {
    let injector = Injector::new(entry, group_id);
    match credential {
        Some(value) => injector.with_credential(value, secret),
        None => injector,
    }
}
