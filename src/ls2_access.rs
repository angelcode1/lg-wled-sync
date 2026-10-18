//! Bounded, no-pixel C5 LS2 capture-method authorization test.
//! Calls ONLY com.webos.service.capture/getCapability with {} using the
//! application's own named LS2 handle, not the privileged luna-send binary.
use anyhow::{anyhow, bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

const NAME: &str = "org.webosbrew.lg-hue-sync.c5-probe";
const URI: &str = "luna://com.webos.service.capture/getCapability";
const MAX_RESPONSE: usize = 1024;

#[repr(C)]
struct LunaError {
    error_code: libc::c_int,
    message: *mut libc::c_char,
    file: *const libc::c_char,
    line: libc::c_int,
    func: *const libc::c_char,
    padding: *mut libc::c_void,
    magic: libc::c_ulong,
}

type Init = unsafe extern "C" fn(*mut LunaError) -> bool;
type Free = unsafe extern "C" fn(*mut LunaError);
type Register = unsafe extern "C" fn(
    *const libc::c_char,
    *mut *mut libc::c_void,
    *mut LunaError,
) -> bool;
type Unregister = unsafe extern "C" fn(*mut libc::c_void, *mut LunaError) -> bool;
type Attach = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_void,
    *mut LunaError,
) -> bool;
type Callback =
    unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut libc::c_void) -> bool;
type CallOneReply = unsafe extern "C" fn(
    *mut libc::c_void,
    *const libc::c_char,
    *const libc::c_char,
    Callback,
    *mut libc::c_void,
    *mut u32,
    *mut LunaError,
) -> bool;
type Cancel = unsafe extern "C" fn(*mut libc::c_void, u32, *mut LunaError) -> bool;
type GetPayload = unsafe extern "C" fn(*mut libc::c_void) -> *const libc::c_char;
type NewContext = unsafe extern "C" fn() -> *mut libc::c_void;
type Iteration = unsafe extern "C" fn(*mut libc::c_void, libc::c_int) -> libc::c_int;
type Unref = unsafe extern "C" fn(*mut libc::c_void);

fn error_details(error: &LunaError) -> String {
    let message = if error.message.is_null() {
        "(no message)".to_owned()
    } else {
        // SAFETY: the LS2 library owns this pointer until LSErrorFree.
        unsafe { CStr::from_ptr(error.message) }
            .to_string_lossy()
            .into_owned()
    };
    format!("code={} message={message}", error.error_code)
}

fn checked(
    init: Init,
    free: Free,
    label: &str,
    operation: impl FnOnce(*mut LunaError) -> bool,
) -> Result<()> {
    // SAFETY: the LSError layout matches the documented 32-bit Luna C ABI.
    let mut error: LunaError = unsafe { std::mem::zeroed() };
    if !unsafe { init(&mut error) } {
        bail!("{label}: LSErrorInit failed");
    }
    let ok = operation(&mut error);
    let details = (!ok).then(|| error_details(&error));
    unsafe { free(&mut error) };
    if let Some(details) = details {
        bail!("{label}: {details}");
    }
    Ok(())
}

struct ReplyState {
    payload_fn: GetPayload,
    received: Option<String>,
}

unsafe extern "C" fn on_reply(
    _handle: *mut libc::c_void,
    message: *mut libc::c_void,
    context: *mut libc::c_void,
) -> bool {
    if context.is_null() {
        return true;
    }
    // SAFETY: address points to ReplyState on our stack for the duration of
    // the single outstanding request; callbacks stop before LSUnregister.
    let state = unsafe { &mut *context.cast::<ReplyState>() };
    if message.is_null() {
        state.received = Some("(null LS2 response)".into());
        return true;
    }
    let payload = unsafe { (state.payload_fn)(message) };
    state.received = Some(if payload.is_null() {
        "(null payload)".into()
    } else {
        let bytes = unsafe { CStr::from_ptr(payload) }.to_bytes();
        String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_RESPONSE)]).into_owned()
    });
    true
}

