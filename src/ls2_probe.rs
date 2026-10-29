//! LS2 private-bus registration probe (anonymous or own name). No capture, media, or
//! borrowed LG service identity. This is not a test of capture.client rights.
use anyhow::{anyhow, bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::ptr;

/// From webOS Luna C API LSError layout (32-bit ABI).
/// Confirmed for this tree's existing registration-only diagnostic.
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
type ErrorInit = unsafe extern "C" fn(*mut LunaError) -> bool;
type ErrorFree = unsafe extern "C" fn(*mut LunaError);
type RegisterPrivate =
    unsafe extern "C" fn(*const libc::c_char, *mut *mut libc::c_void, *mut LunaError) -> bool;
type Unregister = unsafe extern "C" fn(*mut libc::c_void, *mut LunaError) -> bool;

/// Our exact standalone probe identity, never an LG/vendor-owned name.
const OWN_PROBE_NAME: &str = "org.webosbrew.lg-hue-sync.c5-probe";

fn error_details(error: &LunaError) -> String {
    let message = if error.message.is_null() {
        "(no LSError message)".to_owned()
    } else {
        // SAFETY: LS2 owns its error message until LSErrorFree.
        unsafe { CStr::from_ptr(error.message) }
            .to_string_lossy()
            .into_owned()
    };
    format!("code={} message={message}", error.error_code)
}

/// Opens and closes either an anonymous client or our exact independently
/// registered probe name. No capture, media, or permission-management calls.
/// A separate user-reviewed executable role is required for named mode.
pub fn run(acknowledge: bool, named_probe: bool) -> Result<()> {
    if !acknowledge {
        bail!("LS2 registration is active; pass --acknowledge-luna-registration");
    }
    let executable = std::fs::read_link("/proc/self/exe")
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unavailable)".into());
    unsafe {
        let library = Library::new("libluna-service2.so.3")
            .context("unable to load TV Luna client library")?;
        let init: Symbol<'_, ErrorInit> = library.get(b"LSErrorInit\0")?;
        let free: Symbol<'_, ErrorFree> = library.get(b"LSErrorFree\0")?;
        let register: Symbol<'_, RegisterPrivate> = library.get(b"LSRegister\0")?;
        let unregister: Symbol<'_, Unregister> = library.get(b"LSUnregister\0")?;
        let mut error: LunaError = std::mem::zeroed();
        if !init(&mut error) {
            bail!("LSErrorInit refused initialization");
        }
        let mut handle: *mut libc::c_void = ptr::null_mut();
        let name = if named_probe {
            Some(CString::new(OWN_PROBE_NAME).expect("static probe name contains no NUL"))
        } else {
            None
        };
        let label = if named_probe { "NAMED" } else { "ANONYMOUS" };
        println!(
            "LS2_{label}_ATTEMPT executable={executable} name={} private_bus=true capture=false",
            name.as_ref().map_or("(null)", |_| OWN_PROBE_NAME)
        );
        let registered = register(
            name.as_ref().map_or(ptr::null(), |name| name.as_ptr()),
            &mut handle,
            &mut error,
        );
        if !registered {
            let detail = error_details(&error);
            free(&mut error);
            bail!("LS2_{label}_REJECTED {detail}");
        }
        free(&mut error);
        if handle.is_null() {
            bail!("LS2_{label}_INVALID_HANDLE after reported registration");
        }
        let mut cleanup_error: LunaError = std::mem::zeroed();
        if !init(&mut cleanup_error) {
            bail!("LS2_{label}_CLEANUP_ERROR failed to initialize error context");
        }
        let ok = unregister(handle, &mut cleanup_error);
        if !ok {
            let detail = error_details(&cleanup_error);
            free(&mut cleanup_error);
            return Err(anyhow!("LS2_{label}_UNREGISTER_FAILED {detail}"));
        }
        free(&mut cleanup_error);
        println!("LS2_{label}_OK registered=true unregistered=true capture=false");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_probe_uses_one_exact_application_identity() {
        assert_eq!(OWN_PROBE_NAME, "org.webosbrew.lg-hue-sync.c5-probe");
        assert!(!OWN_PROBE_NAME.contains('*'));
        assert!(!OWN_PROBE_NAME.starts_with("com.webos."));
        assert!(CString::new(OWN_PROBE_NAME).is_ok());
    }

    #[test]
    fn requires_explicit_acknowledgement_before_loading_luna() {
        let message = run(false, false).unwrap_err().to_string();
        assert!(run(false, true).is_err());
        assert!(message.contains("--acknowledge-luna-registration"));
    }
}
