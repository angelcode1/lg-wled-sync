//! Anonymous LS2 private-bus registration probe. No capture, media, or
//! borrowed LG service identity. This is not a test of capture.client rights.
use anyhow::{anyhow, bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::CStr;
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

/// Opens and immediately closes an anonymous LS2 client, with no calls to
/// media, capture, permission management, or registered LG service names.
pub fn run(acknowledge: bool) -> Result<()> {
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
        println!("LS2_ANONYMOUS_ATTEMPT executable={executable} private_bus=true capture=false");
        // Null service name is the ordinary anonymous-client registration.
        let registered = register(ptr::null(), &mut handle, &mut error);
        if !registered {
            let detail = error_details(&error);
            free(&mut error);
            bail!("LS2_ANONYMOUS_REJECTED {detail}");
        }
        free(&mut error);
        if handle.is_null() {
            bail!("LS2_ANONYMOUS_INVALID_HANDLE after reported registration");
        }
        let mut cleanup_error: LunaError = std::mem::zeroed();
        if !init(&mut cleanup_error) {
            bail!("LS2_ANONYMOUS_CLEANUP_ERROR failed to initialize error context");
        }
        let ok = unregister(handle, &mut cleanup_error);
        if !ok {
            let detail = error_details(&cleanup_error);
            free(&mut cleanup_error);
            return Err(anyhow!("LS2_ANONYMOUS_UNREGISTER_FAILED {detail}"));
        }
        free(&mut cleanup_error);
        println!("LS2_ANONYMOUS_OK registered=true unregistered=true capture=false");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_explicit_acknowledgement_before_loading_luna() {
        let message = run(false).unwrap_err().to_string();
        assert!(message.contains("--acknowledge-luna-registration"));
    }
}