/// One native LS2 method authorization check. No video, handles or output.
/// Must be deliberately invoked only after installing a scoped own-name role.
pub fn run(acknowledge: bool) -> Result<()> {
    if !acknowledge {
        bail!("pass --acknowledge-capture-permission-check before a native LS2 call");
    }
    let name = CString::new(NAME)?;
    let uri = CString::new(URI)?;
    let json = CString::new("{}")?;
    let ls2 = unsafe { Library::new("libluna-service2.so.3") }
        .context("load native LS2 client library")?;
    let glib =
        unsafe { Library::new("libglib-2.0.so.0") }.context("load GLib main context library")?;

    // SAFETY: verified against webOS Luna public C API signatures.
    unsafe {
        let init: Symbol<'_, Init> = ls2.get(b"LSErrorInit\0")?;
        let free: Symbol<'_, Free> = ls2.get(b"LSErrorFree\0")?;
        let register: Symbol<'_, Register> = ls2.get(b"LSRegister\0")?;
        let unregister: Symbol<'_, Unregister> = ls2.get(b"LSUnregister\0")?;
        let attach: Symbol<'_, Attach> = ls2.get(b"LSGmainContextAttach\0")?;
        let call: Symbol<'_, CallOneReply> = ls2.get(b"LSCallOneReply\0")?;
        let cancel: Symbol<'_, Cancel> = ls2.get(b"LSCallCancel\0")?;
        let get_payload: Symbol<'_, GetPayload> = ls2.get(b"LSMessageGetPayload\0")?;
        let context_new: Symbol<'_, NewContext> = glib.get(b"g_main_context_new\0")?;
        let iterate: Symbol<'_, Iteration> = glib.get(b"g_main_context_iteration\0")?;
        let context_unref: Symbol<'_, Unref> = glib.get(b"g_main_context_unref\0")?;
        let mut handle: *mut libc::c_void = ptr::null_mut();

        println!("LS2_CAPTURE_PREFLIGHT name={NAME} method=getCapability no_pixels=true");
        checked(*init, *free, "LS2_REGISTER", |error| {
            register(name.as_ptr(), &mut handle, error)
        })?;
        if handle.is_null() {
            bail!("LS2 registration reported success with null handle");
        }

        let mut main_context: *mut libc::c_void = ptr::null_mut();
        let mut token: u32 = 0;
        let mut completed = false;
        let test_result = (|| -> Result<()> {
            main_context = context_new();
            if main_context.is_null() {
                bail!("GLib main context allocation failed");
            }
            checked(*init, *free, "LS2_ATTACH", |error| {
                attach(handle, main_context, error)
            })?;
            let mut reply = ReplyState {
                payload_fn: *get_payload,
                received: None,
            };
            checked(*init, *free, "LS2_CALL_DENIED_OR_FAILED", |error| {
                call(
                    handle,
                    uri.as_ptr(),
                    json.as_ptr(),
                    on_reply,
                    (&mut reply as *mut ReplyState).cast(),
                    &mut token,
                    error,
                )
            })?;
            let begin = Instant::now();
            while reply.received.is_none() && begin.elapsed() < Duration::from_secs(2) {
                // Never block in GLib: an inaccessible service must not hang this probe.
                iterate(main_context, 0);
                if reply.received.is_none() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if let Some(payload) = reply.received {
                completed = true;
                println!("LS2_GETCAPABILITY_REPLY {payload}");
                Ok(())
            } else {
                bail!("LS2_GETCAPABILITY_TIMEOUT after 2000ms")
            }
        })();

        // With an outstanding token, cancel it before dropping the callback
        // stack state. The LS2 handle unregister also cancels pending calls.
        if !completed && token != 0 {
            let _ = checked(*init, *free, "LS2_CANCEL", |error| {
                cancel(handle, token, error)
            });
        }
        let unregister_result = checked(*init, *free, "LS2_UNREGISTER", |error| {
            unregister(handle, error)
        });
        if !main_context.is_null() {
            context_unref(main_context);
        }
        unregister_result.context("native Luna test must unregister cleanly")?;
        test_result?;
        println!("LS2_GETCAPABILITY_DONE unregistered=true capture=false");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_ack_required_without_loading_any_native_library() {
        assert!(run(false)
            .unwrap_err()
            .to_string()
            .contains("--acknowledge-capture-permission-check"));
    }
}
