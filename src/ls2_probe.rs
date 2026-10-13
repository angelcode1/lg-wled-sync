//! Registration-only C5 LS2 diagnostic. No VT driver, developer marker or media calls.
//! This exercises the same executable identity used by the native histogram test.
use anyhow::{anyhow, bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::ptr;

/// From webOS luna-service2 public C API's LSError layout (32-bit ABI).
/// This must be checked against the installed TV's C header before runtime use.
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
type RegisterPubPriv =
    unsafe extern "C" fn(*const libc::c_char, *mut *mut libc::c_void, bool, *mut LunaError) -> bool;
type Unregister = unsafe extern "C" fn(*mut libc::c_void, *mut LunaError) -> bool;

const RM_NAME_PREFIX: &str = "com.webos.rm.client._lghuesyncprobe";

fn registration_name(pid: u32) -> CString {
    CString::new(format!("{RM_NAME_PREFIX}{pid}")).expect("static LS2 prefix and decimal PID")
}

fn error_details(error: &LunaError) -> String {
    let message = if error.message.is_null() {
        "(no LSError message)".to_owned()
    } else {
        // SAFETY: The LS2 C library owns the error message until LSErrorFree.
        unsafe { CStr::from_ptr(error.message) }
            .to_string_lossy()
            .into_owned()
    };
    format!("code={} message={message}", error.error_code)
}

/// Registers on the *private* bus with a generated Resource Manager name,
/// and unregisters immediately. No subscriptions, media or capture calls.
///
/// This function deliberately does not load libvtcapture, libums_connector,
/// or libresource_mgr_client; its outcome isolates LS2 registration from the
/// VT native client construction. The caller must explicitly acknowledge.
pub fn run(acknowledge: bool) -> Result<()> {
    if !acknowledge {
        bail!("LS2 registration is an active bus operation; pass --acknowledge-luna-registration");
    }

    let name = registration_name(std::process::id());
    let name_text = name.to_str().context("invalid LS2 registration name")?;
    let executable = std::fs::read_link("/proc/self/exe")
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unavailable)".into());

    // SAFETY: Imported symbols use the Luna C API's public signatures.
    // No calls are made to the native capture or Resource Manager libraries.
    unsafe {
        let library = Library::new("libluna-service2.so.3")
            .context("unable to load the TV Luna client library")?;
        let init: Symbol<'_, ErrorInit> = library.get(b"LSErrorInit\0")?;
        let free: Symbol<'_, ErrorFree> = library.get(b"LSErrorFree\0")?;
        let register: Symbol<'_, RegisterPubPriv> = library.get(b"LSRegisterPubPriv\0")?;
        let unregister: Symbol<'_, Unregister> = library.get(b"LSUnregister\0")?;

        // LSError contains only integers and nullable pointers; zero is its
        // pre-initialization state. LSErrorInit sets the library's magic cookie.
        let mut error: LunaError = std::mem::zeroed();
        if !init(&mut error) {
            bail!("LSErrorInit refused initialization");
        }

        let mut handle: *mut libc::c_void = ptr::null_mut();
        println!(
            "LS2_PROBE_ATTEMPT executable={executable} name={name_text} private_bus=true capture=false"
        );
        let registered = register(name.as_ptr(), &mut handle, false, &mut error);
        if !registered {
            let detail = error_details(&error);
            free(&mut error);
            bail!("LS2_PROBE_REJECTED name={name_text} {detail}");
        }
        free(&mut error);

        if handle.is_null() {
            bail!("LS2_PROBE_INVALID_HANDLE name={name_text} (registration reported success)");
        }

        let mut cleanup_error: LunaError = std::mem::zeroed();
        if !init(&mut cleanup_error) {
            bail!("LS2_PROBE_CLEANUP_ERROR LSErrorInit failed; handle still registered");
        }
        let unregistered = unregister(handle, &mut cleanup_error);
        if !unregistered {
            let detail = error_details(&cleanup_error);
            free(&mut cleanup_error);
            return Err(anyhow!(
                "LS2_PROBE_UNREGISTER_FAILED name={name_text} {detail}"
            ));
        }
        free(&mut cleanup_error);
        println!("LS2_PROBE_OK name={name_text} registered=true unregistered=true capture=false");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_uses_only_the_verified_resource_manager_family() {
        let name = registration_name(12345);
        assert_eq!(
            name.to_str().unwrap(),
            "com.webos.rm.client._lghuesyncprobe12345"
        );
        assert!(!name.to_bytes().contains(&0));
    }

    #[test]
    fn requires_explicit_acknowledgement_before_loading_luna() {
        let message = run(false).unwrap_err().to_string();
        assert!(message.contains("--acknowledge-luna-registration"));
    }
}
