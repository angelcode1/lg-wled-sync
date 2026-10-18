//! Bounded, no-pixel C5 LS2 capture-method authorization test.
//! Calls ONLY com.webos.service.capture/getCapability with {} using the
//! application's own named LS2 handle, not the privileged luna-send binary.
use anyhow::{bail, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
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
type Register =
    unsafe extern "C" fn(*const libc::c_char, *mut *mut libc::c_void, *mut LunaError) -> bool;
type Unregister = unsafe extern "C" fn(*mut libc::c_void, *mut LunaError) -> bool;
type Attach = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut LunaError) -> bool;
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
        // Keep the callback state alive until cancellation and LSUnregister.
        let mut reply = ReplyState {
            payload_fn: *get_payload,
            received: None,
        };
        let test_result = (|| -> Result<()> {
            main_context = context_new();
            if main_context.is_null() {
                bail!("GLib main context allocation failed");
            }
            checked(*init, *free, "LS2_ATTACH", |error| {
                attach(handle, main_context, error)
            })?;
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
            if let Some(payload) = reply.received.as_deref() {
                completed = true;
                println!("LS2_GETCAPABILITY_REPLY {payload}");
                let json: serde_json::Value =
                    serde_json::from_str(payload).context("LS2_GETCAPABILITY_INVALID_JSON")?;
                if json.get("returnValue").and_then(serde_json::Value::as_bool) != Some(true) {
                    bail!("LS2_GETCAPABILITY_NOT_AUTHORIZED_OR_REJECTED: {payload}");
                }
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

/// Exactly one createHandle {} followed by destroyHandle {handle}.
/// This mutates an LG capture-service session, even though it does not
/// set an output or execute a screenshot. Never called by the daemon.
pub fn run_handle_lifecycle(acknowledge: bool) -> Result<()> {
    run_handle_session(acknowledge, false, None, 0)
}

/// Stage 4A: configure a private no-pixel output, then destroy the handle.
/// NO execute call. Requires a separately approved temporary capture.client ACG.
pub fn run_handle_configuration(acknowledge: bool) -> Result<()> {
    run_handle_session(acknowledge, true, None, 0)
}

/// Stage 4B is never automatic. One frame is allowed only when an independent
/// foreground-media classifier reports Clear repeatedly and immediately
/// before the native execute call. No protected/unknown fallback.
pub async fn run_one_clear_execute(acknowledge: bool) -> Result<()> {
    if !acknowledge {
        bail!("pass --acknowledge-single-clear-execute for one native LG screenshot");
    }
    for _ in 0..3 {
        if crate::capture::media::query().await != crate::capture::drm_routing::PlaybackClass::Clear
        {
            bail!("C5_EXECUTE_REFUSE_MEDIA_NOT_CLEAR: no capture requested");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    // A blocking LS2 GLib event loop must not occupy a Tokio worker. The
    // runtime handle is used for one last fresh classification after output
    // setup and immediately before the only execute call.
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || run_handle_session(true, true, Some(runtime), 1))
        .await
        .context("C5_EXECUTE_BLOCKING_THREAD_FAILED")?
}

struct ScopedOutputDir {
    path: PathBuf,
}
impl ScopedOutputDir {
    fn new() -> Result<Self> {
        let path = PathBuf::from(format!("/tmp/lg-hue-sync-stage4-{}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .with_context(|| {
                format!(
                    "cannot create fresh private temp directory {}",
                    path.display()
                )
            })?;
        Ok(Self { path })
    }
    fn jpeg_path(&self) -> PathBuf {
        self.path.join("single.jpg")
    }
    fn numbered_jpeg_path(&self, number: u8) -> PathBuf {
        self.path.join(format!("frame-{number:02}.jpg"))
    }
}
impl Drop for ScopedOutputDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Stage 5 uses exactly one native registration and capture handle for five
/// executions. The caller separately performs independent Clear preflight,
/// owns timing and CPU sampling, and restores the Stage 1 role afterward.
pub(crate) fn run_five_native_clear_frames(runtime: tokio::runtime::Handle) -> Result<()> {
    run_handle_session(true, true, Some(runtime), 5)
}

fn run_handle_session(
    acknowledge: bool,
    configure: bool,
    execute_runtime: Option<tokio::runtime::Handle>,
    frame_limit: u8,
) -> Result<()> {
    if frame_limit > 5 || (execute_runtime.is_none() && frame_limit != 0) {
        bail!("C5_FRAME_LIMIT_INVALID");
    }
    if !acknowledge {
        if configure {
            bail!(
                "pass --acknowledge-handle-configuration before configuring an LG capture handle"
            );
        }
        bail!("pass --acknowledge-handle-lifecycle before creating an LG capture handle");
    }
    // Fail before creating any LG handle if the output directory cannot
    // be safely created with O_EXCL-like create_dir semantics (no reuse).
    let scoped_output = if configure {
        Some(ScopedOutputDir::new()?)
    } else {
        None
    };
    let name = CString::new(NAME)?;
    let ls2 = unsafe { Library::new("libluna-service2.so.3") }
        .context("load native LS2 client library")?;
    let glib =
        unsafe { Library::new("libglib-2.0.so.0") }.context("load GLib main context library")?;

    // SAFETY: types follow the public webOS LS2 and GLib ABI.
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

        let mut client: *mut libc::c_void = ptr::null_mut();
        println!(
            "C5_HANDLE_TEST_BEGIN client={NAME} max_handles=1 config={configure} execute={} max_frames={frame_limit}",
            execute_runtime.is_some()
        );
        checked(*init, *free, "LS2_REGISTER", |err| {
            register(name.as_ptr(), &mut client, err)
        })?;
        if client.is_null() {
            bail!("LS2_REGISTER_NULL_HANDLE");
        }

        let mut context: *mut libc::c_void = ptr::null_mut();
        // Keep the one callback state alive through unregister, including on
        // error and cancelled calls.
        let mut reply = ReplyState {
            payload_fn: *get_payload,
            received: None,
        };

        let test_result = (|| -> Result<()> {
            context = context_new();
            if context.is_null() {
                bail!("GLIB_CONTEXT_ALLOC_FAILED");
            }
            checked(*init, *free, "LS2_ATTACH", |err| {
                attach(client, context, err)
            })?;

            // Bounded requests, no retries, no parallel capture handles, no
            // file output. The same LS2 connection serves both calls.
            let mut request =
                |method: &str, request_body: &serde_json::Value| -> Result<serde_json::Value> {
                    reply.received = None;
                    let uri = CString::new(format!("luna://com.webos.service.capture/{method}"))?;
                    let body = CString::new(request_body.to_string())?;
                    let mut token: u32 = 0;
                    checked(*init, *free, "LS2_CALL_FAILED", |err| {
                        call(
                            client,
                            uri.as_ptr(),
                            body.as_ptr(),
                            on_reply,
                            (&mut reply as *mut ReplyState).cast(),
                            &mut token,
                            err,
                        )
                    })?;
                    let start = Instant::now();
                    while reply.received.is_none() && start.elapsed() < Duration::from_secs(2) {
                        iterate(context, 0);
                        if reply.received.is_none() {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    let Some(payload) = reply.received.take() else {
                        // Pending callback remains in stable ReplyState until
                        // unregister; cancel before returning on timeout.
                        if token != 0 {
                            checked(*init, *free, "LS2_CANCEL", |err| cancel(client, token, err))?;
                        }
                        bail!("LS2_{method}_TIMEOUT_2000MS");
                    };
                    let json: serde_json::Value = serde_json::from_str(&payload)
                        .with_context(|| format!("LS2_{method}_INVALID_REPLY"))?;
                    if json.get("returnValue").and_then(serde_json::Value::as_bool) != Some(true) {
                        bail!("LS2_{method}_REJECTED: {payload}");
                    }
                    Ok(json)
                };

            let created = request("createHandle", &serde_json::json!({}))?;
            let handle = created.get("handle").ok_or_else(|| {
                anyhow::anyhow!("C5_CREATE_HANDLE_MISSING_HANDLE; do not continue")
            })?;
            if !valid_capture_handle(handle) {
                bail!("C5_CREATE_HANDLE_INVALID_HANDLE");
            }
            let handle = handle.clone();
            println!(
                "C5_HANDLE_CREATED kind={} no_frame_requested=true",
                if handle.is_number() {
                    "number"
                } else {
                    "string"
                }
            );

            // These requests configure one capture session but explicitly
            // never invoke execute or read pixel data. Always attempt
            // destroyHandle even if any configuration call is rejected.
            let config_result = (|| -> Result<()> {
                if let Some(output) = scoped_output.as_ref() {
                    let properties = serde_json::json!({
                        "handle": handle,
                        "properties": {
                            "method": "BLENDED",
                            "width": 320,
                            "height": 180,
                            "format": "JPEG"
                        }
                    });
                    request("setProperties", &properties)?;
                    request(
                        "setOptions",
                        &serde_json::json!({
                            "handle": handle,
                            "options": {"jpegQuality": 75}
                        }),
                    )?;
                    request(
                        "setOutput",
                        &serde_json::json!({
                            "handle": handle,
                            "file": {"path": output.jpeg_path()}
                        }),
                    )?;
                    println!(
                        "C5_HANDLE_CONFIGURED method=BLENDED format=JPEG width=320 height=180 execute={}",
                        execute_runtime.is_some()
                    );
                    if let Some(runtime) = execute_runtime.as_ref() {
                        // Re-classify before EVERY frame, including the
                        // very first one. Do not proceed after unknown/HDMI/
                        // Protected status or a failed media query.
                        for number in 1..=frame_limit {
                            let path = if frame_limit == 1 {
                                output.jpeg_path()
                            } else {
                                output.numbered_jpeg_path(number)
                            };
                            if frame_limit > 1 {
                                // One unique output path per frame: no stale
                                // file, no overwrite of old frame data.
                                request(
                                    "setOutput",
                                    &serde_json::json!({
                                        "handle": handle,
                                        "file": {"path": path}
                                    }),
                                )?;
                            }
                            let last_class = runtime.block_on(crate::capture::media::query());
                            if last_class != crate::capture::drm_routing::PlaybackClass::Clear {
                                bail!("C5_EXECUTE_REFUSE_MEDIA_CHANGED: stopped before execute {number}");
                            }
                            let began = Instant::now();
                            request("execute", &serde_json::json!({"handle": handle}))?;
                            let elapsed_ms = began.elapsed().as_millis();
                            // Never print, transmit, decode or retain JPEG bytes.
                            // Validate bounded file/markers/geometry, then delete.
                            let file_bytes = verify_private_jpeg(&path)?;
                            if frame_limit == 1 {
                                println!(
                                    "C5_SINGLE_EXECUTE_VALID_JPEG width=320 height=180 bytes={file_bytes} execute_ms={elapsed_ms} frames=1"
                                );
                            } else {
                                println!(
                                    "C5_STAGE5_NATIVE_FRAME index={number} width=320 height=180 bytes={file_bytes} request_ms={elapsed_ms}"
                                );
                            }
                            std::fs::remove_file(&path).context("C5_FRAME_CLEANUP_FAILED")?;
                        }
                    }
                }
                Ok(())
            })();
            let destroy_result = request("destroyHandle", &serde_json::json!({"handle": handle}));
            if destroy_result.is_ok() {
                println!("C5_HANDLE_DESTROYED returned_true=true");
            }
            destroy_result.context("destroyHandle failed after configuration attempt")?;
            config_result?;
            Ok(())
        })();

        let cleanup_result = checked(*init, *free, "LS2_UNREGISTER", |err| {
            unregister(client, err)
        });
        if !context.is_null() {
            context_unref(context);
        }
        cleanup_result.context("native Luna test must unregister cleanly")?;
        test_result?;
        if execute_runtime.is_some() {
            if frame_limit == 5 {
                println!(
                    "C5_STAGE5_NATIVE_PASS frames=5 handles=1 destroyed=1 frame_retained=false"
                );
            } else {
                println!(
                    "C5_SINGLE_EXECUTE_PASS created=1 executed=1 destroyed=1 frame_retained=false"
                );
            }
        } else if configure {
            println!(
                "C5_HANDLE_CONFIGURATION_PASS created=1 configured=true destroyed=1 no_pixels=true"
            );
        } else {
            println!("C5_HANDLE_LIFECYCLE_PASS created=1 destroyed=1 no_pixels=true");
        }
    }
    Ok(())
}

/// Examine only the bounded JPEG signature and SOF dimensions. No decode,
/// no image logging and no file retention (ScopedOutputDir Drop removes it).
pub(crate) fn verify_private_jpeg(path: &std::path::Path) -> Result<usize> {
    const MAX_JPEG: u64 = 2 * 1024 * 1024;
    let metadata = std::fs::symlink_metadata(path).context("C5_EXECUTE_JPEG_MISSING")?;
    if !metadata.file_type().is_file() || metadata.len() < 16 || metadata.len() > MAX_JPEG {
        bail!("C5_EXECUTE_JPEG_NOT_REGULAR_OR_SIZE_OUT_OF_RANGE");
    }
    let file = std::fs::File::open(path).context("C5_EXECUTE_JPEG_OPEN_FAILED")?;
    let mut jpeg = Vec::new();
    file.take(MAX_JPEG + 1).read_to_end(&mut jpeg)?;
    if jpeg.len() as u64 != metadata.len() {
        bail!("C5_EXECUTE_JPEG_SIZE_CHANGED");
    }
    verify_jpeg_dimensions(&jpeg, 320, 180)?;
    Ok(jpeg.len())
}

fn verify_jpeg_dimensions(jpeg: &[u8], expected_width: u16, expected_height: u16) -> Result<()> {
    if !jpeg.starts_with(&[0xff, 0xd8]) || !jpeg.ends_with(&[0xff, 0xd9]) || jpeg.len() < 16 {
        bail!("C5_EXECUTE_JPEG_INVALID_MARKERS");
    }
    let mut cursor = 2usize;
    while cursor + 4 <= jpeg.len() {
        if jpeg[cursor] != 0xff {
            bail!("C5_EXECUTE_JPEG_INVALID_SEGMENT");
        }
        while cursor < jpeg.len() && jpeg[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= jpeg.len() {
            bail!("C5_EXECUTE_JPEG_TRUNCATED");
        }
        let marker = jpeg[cursor];
        cursor += 1;
        if matches!(marker, 0xd9 | 0xda) {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if cursor + 2 > jpeg.len() {
            break;
        }
        let length = usize::from(u16::from_be_bytes([jpeg[cursor], jpeg[cursor + 1]]));
        if length < 2
            || cursor
                .checked_add(length)
                .is_none_or(|end| end > jpeg.len())
        {
            bail!("C5_EXECUTE_JPEG_BAD_SEGMENT_LENGTH");
        }
        // Baseline/progressive/lossless SOF, excluding Huffman/DAC markers.
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            if length < 7 {
                bail!("C5_EXECUTE_JPEG_BAD_SOF");
            }
            let height = u16::from_be_bytes([jpeg[cursor + 3], jpeg[cursor + 4]]);
            let width = u16::from_be_bytes([jpeg[cursor + 5], jpeg[cursor + 6]]);
            if width != expected_width || height != expected_height {
                bail!("C5_EXECUTE_JPEG_UNEXPECTED_DIMENSIONS");
            }
            return Ok(());
        }
        cursor += length;
    }
    bail!("C5_EXECUTE_JPEG_SOF_NOT_FOUND")
}

fn valid_capture_handle(handle: &serde_json::Value) -> bool {
    match handle {
        serde_json::Value::Number(n) => n.as_u64().is_some(),
        serde_json::Value::String(s) => !s.is_empty() && s.len() <= 128,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_metadata_gate_accepts_synthetic_dimensions_only() {
        // Metadata-only fixture is not a real decodable image; this test
        // validates the bounded marker and expected geometry parser.
        let fake = [
            0xff, 0xd8, // SOI
            0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, // APP0
            0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0xb4, 0x01, 0x40, 0x01, 0x01, 0x11,
            0x00, // SOF0 width 320 height 180
            0xff, 0xd9, // EOI
        ];
        assert!(verify_jpeg_dimensions(&fake, 320, 180).is_ok());
        assert!(verify_jpeg_dimensions(&fake, 640, 360).is_err());
        assert!(verify_jpeg_dimensions(&fake[..fake.len() - 2], 320, 180).is_err());
    }

    #[test]
    fn handle_validation_rejects_null_and_compound_handles() {
        assert!(valid_capture_handle(&serde_json::json!(42)));
        assert!(valid_capture_handle(&serde_json::json!("capture-id")));
        assert!(!valid_capture_handle(&serde_json::json!(null)));
        assert!(!valid_capture_handle(&serde_json::json!({})));
        assert!(!valid_capture_handle(&serde_json::json!("")));
        assert!(!valid_capture_handle(&serde_json::json!(-5)));
        assert!(run_handle_lifecycle(false).is_err());
        assert!(run_handle_configuration(false).is_err());
    }

    #[test]
    fn explicit_ack_required_without_loading_any_native_library() {
        assert!(run(false)
            .unwrap_err()
            .to_string()
            .contains("--acknowledge-capture-permission-check"));
    }
}
