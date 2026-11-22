//! Resolve explicitly named controls locally. Never query control values.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::Variant::VARIANT,
        UI::{
            Accessibility::{
                IUIAutomationElement, TreeScope_Descendants, UIA_ControlTypePropertyId,
                UIA_EditControlTypeId,
            },
            WindowsAndMessaging::{
                EnumWindows, GetWindowTextW, IsIconic, IsWindow, IsWindowVisible,
                SetForegroundWindow,
            },
        },
    },
    core::{BOOL, BSTR},
};

/// Capture and set-value share one accessibility worker at a time.
static BUSY: AtomicBool = AtomicBool::new(false);
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}
fn claim() -> std::result::Result<(), &'static str> {
    BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| ())
        .map_err(|_| "accessibility_busy")
}

struct Search {
    request: CaptureRequest,
    windows: Vec<isize>,
}
unsafe extern "system" fn enumerate(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let search = unsafe { &mut *(parameter.0 as *mut Search) };
    if matches_window(hwnd, &search.request) {
        search.windows.push(hwnd.0 as isize);
    }
    true.into()
}
fn matches_window(hwnd: HWND, request: &CaptureRequest) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if request.process_id.is_some_and(|id| id != pid) {
            return false;
        }
        if let Some(title) = &request.window_title {
            let mut buffer = [0u16; 2048];
            let count = GetWindowTextW(hwnd, &mut buffer);
            if count <= 0
                || !String::from_utf16_lossy(&buffer[..count as usize])
                    .to_lowercase()
                    .contains(&title.to_lowercase())
            {
                return false;
            }
        }
        true
    }
}
/// The one window the selectors name, or the foreground window without selectors.
fn target_window(request: &CaptureRequest) -> std::result::Result<HWND, &'static str> {
    let target = if request.window_title.is_some() || request.process_id.is_some() {
        let mut search = Search {
            request: request.clone(),
            windows: vec![],
        };
        unsafe { EnumWindows(Some(enumerate), LPARAM(&mut search as *mut Search as isize)) }
            .map_err(|_| "window_unavailable")?;
        match search.windows.as_slice() {
            [one] => HWND(*one as *mut _),
            [] => return Err("window_not_found"),
            _ => return Err("ambiguous_window"),
        }
    } else {
        unsafe { GetForegroundWindow() }
    };
    if target.0.is_null() {
        return Err("window_not_found");
    }
    Ok(target)
}
fn automation() -> std::result::Result<IUIAutomation2, &'static str> {
    unsafe {
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .map_err(|_| "accessibility_unavailable")?;
        automation
            .SetConnectionTimeout(500)
            .map_err(|_| "accessibility_unavailable")?;
        automation
            .SetTransactionTimeout(500)
            .map_err(|_| "accessibility_unavailable")?;
        Ok(automation)
    }
}
/// The single visible, enabled, writable edit control with exactly this name.
fn find_field(
    automation: &IUIAutomation2,
    target: HWND,
    request: &CaptureRequest,
    cancelled: &AtomicBool,
) -> std::result::Result<IUIAutomationElement, &'static str> {
    unsafe {
        let condition = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
            .map_err(|_| "accessibility_unavailable")?;
        let deadline = Instant::now() + Duration::from_millis(3000);
        loop {
            if cancelled.load(Ordering::SeqCst) {
                return Err("accessibility_timeout");
            }
            // Chromium can invalidate the first UIA tree while enabling accessibility.
            // Retry the entire read-only query, never a partial candidate set.
            let queried = (|| -> std::result::Result<_, &'static str> {
                let root = automation
                    .ElementFromHandle(target)
                    .map_err(|_| "window_unavailable")?;
                let elements = root
                    .FindAll(TreeScope_Descendants, &condition)
                    .map_err(|_| "accessibility_unavailable")?;
                let count = elements.Length().map_err(|_| "accessibility_unavailable")?;
                if count > 4096 {
                    return Err("too_many_controls");
                }
                let mut found = None;
                for index in 0..count {
                    if cancelled.load(Ordering::SeqCst) {
                        return Err("accessibility_timeout");
                    }
                    let element = elements
                        .GetElement(index)
                        .map_err(|_| "accessibility_unavailable")?;
                    let name = element
                        .CurrentName()
                        .map_err(|_| "accessibility_unavailable")?;
                    if name.to_string().trim() != request.field_name.as_deref().unwrap_or("").trim()
                        || element
                            .CurrentIsOffscreen()
                            .map_err(|_| "accessibility_unavailable")?
                            .as_bool()
                    {
                        continue;
                    }
                    if found.is_some() {
                        return Err("ambiguous_field");
                    }
                    found = Some(element);
                }
                Ok(found)
            })();
            let found = match queried {
                Ok(found) => found,
                Err("accessibility_unavailable" | "window_unavailable")
                    if Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(150));
                    continue;
                }
                Err(reason) => return Err(reason),
            };
            if let Some(element) = found {
                if !element
                    .CurrentIsEnabled()
                    .map_err(|_| "accessibility_unavailable")?
                    .as_bool()
                {
                    return Err("read_only_or_non_input");
                }
                if let Ok(pattern) =
                    element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    && pattern
                        .CurrentIsReadOnly()
                        .map_err(|_| "accessibility_unavailable")?
                        .as_bool()
                {
                    return Err("read_only_or_non_input");
                }
                let password = element
                    .CurrentIsPassword()
                    .map_err(|_| "accessibility_unavailable")?
                    .as_bool();
                if request
                    .expected_password
                    .is_some_and(|expected| expected != password)
                {
                    return Err("field_type_mismatch");
                }
                return Ok(element);
            }
            if Instant::now() >= deadline {
                return Err("field_not_found");
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}
pub(crate) fn capture(request: CaptureRequest) -> std::result::Result<Identity, &'static str> {
    claim()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("vault-input-locator".into())
        .spawn(move || {
            let _reset = Reset;
            let result = locate(request, &worker_cancelled);
            let _ = sender.send(result);
        })
        .is_err()
    {
        BUSY.store(false, Ordering::SeqCst);
        return Err("accessibility_unavailable");
    }
    let result = receiver.recv_timeout(Duration::from_secs(5));
    cancelled.store(true, Ordering::SeqCst);
    result.unwrap_or(Err("accessibility_timeout"))
}
fn locate(
    request: CaptureRequest,
    cancelled: &AtomicBool,
) -> std::result::Result<Identity, &'static str> {
    let _apartment = Apartment::new().map_err(|_| "accessibility_unavailable")?;
    unsafe {
        let original = GetForegroundWindow();
        let target = target_window(&request)?;
        let automation = automation()?;
        let element = find_field(&automation, target, &request, cancelled)?;
        if cancelled.load(Ordering::SeqCst) {
            return Err("accessibility_timeout");
        }
        let foreground = GetForegroundWindow();
        if foreground != original && foreground != target {
            return Err("focus_changed_during_capture");
        }
        if !matches_window(target, &request) {
            return Err("window_unavailable");
        }
        if [0x10, 0x11, 0x12, 0x5b, 0x5c]
            .iter()
            .any(|key| GetAsyncKeyState(*key) < 0)
        {
            return Err("modifier_held");
        }
        // Only focus in this worker; it never resolves or types a credential.
        let _ = SetForegroundWindow(target);
        if cancelled.load(Ordering::SeqCst) {
            return Err("accessibility_timeout");
        }
        element.SetFocus().map_err(|_| "focus_unavailable")?;
        // SetFocus may return before Chromium publishes its new focus state.
        let focus_deadline = Instant::now() + Duration::from_millis(600);
        loop {
            if cancelled.load(Ordering::SeqCst) {
                return Err("accessibility_timeout");
            }
            let foreground = GetForegroundWindow();
            if foreground != original && foreground != target {
                return Err("focus_changed_during_capture");
            }
            let focused = automation.GetFocusedElement();
            let ready = focused.ok().is_some_and(|focused| {
                automation
                    .CompareElements(&element, &focused)
                    .is_ok_and(|same| same.as_bool())
                    && focused
                        .CurrentHasKeyboardFocus()
                        .is_ok_and(|has| has.as_bool())
            });
            if foreground == target && ready {
                break;
            }
            if Instant::now() >= focus_deadline {
                return Err("focus_unavailable");
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        let (process, runtime, is_password) =
            accessibility_identity().ok_or("focus_unavailable")?;
        if process == -1 || runtime.is_empty() {
            return Err("read_only_or_non_input");
        }
        let mut captured = identity(false)?;
        if captured.window != target.0 as isize || cancelled.load(Ordering::SeqCst) {
            return Err("focus_changed_during_capture");
        }
        captured.process = process;
        captured.runtime = runtime;
        captured.is_password = is_password;
        Ok(captured)
    }
}

/// Browser page fields ignore a UIA value change (scripts never see it), so
/// they go through the page itself or the keyboard instead.
const BROWSER_FRAMEWORKS: [&str; 2] = ["chrome", "gecko"];

/// Sets the named field through its UIA value pattern: no focus change, no
/// keyboard, nothing read back. The worker finds and checks the field first;
/// the credential is resolved only after that, then handed to the worker.
pub(crate) fn set_value(
    request: CaptureRequest,
    resolve: impl FnOnce() -> vault_contracts::errors::Result<String>,
) -> vault_contracts::errors::Result<std::result::Result<(), &'static str>> {
    if request.field_name.is_none() {
        return Ok(Err("field_name_required"));
    }
    if let Err(reason) = claim() {
        return Ok(Err(reason));
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (value_tx, value_rx) = std::sync::mpsc::sync_channel::<Option<Zeroizing<String>>>(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("vault-input-value".into())
        .spawn(move || {
            let _reset = Reset;
            let Ok(_apartment) = Apartment::new() else {
                let _ = ready_tx.send(Err("accessibility_unavailable"));
                return;
            };
            let found = (|| {
                let target = target_window(&request)?;
                let automation = automation()?;
                let element = find_field(&automation, target, &request, &worker_cancelled)?;
                let framework = unsafe { element.CurrentFrameworkId() }
                    .map_err(|_| "accessibility_unavailable")?
                    .to_string()
                    .to_lowercase();
                if BROWSER_FRAMEWORKS.contains(&framework.as_str()) {
                    return Err("browser_document");
                }
                let pattern = unsafe {
                    element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                }
                .map_err(|_| "no_value_pattern")?;
                Ok((target, pattern))
            })();
            let (target, pattern) = match found {
                Ok(found) => {
                    let _ = ready_tx.send(Ok(()));
                    found
                }
                Err(reason) => {
                    let _ = ready_tx.send(Err(reason));
                    return;
                }
            };
            let Ok(Some(value)) = value_rx.recv_timeout(Duration::from_secs(10)) else {
                return;
            };
            let result = (|| {
                if worker_cancelled.load(Ordering::SeqCst) {
                    return Err("accessibility_timeout");
                }
                if !unsafe { IsWindow(Some(target)) }.as_bool() || !matches_window(target, &request)
                {
                    return Err("target_changed");
                }
                if unsafe { pattern.CurrentIsReadOnly() }
                    .map_err(|_| "target_changed")?
                    .as_bool()
                {
                    return Err("read_only_or_non_input");
                }
                let wide: Zeroizing<Vec<u16>> = Zeroizing::new(value.encode_utf16().collect());
                let bstr = BSTR::from_wide(&wide);
                let set = unsafe { pattern.SetValue(&bstr) };
                wipe(&bstr);
                set.map_err(|_| "set_value_failed")
            })();
            let _ = done_tx.send(result);
        })
        .is_err()
    {
        BUSY.store(false, Ordering::SeqCst);
        return Ok(Err("accessibility_unavailable"));
    }
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or(Err("accessibility_timeout"));
    if let Err(reason) = ready {
        cancelled.store(true, Ordering::SeqCst);
        return Ok(Err(reason));
    }
    let value = match resolve() {
        Ok(value) => Zeroizing::new(value),
        Err(error) => {
            cancelled.store(true, Ordering::SeqCst);
            let _ = value_tx.send(None);
            return Err(error);
        }
    };
    if value.is_empty() || value.chars().any(char::is_control) || value.len() > 16384 {
        let _ = value_tx.send(None);
        return Ok(Err("invalid_text"));
    }
    if value_tx.send(Some(value)).is_err() {
        return Ok(Err("accessibility_unavailable"));
    }
    let done = done_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or(Err("accessibility_timeout"));
    cancelled.store(true, Ordering::SeqCst);
    Ok(done)
}
/// Overwrite the BSTR's characters before it is freed.
fn wipe(bstr: &BSTR) {
    let len = bstr.len();
    if len > 0 {
        let pointer = bstr.as_ptr() as *mut u16;
        for index in 0..len {
            unsafe { std::ptr::write_volatile(pointer.add(index), 0) };
        }
    }
}
