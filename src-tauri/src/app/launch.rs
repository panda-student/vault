//! Shell launch options. Parsing never starts services or opens storage.
use std::{ffi::OsString, path::PathBuf};
pub(super) struct LaunchOptions {
    pub is_mcp: bool,
    pub backend_mode: bool,
    pub directory: Option<PathBuf>,
    pub report: Option<PathBuf>,
    pub background: bool,
    pub fixture_directory: Option<PathBuf>,
    pub update_health: Option<(PathBuf, String)>,
}
pub(super) fn parse(values: Vec<OsString>) -> Result<LaunchOptions, Box<dyn std::error::Error>> {
    let mut is_mcp = false;
    let mut backend_mode = false;
    let mut directory = None;
    let mut report = None;
    let mut background = false;
    let mut fixture_directory = None;
    let mut update_health = None;
    let mut args = values.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--update-health") => {
                let path = PathBuf::from(args.next().ok_or("Missing health path")?);
                let token = args
                    .next()
                    .ok_or("Missing health token")?
                    .into_string()
                    .map_err(|_| "Invalid health token")?;
                update_health = Some((path, token));
            }
            Some("--mcp") => is_mcp = true,
            Some("--backend") => {
                is_mcp = true;
                backend_mode = true;
            }
            Some("--data-dir") => {
                directory = Some(PathBuf::from(
                    args.next().ok_or("--data-dir requires a path")?,
                ))
            }
            Some("--smoke-test") => {
                report = Some(PathBuf::from(
                    args.next().ok_or("--smoke-test requires a path")?,
                ))
            }
            Some("--background") => background = true,
            Some("--fill-fixture") => {
                fixture_directory = Some(PathBuf::from(
                    args.next().ok_or("--fill-fixture requires a directory")?,
                ))
            }
            _ => return Err("Unknown argument".into()),
        }
    }
    Ok(LaunchOptions {
        is_mcp,
        backend_mode,
        directory,
        report,
        background,
        fixture_directory,
        update_health,
    })
}
