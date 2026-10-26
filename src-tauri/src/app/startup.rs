use super::AppState;
use super::host;
use std::{path::PathBuf, sync::atomic::AtomicBool};
use vault_backend::{adapters::mcp::bridge as mcp_bridge, bootstrap};
fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let bootstrap_args: Vec<_> = std::env::args_os().skip(1).collect();
    if bootstrap_args
        .first()
        .is_some_and(|arg| arg == "--installer-replace")
    {
        if bootstrap_args.len() != 3 {
            return Err("Invalid installer arguments".into());
        }
        return bootstrap::replace_package(
            &PathBuf::from(&bootstrap_args[1]),
            &PathBuf::from(&bootstrap_args[2]),
        )
        .map_err(Into::into);
    }
    if bootstrap_args
        .first()
        .is_some_and(|arg| arg == "--apply-update")
    {
        if bootstrap_args.len() != 2 {
            return Err("Invalid updater arguments".into());
        }
        return bootstrap::apply_update(&PathBuf::from(&bootstrap_args[1])).map_err(Into::into);
    }
    let super::launch::LaunchOptions {
        is_mcp,
        backend_mode,
        directory,
        report,
        background,
        fixture_directory,
        update_health,
    } = super::launch::parse(bootstrap_args)?;
    if let Some(path) = fixture_directory {
        if is_mcp || report.is_some() || directory.is_some() || background {
            return Err("Fixture mode cannot be combined with other modes".into());
        }
        return crate::testing::input_fixture::run(path);
    }
    if report.is_some() && directory.is_none() {
        return Err("--smoke-test requires an explicit --data-dir".into());
    }
    let executable = std::env::current_exe()?;
    if let Some((path, token)) = &update_health {
        let stage = path.parent().ok_or("Invalid health path")?;
        if is_mcp
            || !path.is_absolute()
            || path.file_name().and_then(|n| n.to_str()) != Some("healthy")
            || stage.parent().and_then(|p| std::fs::canonicalize(p).ok())
                != executable
                    .parent()
                    .and_then(|p| std::fs::canonicalize(p).ok())
            || !stage
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".vault-update-"))
            || uuid::Uuid::parse_str(token).is_err()
        {
            return Err("Invalid update health arguments".into());
        }
    }
    let directory = directory.unwrap_or(
        executable
            .parent()
            .ok_or("No executable directory")?
            .join("data"),
    );
    if !directory.is_absolute() {
        return Err("--data-dir must be absolute".into());
    }
    if report.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err("--smoke-test requires an absolute report path".into());
    }
    if is_mcp && (report.is_some() || background) {
        return Err("MCP cannot be combined with desktop arguments".into());
    }
    if is_mcp && !backend_mode {
        return mcp_bridge::run(&executable, &directory);
    }
    // MCP bridges start the same desktop-capable backend without a WebView.
    // Identity remains bound by the host, never by a protocol field.
    let Some(backend) = host::Host::acquire(&executable, &directory)? else {
        if report.is_some() || update_health.is_some() {
            return Err("An isolated test or update launch found an existing backend".into());
        }
        return host::activate(&executable, &directory, background || backend_mode);
    };
    let runtime = bootstrap::UpdateRuntime::acquire(&executable)?;
    let core = bootstrap::open(&directory, false)?;
    core.bind_update_runtime(runtime)?;
    super::runtime::desktop(
        AppState {
            core,
            smoke_report: report,
            smoke_finished: AtomicBool::new(false),
            update_health,
            backend,
        },
        background,
        backend_mode,
    )
}
pub(super) fn main_entry() {
    let installer = std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--installer-replace");
    let health_probe = std::env::args_os().any(|arg| arg == "--update-health");
    let is_mcp = std::env::args_os().any(|arg| arg == "--mcp" || arg == "--backend");
    if is_mcp {
        std::panic::set_hook(Box::new(|_| eprintln!("Local credential service failed")));
    }
    if let Err(error) = run() {
        if is_mcp {
            eprintln!("Local credential service could not complete startup");
        } else {
            eprintln!("vault: {error}");
        }
        if !is_mcp && !installer && !health_probe {
            super::errors::show_error(&error.to_string());
        }
        std::process::exit(1);
    }
}
