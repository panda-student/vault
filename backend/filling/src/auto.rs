//! `target.type = "auto"`: vault picks how to put the value in and falls back
//! on its own. Order: a web page through the browser's debugging port, a
//! console program waiting for a password, a named desktop field through UI
//! Automation, and finally keystrokes into a captured field.
//!
//! A step may hand over to the next one only while the value has not been
//! typed anywhere yet; UI Automation replaces the whole field, so it is the one
//! step that may fail after reading the value and still fall through.
use serde_json::{Value, json};
use std::cell::Cell;
use vault_contracts::{
    errors::{AppError, Result},
    filling::{CaptureRequest, ConsoleRequest, FillStatus},
    ports::system::SystemPlatform,
    write::AutoTarget,
};

struct Attempts(Vec<Value>);
impl Attempts {
    fn push(&mut self, method: &str, reason: impl Into<String>) {
        self.0
            .push(json!({"method": method, "reason": reason.into()}));
    }
}

fn reason_of(status: &FillStatus) -> String {
    status.reason.unwrap_or(status.status).to_string()
}

fn finished(mut result: Value, method: &str, attempts: Attempts) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("method".into(), json!(method));
        object.insert("attempts".into(), Value::Array(attempts.0));
    }
    result
}

pub(crate) fn fill(
    windows: &dyn SystemPlatform,
    target: AutoTarget,
    hosts: &[String],
    resolve: impl Fn() -> Result<String>,
) -> Result<Value> {
    let read = Cell::new(false);
    let value = || {
        read.set(true);
        resolve()
    };
    let named_field = target.field_name.is_some();
    let window = target.window_title.is_some() || target.process_id.is_some();
    if target.port.is_none() && target.target_id.is_none() && !named_field && !window {
        return Err(AppError::invalid().with(
            "target",
            "auto 目标至少要给出一种定位：网页（port+selector）、控制台窗口（processId 或 windowTitle）、桌面输入框（windowTitle/processId+fieldName）或已捕获的 targetId。",
        ));
    }
    if target.port.is_some() && target.selector.is_none() {
        return Err(AppError::invalid().with("selector", "网页填写要给出输入框的 CSS 选择器。"));
    }
    let mut attempts = Attempts(Vec::new());

    // 1. A page in a browser started with a debugging port.
    if let (Some(port), Some(selector)) = (target.port, target.selector.as_deref()) {
        match crate::browser::fill(port, selector, target.target_id.as_deref(), value) {
            Ok(result) => return Ok(finished(result, "browser", attempts)),
            // Part of the value may already be typed into the page.
            Err(error) if read.get() => return Err(error),
            Err(error) => attempts.push("browser", error.code),
        }
    }

    // 2. A console program asking for a password.
    if target.port.is_none() && !named_field && target.target_id.is_none() && window {
        let status = windows.fill_console(
            ConsoleRequest {
                process_id: target.process_id,
                window_title: target.window_title.clone(),
                submit: target.submit,
                hosts: hosts.to_vec(),
            },
            Box::new(&value),
        )?;
        if status.status == "console_typed" {
            return Ok(finished(json!(status), "console", attempts));
        }
        if status.reason != Some("not_console") || read.get() {
            // The console is there but not waiting for this password: refuse
            // rather than type somewhere else.
            return Ok(finished(json!(status), "console", attempts));
        }
        attempts.push("console", reason_of(&status));
    }

    let request = || CaptureRequest {
        window_title: target.window_title.clone(),
        process_id: target.process_id,
        field_name: target.field_name.clone(),
        expected_password: target.expected_password,
    };

    // 3. A named desktop field set through UI Automation, without keystrokes.
    if named_field {
        match windows.set_value(request(), Box::new(&value)) {
            Ok(status) if status.status == "value_set" => {
                return Ok(finished(json!(status), "set_value", attempts));
            }
            Ok(status) => attempts.push("set_value", reason_of(&status)),
            Err(error) => attempts.push("set_value", error.code),
        }
    }

    // 4. Keystrokes into a captured field.
    let target_id = match (&target.target_id, target.port) {
        (Some(id), None) => Some(id.clone()),
        _ if named_field || window => match windows.capture_target(request()) {
            Ok(status) if status.status == "target_captured" => status.target_id,
            Ok(status) => {
                attempts.push("keyboard", reason_of(&status));
                None
            }
            Err(error) => {
                attempts.push("keyboard", error.code);
                None
            }
        },
        _ => None,
    };
    if let Some(id) = target_id {
        let status = windows.fill_target(&id, Box::new(&value))?;
        return Ok(finished(json!(status), "keyboard", attempts));
    }
    Ok(json!({"status": "not_filled", "attempts": attempts.0}))
}
