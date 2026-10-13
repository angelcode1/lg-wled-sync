use anyhow::{anyhow, Context, Result};
use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use tracing::{error, info, warn};

use super::{CaptureError, CapturedFrame, NativeCaptureError, ScreenCapture};

pub const LIBVTCAPTURE_PATH: &str = "/usr/lib/libvtcapture.so.1";

/// Properties passed to `vtCapture_preprocess` (20 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibVtCaptureProperties {
    /// 0 = SCALER_INPUT (HDMI video stream before scaler)
    /// 1 = SCALER_OUTPUT (after hardware scaler)
    /// 2 = DISPLAY_OUTPUT (composited display output)
    pub dump: c_int,
    pub loc_x: i16,
    pub loc_y: i16,
    pub reg_w: i16,
    pub reg_h: i16,
    pub buf_cnt: c_int,
    pub frm: c_int,
}

/// Plane layout information returned by `vtCapture_planeInfo` (20 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LibVtCapturePlaneInfo {
    pub stride: c_int,
    pub plane_x: i16,
    pub plane_y: i16,
    pub plane_w: i16,
    pub plane_h: i16,
    pub active_x: i16,
    pub active_y: i16,
    pub active_w: i16,
    pub active_h: i16,
}

/// Active frame buffer memory addresses from `vtCapture_currentCaptureBuffInfo` (16 bytes on 32-bit ARM).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LibVtCaptureBufferInfo {
    /// Pointer to Y plane (luma)
    pub start_addr0: *mut u8,
    /// Pointer to UV interleaved plane (chroma NV12)
    pub start_addr1: *mut u8,
    pub size0: c_int,
    pub size1: c_int,
}

type FnVtCaptureCreate = unsafe extern "C" fn() -> *mut c_void;
type FnVtCaptureInit = unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_char) -> c_int;
type FnVtCapturePreprocess =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const LibVtCaptureProperties) -> c_int;
type FnVtCapturePlaneInfo =
    unsafe extern "C" fn(*mut c_void, *const c_char, *mut LibVtCapturePlaneInfo) -> c_int;
type FnVtCaptureProcess = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type FnVtCaptureCurrentCaptureBuffInfo =
    unsafe extern "C" fn(*mut c_void, *mut LibVtCaptureBufferInfo) -> c_int;
type FnVtCaptureStop = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type FnVtCapturePostprocess = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type FnVtCaptureFinalize = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
// Native vtCapture_release dispatches driver cleanup via the vtable; it has
// no documented integer status on the audited C5. Never interpret r0 as rc.
type FnVtCaptureRelease = unsafe extern "C" fn(*mut c_void);

// The C5's native C++ code can throw during LS2/Resource Manager registration.
// Every Linux native frame call, including cleanup and Drop, goes through the
// typed noexcept C++ barrier. No vendor C++ exception crosses Rust FFI.
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn lg_frame_guard_create(
        f: FnVtCaptureCreate,
        out: *mut *mut c_void,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_init(
        f: FnVtCaptureInit,
        driver: *mut c_void,
        caller: *const c_char,
        client: *mut c_char,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_preprocess(
        f: FnVtCapturePreprocess,
        driver: *mut c_void,
        client: *const c_char,
        props: *const LibVtCaptureProperties,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_plane_info(
        f: FnVtCapturePlaneInfo,
        driver: *mut c_void,
        client: *const c_char,
        plane: *mut LibVtCapturePlaneInfo,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_process(
        f: FnVtCaptureProcess,
        driver: *mut c_void,
        client: *const c_char,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_current_buffer(
        f: FnVtCaptureCurrentCaptureBuffInfo,
        driver: *mut c_void,
        buffer: *mut LibVtCaptureBufferInfo,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_stop(
        f: FnVtCaptureStop,
        driver: *mut c_void,
        client: *const c_char,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_postprocess(
        f: FnVtCapturePostprocess,
        driver: *mut c_void,
        client: *const c_char,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_finalize(
        f: FnVtCaptureFinalize,
        driver: *mut c_void,
        client: *const c_char,
        rc: *mut c_int,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
    fn lg_frame_guard_release(
        f: FnVtCaptureRelease,
        driver: *mut c_void,
        err: *mut c_char,
        cap: usize,
    ) -> c_int;
}

#[cfg(target_os = "linux")]
fn frame_guard_check(operation: &str, boundary: c_int, error: &[c_char; 256]) -> Result<()> {
    if boundary != 0 {
        let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
        return Err(anyhow!(
            "LG C++ frame operation {operation} threw or was rejected by guard: {message}"
        ));
    }
    Ok(())
}

macro_rules! frame_native {
    ($guard:ident, $operation:literal, $fn_ptr:expr $(, $arg:expr)* $(,)?) => {{
        #[cfg(target_os = "linux")]
        {
            let mut rc = 0 as c_int;
            let mut error = [0 as c_char; 256];
            // This macro is invoked both inside and outside existing unsafe
            // scopes. The local FFI boundary remains explicit in both cases.
            #[allow(unused_unsafe)]
            let outcome = unsafe {
                $guard($fn_ptr, $($arg,)* &mut rc, error.as_mut_ptr(), error.len())
            };
            frame_guard_check($operation, outcome, &error).map(|()| rc)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok::<c_int, anyhow::Error>(unsafe { ($fn_ptr)($($arg),*) })
        }
    }};
}

// The C5's release entry point tail-dispatches via the driver vtable.
// It is a cleanup operation, NOT an integer-returning status API.
// Report only the C++ guard boundary result, including exceptions.
fn frame_release(f: FnVtCaptureRelease, driver: *mut c_void) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut error = [0 as c_char; 256];
        let outcome = unsafe { lg_frame_guard_release(f, driver, error.as_mut_ptr(), error.len()) };
        frame_guard_check("release", outcome, &error)
    }
    #[cfg(not(target_os = "linux"))]
    {
        unsafe { f(driver) };
        Ok(())
    }
}

unsafe fn frame_create(f: FnVtCaptureCreate) -> Result<*mut c_void> {
    #[cfg(target_os = "linux")]
    {
        let mut out = std::ptr::null_mut();
        let mut error = [0 as c_char; 256];
        let outcome = lg_frame_guard_create(f, &mut out, error.as_mut_ptr(), error.len());
        frame_guard_check("create", outcome, &error)?;
        Ok(out)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(f())
    }
}

// Reject malformed or oversized metadata before native buffer pointers are dereferenced.
// A valid C5 layout must be established independently of our requested resolution.
fn checked_plane_dimensions(plane: &LibVtCapturePlaneInfo) -> Result<(u32, u32, u32)> {
    if plane.plane_w <= 0 || plane.plane_h <= 0 || plane.stride <= 0 {
        return Err(anyhow!("LG frame plane has absent dimensions or stride"));
    }
    let width = plane.plane_w as u32;
    let height = plane.plane_h as u32;
    let stride = plane.stride as u32;
    if width > 4096
        || height > 2160
        || stride > 8192
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || !stride.is_multiple_of(2)
        || stride < width
    {
        return Err(anyhow!(
            "LG frame plane exceeds bounded NV12 geometry or is misaligned"
        ));
    }
    nv12_lengths(width, height, stride)?;
    Ok((width, height, stride))
}

/// Native libvtcapture driver implementation for LG webOS 6.x (LG C1 OLED).
pub struct VtCapture {
    driver: *mut c_void,
    client_id: [u8; 128],
    width: u32,
    height: u32,
    stride: u32,
    rgb_buffer: Vec<u8>,
    is_stopped: bool,
    last_addr0: usize,
    stale_count: u32,
    raw_stats_enabled: bool,
    last_raw_stats: Option<RawNv12Stats>,
    fn_current_buff_info: FnVtCaptureCurrentCaptureBuffInfo,
    fn_stop: FnVtCaptureStop,
    fn_postprocess: FnVtCapturePostprocess,
    fn_finalize: FnVtCaptureFinalize,
    fn_release: FnVtCaptureRelease,
    _lib: Library,
}

/// Aggregate of a central, at-most-16x16 NV12 patch; no frame pixels are retained.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawNv12Stats {
    pub y_min: u8,
    pub y_mean: f32,
    pub y_max: u8,
    pub u_mean: f32,
    pub v_mean: f32,
    pub luma_samples: u16,
}

fn central_nv12_stats(
    width: u32,
    height: u32,
    stride: u32,
    y_plane: &[u8],
    uv_plane: &[u8],
) -> Result<RawNv12Stats> {
    let (y_len, uv_len, _) = nv12_lengths(width, height, stride)?;
    if y_plane.len() < y_len || uv_plane.len() < uv_len {
        return Err(anyhow!("NV12 plane is too short for raw statistics"));
    }
    let (w, h, s) = (width as usize, height as usize, stride as usize);
    let x0 = (w.saturating_sub(16) / 2) & !1;
    let y0 = (h.saturating_sub(16) / 2) & !1;
    let x1 = (x0 + 16).min(w);
    let y1 = (y0 + 16).min(h);
    let mut y_min = u8::MAX;
    let mut y_max = u8::MIN;
    let mut y_sum = 0u32;
    for row in y0..y1 {
        for &value in &y_plane[row * s + x0..row * s + x1] {
            y_min = y_min.min(value);
            y_max = y_max.max(value);
            y_sum += u32::from(value);
        }
    }
    let mut u_sum = 0u32;
    let mut v_sum = 0u32;
    let mut chroma_samples = 0u32;
    for row in (y0..y1).step_by(2) {
        for col in (x0..x1).step_by(2) {
            let offset = row / 2 * s + col;
            u_sum += u32::from(uv_plane[offset]);
            v_sum += u32::from(uv_plane[offset + 1]);
            chroma_samples += 1;
        }
    }
    let luma_samples = ((x1 - x0) * (y1 - y0)) as u16;
    Ok(RawNv12Stats {
        y_min,
        y_mean: y_sum as f32 / f32::from(luma_samples),
        y_max,
        u_mean: u_sum as f32 / chroma_samples as f32,
        v_mean: v_sum as f32 / chroma_samples as f32,
        luma_samples,
    })
}

// A descriptor is metadata only. Presence/length checks cannot establish
// whether raw native pointers refer to mapped or accessible memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DescriptorSummary {
    y_present: bool,
    uv_present: bool,
    y_size: usize,
    uv_size: usize,
}

fn validate_buffer_descriptor(
    descriptor: &LibVtCaptureBufferInfo,
    layout: (u32, u32, u32),
) -> Result<DescriptorSummary> {
    let (width, height, stride) = layout;
    let (minimum_y, minimum_uv, _) = nv12_lengths(width, height, stride)?;
    let y_present = !descriptor.start_addr0.is_null();
    let uv_present = !descriptor.start_addr1.is_null();
    if !y_present || !uv_present {
        return Err(anyhow!(
            "native descriptor missing Y or UV pointer (y_present={y_present} uv_present={uv_present}); no pointer dereferenced"
        ));
    }
    if descriptor.size0 < 0 || descriptor.size1 < 0 {
        return Err(anyhow!("native buffer descriptor has negative plane size"));
    }
    let y_size = descriptor.size0 as usize;
    let uv_size = descriptor.size1 as usize;
    // Bound corrupted metadata; these values are NOT trusted allocation sizes.
    if y_size < minimum_y || uv_size < minimum_uv {
        return Err(anyhow!(
            "native buffer descriptor plane sizes smaller than required 320x180 layout"
        ));
    }
    if y_size > 16 * 1024 * 1024 || uv_size > 16 * 1024 * 1024 {
        return Err(anyhow!(
            "native buffer descriptor sizes exceed sanity limits"
        ));
    }
    Ok(DescriptorSummary {
        y_present,
        uv_present,
        y_size,
        uv_size,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadableSpan {
    start: usize,
    end: usize,
}

/// Interpret /proc/self/maps as a bounded, read-only VMA inventory. File
/// names, absolute addresses and mapping identities are never logged.
fn parse_readable_spans(maps: &str) -> Result<Vec<ReadableSpan>> {
    let mut spans = Vec::new();
    for line in maps.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let range = fields.next().ok_or_else(|| anyhow!("missing VMA range"))?;
        let perms = fields
            .next()
            .ok_or_else(|| anyhow!("missing VMA permissions"))?;
        let (start, end) = range
            .split_once('-')
            .ok_or_else(|| anyhow!("malformed VMA address range"))?;
        let start = usize::from_str_radix(start, 16).context("invalid VMA starting address")?;
        let end = usize::from_str_radix(end, 16).context("invalid VMA ending address")?;
        if start >= end {
            return Err(anyhow!("invalid VMA address ordering"));
        }
        if perms.as_bytes().first() == Some(&b'r') {
            spans.push(ReadableSpan { start, end });
        }
    }
    spans.sort_unstable_by_key(|span| span.start);
    Ok(spans)
}

/// Checking the address is arithmetic only: NEVER create a pointer slice or
/// dereference the vendor-provided address. Adjacent readable VMAs may cover
/// one buffer, but an unreadable gap fails closed.
fn span_is_readable(addr: usize, size: usize, spans: &[ReadableSpan]) -> bool {
    if addr == 0 || size == 0 {
        return false;
    }
    let Some(end) = addr.checked_add(size) else {
        return false;
    };
    let mut cursor = addr;
    for span in spans {
        if span.end <= cursor {
            continue;
        }
        if span.start > cursor {
            return false;
        }
        cursor = cursor.max(span.end);
        if cursor >= end {
            return true;
        }
    }
    false
}

/// This proves only *reported mapping coverage at the time of inspection*.
/// It does not prove the pages are resident, correctly owned, synchronized,
/// DRM-clear, or safe for subsequent raw Rust pointer dereferencing.
fn check_descriptor_readable_mappings(
    descriptor: &LibVtCaptureBufferInfo,
    summary: DescriptorSummary,
    maps: &str,
) -> Result<()> {
    let spans = parse_readable_spans(maps)?;
    let y_addr = descriptor.start_addr0 as usize;
    let uv_addr = descriptor.start_addr1 as usize;
    let y_end = y_addr
        .checked_add(summary.y_size)
        .ok_or_else(|| anyhow!("Y range overflow"))?;
    let uv_end = uv_addr
        .checked_add(summary.uv_size)
        .ok_or_else(|| anyhow!("UV range overflow"))?;
    if y_addr < uv_end && uv_addr < y_end {
        return Err(anyhow!("native Y and UV descriptor ranges overlap"));
    }
    if !span_is_readable(y_addr, summary.y_size, &spans) {
        return Err(anyhow!(
            "native Y descriptor is not fully covered by readable mappings"
        ));
    }
    if !span_is_readable(uv_addr, summary.uv_size, &spans) {
        return Err(anyhow!(
            "native UV descriptor is not fully covered by readable mappings"
        ));
    }
    Ok(())
}

/**
 * One-shot native probe owner. Once preprocess has been attempted, even a
 * native error may have partially armed the scaler: attempt postprocess before
 * finalize and release. Only the selected explicit diagnostic may call process or descriptor APIs; no native buffer is ever dereferenced.
 */
struct C5MetadataSession {
    driver: *mut c_void,
    client_id: [u8; 128],
    initialized: bool,
    preprocess_attempted: bool,
    process_attempted: bool,
    released: bool,
    stop: FnVtCaptureStop,
    postprocess: FnVtCapturePostprocess,
    finalize: FnVtCaptureFinalize,
    release: FnVtCaptureRelease,
}

impl C5MetadataSession {
    fn cleanup(&mut self) -> Result<()> {
        if self.released {
            return Ok(());
        }
        let mut issues = Vec::new();
        let client = self.client_id.as_ptr() as *const c_char;
        // A failed/throwing process may have partially started capture.
        // Attempt stop before postprocess in that case; never read buffers.
        if self.process_attempted {
            match frame_native!(lg_frame_guard_stop, "stop", self.stop, self.driver, client) {
                Ok(0) => {}
                Ok(code) => issues.push(format!("stop returned {code}")),
                Err(error) => issues.push(format!("stop guard: {error:#}")),
            }
        }
        if self.preprocess_attempted {
            match frame_native!(
                lg_frame_guard_postprocess,
                "postprocess",
                self.postprocess,
                self.driver,
                client
            ) {
                Ok(0) => {}
                Ok(code) => issues.push(format!("postprocess returned {code}")),
                Err(error) => issues.push(format!("postprocess guard: {error:#}")),
            }
        }
        if self.initialized {
            match frame_native!(
                lg_frame_guard_finalize,
                "finalize",
                self.finalize,
                self.driver,
                client
            ) {
                Ok(0) => {}
                Ok(code) => issues.push(format!("finalize returned {code}")),
                Err(error) => issues.push(format!("finalize guard: {error:#}")),
            }
        }

        // Never repeat a release, even if its native destructor threw after
        // partially destroying the driver. The C++ guard handles exceptions.
        self.released = true;
        if let Err(error) = frame_release(self.release, self.driver) {
            issues.push(format!("release guard: {error:#}"));
        }
        self.driver = std::ptr::null_mut();

        if issues.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("C5 metadata probe cleanup: {}", issues.join("; ")))
        }
    }
}

impl Drop for C5MetadataSession {
    fn drop(&mut self) {
        if !self.released {
            if let Err(error) = self.cleanup() {
                warn!("C5 metadata probe fallback cleanup: {error:#}");
            }
        }
    }
}

impl VtCapture {
    /// Bounded, explicit native *initialization-only* diagnostic.
    ///
    /// This may acquire LG Resource Manager / LS2 ownership temporarily, so
    /// callers MUST verify the source is positively classified Clear first.
    /// Never preprocess, select a capture mode, start a stream, request buffer
    /// metadata, or read frame pixels. Respect all vendor permission refusals.
    pub fn probe_init_only() -> Result<()> {
        if !Path::new(LIBVTCAPTURE_PATH).exists() {
            return Err(anyhow!("libvtcapture is absent"));
        }
        let lib = unsafe { Library::new(LIBVTCAPTURE_PATH) }
            .context("failed to load native LG capture library for init-only probe")?;
        let caller = CString::new("org.webosbrew.lg-hue-sync")?;
        let mut client_id = [0u8; 128];
        client_id[0] = b'0';
        client_id[1] = b'0';

        unsafe {
            let create: Symbol<FnVtCaptureCreate> = lib.get(b"vtCapture_create\0")?;
            let init: Symbol<FnVtCaptureInit> = lib.get(b"vtCapture_init\0")?;
            let finalize: Symbol<FnVtCaptureFinalize> = lib.get(b"vtCapture_finalize\0")?;
            let release: Symbol<FnVtCaptureRelease> = lib.get(b"vtCapture_release\0")?;

            let driver = frame_create(*create)?;
            if driver.is_null() {
                return Err(anyhow!("vtCapture_create returned null"));
            }

            let init_result = frame_native!(
                lg_frame_guard_init,
                "init",
                *init,
                driver,
                caller.as_ptr(),
                client_id.as_mut_ptr() as *mut c_char
            );

            // Only an initialized client may be finalized. A denied or
            // throwing init is released, never retried with another caller.
            let finalize_result = if matches!(init_result, Ok(0)) {
                Some(frame_native!(
                    lg_frame_guard_finalize,
                    "finalize",
                    *finalize,
                    driver,
                    client_id.as_ptr() as *const c_char
                ))
            } else {
                None
            };

            // Attempt release even if init/finalize returned an error.
            let release_result = frame_release(*release, driver);

            let init_code = init_result?;
            if init_code != 0 {
                if let Err(error) = release_result {
                    warn!("Native release after init refusal: {error:#}");
                }
                return Err(NativeCaptureError {
                    backend: "libvtcapture",
                    operation: "vtCapture_init",
                    code: init_code,
                }
                .into());
            }

            let final_code = finalize_result.expect("finalize required after a successful init")?;
            if final_code != 0 {
                return Err(NativeCaptureError {
                    backend: "libvtcapture",
                    operation: "vtCapture_finalize",
                    code: final_code,
                }
                .into());
            }
            // A completed, non-throwing release has no integer status.
            release_result?;
            info!("C5_FRAME_INIT_ONLY_PASS: initialized and released; no preprocess or frame read");
            Ok(())
        }
    }

    /// Single, opt-in preprocessing/plane-layout diagnostic. Preprocess may
    /// briefly arm the LG display-output scaler; no frame processing, buffer
    /// requests, pixel reads, output devices, or mode fallbacks are permitted.
    /// Call only after three fresh, positively Clear media classifications.
    pub fn probe_preprocess_metadata() -> Result<()> {
        Self::probe_preprocess_once(false, false, false)
    }

    /// One native process invocation on a known-clear display-output source.
    /// No buffer metadata requests or pixel reads. Process MAY enable a stream.
    pub fn probe_process_no_buffers() -> Result<()> {
        Self::probe_preprocess_once(true, false, false)
    }

    /// Exactly one request for the native buffer *descriptor*, not its bytes.
    /// The returned pointers are treated as opaque: never logged, copied or
    /// dereferenced. The source must have been independently verified Clear.
    pub fn probe_buffer_descriptor_once() -> Result<()> {
        Self::probe_preprocess_once(true, true, false)
    }

    /// Validate the Y and UV descriptor address ranges against readable VMAs
    /// while the native capture session is alive. No pixel loads or copies.
    pub fn probe_buffer_mappings_once() -> Result<()> {
        Self::probe_preprocess_once(true, true, true)
    }

    fn probe_preprocess_once(
        process_once: bool,
        descriptor_once: bool,
        mapping_once: bool,
    ) -> Result<()> {
        if mapping_once && (!process_once || !descriptor_once) {
            return Err(anyhow!(
                "mapping-only probe requires a processed descriptor"
            ));
        }
        let lib = unsafe { Library::new(LIBVTCAPTURE_PATH) }
            .context("unable to load LG VT library for metadata-only probe")?;
        let caller = CString::new("org.webosbrew.lg-hue-sync")?;

        // Resolve all required symbols before creating the native driver.
        unsafe {
            let create: Symbol<FnVtCaptureCreate> = lib.get(b"vtCapture_create\0")?;
            let init: Symbol<FnVtCaptureInit> = lib.get(b"vtCapture_init\0")?;
            let preprocess: Symbol<FnVtCapturePreprocess> = lib.get(b"vtCapture_preprocess\0")?;
            let plane_info: Symbol<FnVtCapturePlaneInfo> = lib.get(b"vtCapture_planeInfo\0")?;
            let process: Symbol<FnVtCaptureProcess> = lib.get(b"vtCapture_process\0")?;
            let current_buffer: Symbol<FnVtCaptureCurrentCaptureBuffInfo> =
                lib.get(b"vtCapture_currentCaptureBuffInfo\0")?;
            let stop: Symbol<FnVtCaptureStop> = lib.get(b"vtCapture_stop\0")?;
            let postprocess: Symbol<FnVtCapturePostprocess> =
                lib.get(b"vtCapture_postprocess\0")?;
            let finalize: Symbol<FnVtCaptureFinalize> = lib.get(b"vtCapture_finalize\0")?;
            let release: Symbol<FnVtCaptureRelease> = lib.get(b"vtCapture_release\0")?;

            let driver = frame_create(*create)?;
            if driver.is_null() {
                return Err(anyhow!("vtCapture_create returned null"));
            }

            let mut session = C5MetadataSession {
                driver,
                client_id: [0u8; 128],
                initialized: false,
                preprocess_attempted: false,
                process_attempted: false,
                released: false,
                stop: *stop,
                postprocess: *postprocess,
                finalize: *finalize,
                release: *release,
            };
            session.client_id[..2].copy_from_slice(b"00");

            // The closure must not return directly from the outer scope:
            // explicit cleanup has to run for both success and native refusal.
            let operation = (|| -> Result<((u32, u32, u32), Option<DescriptorSummary>)> {
                let init_code = frame_native!(
                    lg_frame_guard_init,
                    "init",
                    *init,
                    session.driver,
                    caller.as_ptr(),
                    session.client_id.as_mut_ptr() as *mut c_char
                )?;
                if init_code != 0 {
                    return Err(NativeCaptureError {
                        backend: "libvtcapture",
                        operation: "vtCapture_init",
                        code: init_code,
                    }
                    .into());
                }
                session.initialized = true;
                CStr::from_bytes_until_nul(&session.client_id)
                    .context("native init returned an unterminated client ID")?;

                // One fixed, low-resolution display-output request. Never try
                // HDMI/scaler-input alternatives after an error or refusal.
                let props = LibVtCaptureProperties {
                    dump: 2,
                    loc_x: 0,
                    loc_y: 0,
                    reg_w: 320,
                    reg_h: 180,
                    buf_cnt: 3,
                    frm: 30,
                };
                session.preprocess_attempted = true;
                let prep_code = frame_native!(
                    lg_frame_guard_preprocess,
                    "preprocess",
                    *preprocess,
                    session.driver,
                    session.client_id.as_ptr() as *const c_char,
                    &props
                )?;
                if prep_code != 0 {
                    return Err(NativeCaptureError {
                        backend: "libvtcapture",
                        operation: "vtCapture_preprocess",
                        code: prep_code,
                    }
                    .into());
                }

                let mut plane = LibVtCapturePlaneInfo::default();
                let plane_code = frame_native!(
                    lg_frame_guard_plane_info,
                    "plane-info",
                    *plane_info,
                    session.driver,
                    session.client_id.as_ptr() as *const c_char,
                    &mut plane
                )?;
                if plane_code != 0 {
                    return Err(NativeCaptureError {
                        backend: "libvtcapture",
                        operation: "vtCapture_planeInfo",
                        code: plane_code,
                    }
                    .into());
                }
                let layout = checked_plane_dimensions(&plane)
                    .context("native plane metadata failed bounds validation")?;

                if process_once {
                    // Mark attempted before entry: a native error or exception
                    // may occur after partial stream activation.
                    session.process_attempted = true;
                    let process_code = frame_native!(
                        lg_frame_guard_process,
                        "process",
                        *process,
                        session.driver,
                        session.client_id.as_ptr() as *const c_char
                    )?;
                    if process_code != 0 {
                        return Err(NativeCaptureError {
                            backend: "libvtcapture",
                            operation: "vtCapture_process",
                            code: process_code,
                        }
                        .into());
                    }
                }

                // One native descriptor fetch; no retry, pointer arithmetic,
                // buffer reading, slices, hashes, or image/light output.
                // An error code is terminal and cleanup is still performed.
                let summary = if descriptor_once {
                    let mut descriptor = LibVtCaptureBufferInfo {
                        start_addr0: std::ptr::null_mut(),
                        start_addr1: std::ptr::null_mut(),
                        size0: 0,
                        size1: 0,
                    };
                    let rc = frame_native!(
                        lg_frame_guard_current_buffer,
                        "current-buffer-descriptor",
                        *current_buffer,
                        session.driver,
                        &mut descriptor
                    )?;
                    if rc != 0 {
                        return Err(NativeCaptureError {
                            backend: "libvtcapture",
                            operation: "vtCapture_currentCaptureBuffInfo",
                            code: rc,
                        }
                        .into());
                    }
                    let validated = validate_buffer_descriptor(&descriptor, layout)?;
                    if mapping_once {
                        let maps = std::fs::read_to_string("/proc/self/maps")
                            .context("cannot inspect own process VMA map")?;
                        check_descriptor_readable_mappings(&descriptor, validated, &maps)?;
                    }
                    Some(validated)
                } else {
                    None
                };
                Ok((layout, summary))
            })();

            let cleanup = session.cleanup();
            match (operation, cleanup) {
                (Ok(((width, height, stride), summary)), Ok(())) => {
                    if let Some(summary) = summary {
                        if mapping_once {
                            info!(
                                "C5_FRAME_MAPPING_PASS: display-output plane={}x{} stride={} y_size={} uv_size={} y_vma_readable=true uv_vma_readable=true descriptor_calls=1 frame_reads=0 clean_shutdown=true",
                                width, height, stride, summary.y_size, summary.uv_size
                            );
                        } else {
                            info!(
                                "C5_FRAME_DESCRIPTOR_PASS: display-output plane={}x{} stride={} y_present={} uv_present={} y_size={} uv_size={} descriptor_calls=1 frame_reads=0 clean_shutdown=true",
                                width, height, stride,
                                summary.y_present, summary.uv_present,
                                summary.y_size, summary.uv_size
                            );
                        }
                    } else if process_once {
                        info!(
                            "C5_FRAME_PROCESS_PASS: display-output plane={}x{} stride={} process_calls=1 frame_reads=0 clean_shutdown=true",
                            width, height, stride
                        );
                    } else {
                        info!(
                            "C5_FRAME_METADATA_PASS: display-output plane={}x{} stride={} clean_shutdown=true frame_reads=0",
                            width, height, stride
                        );
                    }
                    Ok(())
                }
                (Err(error), Ok(())) => Err(error),
                (Ok(_), Err(error)) => Err(error),
                (Err(native), Err(cleanup)) => {
                    Err(native.context(format!("native cleanup also failed: {cleanup:#}")))
                }
            }
        }
    }

    pub fn enable_raw_stats(&mut self) {
        self.raw_stats_enabled = true;
    }

    pub fn take_raw_stats(&mut self) -> Option<RawNv12Stats> {
        self.last_raw_stats.take()
    }

    /// Attempts to dynamically load `/usr/lib/libvtcapture.so.1` and initialize the capture pipeline.
    pub fn try_new(target_width: u32, target_height: u32) -> Result<Self> {
        if target_width == 0
            || target_height == 0
            || target_width > i16::MAX as u32
            || target_height > i16::MAX as u32
        {
            return Err(anyhow!("capture dimensions exceed libvtcapture limits"));
        }
        nv12_lengths(target_width, target_height, target_width + target_width % 2)?;
        if !Path::new(LIBVTCAPTURE_PATH).exists() {
            return Err(anyhow!(
                "libvtcapture driver library not found at {}",
                LIBVTCAPTURE_PATH
            ));
        }

        info!(
            "Loading webOS libvtcapture driver from {}...",
            LIBVTCAPTURE_PATH
        );
        let lib = unsafe { Library::new(LIBVTCAPTURE_PATH) }
            .map_err(|e| anyhow!("Failed to dlopen {}: {}", LIBVTCAPTURE_PATH, e))?;

        unsafe {
            let fn_create: Symbol<FnVtCaptureCreate> = lib
                .get(b"vtCapture_create\0")
                .map_err(|e| anyhow!("Symbol vtCapture_create not found: {}", e))?;
            let fn_init: Symbol<FnVtCaptureInit> = lib
                .get(b"vtCapture_init\0")
                .map_err(|e| anyhow!("Symbol vtCapture_init not found: {}", e))?;
            let fn_preprocess: Symbol<FnVtCapturePreprocess> =
                lib.get(b"vtCapture_preprocess\0")
                    .map_err(|e| anyhow!("Symbol vtCapture_preprocess not found: {}", e))?;
            let fn_plane_info: Symbol<FnVtCapturePlaneInfo> = lib
                .get(b"vtCapture_planeInfo\0")
                .map_err(|e| anyhow!("Symbol vtCapture_planeInfo not found: {}", e))?;
            let fn_process: Symbol<FnVtCaptureProcess> = lib
                .get(b"vtCapture_process\0")
                .map_err(|e| anyhow!("Symbol vtCapture_process not found: {}", e))?;
            let fn_current_buff_info: Symbol<FnVtCaptureCurrentCaptureBuffInfo> = lib
                .get(b"vtCapture_currentCaptureBuffInfo\0")
                .map_err(|e| anyhow!("Symbol vtCapture_currentCaptureBuffInfo not found: {}", e))?;
            let fn_stop: Symbol<FnVtCaptureStop> = lib
                .get(b"vtCapture_stop\0")
                .map_err(|e| anyhow!("Symbol vtCapture_stop not found: {}", e))?;
            let fn_postprocess: Symbol<FnVtCapturePostprocess> = lib
                .get(b"vtCapture_postprocess\0")
                .map_err(|e| anyhow!("Symbol vtCapture_postprocess not found: {}", e))?;
            let fn_finalize: Symbol<FnVtCaptureFinalize> = lib
                .get(b"vtCapture_finalize\0")
                .map_err(|e| anyhow!("Symbol vtCapture_finalize not found: {}", e))?;
            let fn_release: Symbol<FnVtCaptureRelease> = lib
                .get(b"vtCapture_release\0")
                .map_err(|e| anyhow!("Symbol vtCapture_release not found: {}", e))?;

            // 1. Create driver instance
            let driver = frame_create(*fn_create)?;
            if driver.is_null() {
                return Err(anyhow!("vtCapture_create returned NULL"));
            }

            // 2. Initialize with caller ID and initial "00" client ID buffer
            let mut client_id = [0u8; 128];
            client_id[0] = b'0';
            client_id[1] = b'0';
            let caller_id = CString::new("org.webosbrew.lg-hue-sync")?;

            let ret_init = match frame_native!(
                lg_frame_guard_init,
                "init",
                *fn_init,
                driver,
                caller_id.as_ptr(),
                client_id.as_mut_ptr() as *mut c_char
            ) {
                Ok(rc) => rc,
                Err(err) => {
                    let _ = frame_release(*fn_release, driver);
                    return Err(err);
                }
            };
            if ret_init != 0 {
                let _ = frame_release(*fn_release, driver);
                if ret_init == 11 {
                    error!(
                        "[-] vtCapture_init returned native status 11 (Resource Manager acquisition failed in the audited C5 ELF); not necessarily errno EBUSY"
                    );
                }
                return Err(NativeCaptureError {
                    backend: "libvtcapture",
                    operation: "vtCapture_init",
                    code: ret_init,
                }
                .into());
            }

            let client_str = match CStr::from_bytes_until_nul(&client_id) {
                Ok(value) => value.to_string_lossy(),
                Err(_) => {
                    let _ = frame_release(*fn_release, driver);
                    return Err(anyhow!("vtCapture_init returned an unterminated client ID"));
                }
            };
            info!(
                "[+] vtCapture_init successful (caller: {}, client_id: '{}')",
                caller_id.to_string_lossy(),
                client_str
            );

            // 3. Use one audited capture mode only. Do not switch source
            // modes after a native refusal (which may be a policy denial).
            let client_ptr = client_id.as_ptr() as *const c_char;
            let props = LibVtCaptureProperties {
                dump: 2, // DISPLAY_OUTPUT; C5 hardware support still unverified.
                loc_x: 0,
                loc_y: 0,
                reg_w: target_width as i16,
                reg_h: target_height as i16,
                buf_cnt: 3,
                frm: 60,
            };
            let ret_pre = match frame_native!(
                lg_frame_guard_preprocess,
                "preprocess",
                *fn_preprocess,
                driver,
                client_ptr,
                &props
            ) {
                Ok(rc) => rc,
                Err(err) => {
                    let _ = frame_native!(
                        lg_frame_guard_finalize,
                        "finalize",
                        *fn_finalize,
                        driver,
                        client_ptr
                    );
                    let _ = frame_release(*fn_release, driver);
                    return Err(err);
                }
            };
            if ret_pre != 0 {
                let _ = frame_native!(
                    lg_frame_guard_finalize,
                    "finalize",
                    *fn_finalize,
                    driver,
                    client_ptr
                );
                let _ = frame_release(*fn_release, driver);
                return Err(NativeCaptureError {
                    backend: "libvtcapture",
                    operation: "vtCapture_preprocess",
                    code: ret_pre,
                }
                .into());
            }
            info!("vtCapture_preprocess completed for selected display-output mode");

            // 4. Query plane layout and hardware stride
            let mut plane_info = LibVtCapturePlaneInfo::default();
            let ret_plane = match frame_native!(
                lg_frame_guard_plane_info,
                "plane-info",
                *fn_plane_info,
                driver,
                client_ptr,
                &mut plane_info
            ) {
                Ok(rc) => rc,
                Err(err) => {
                    let _ = frame_native!(
                        lg_frame_guard_finalize,
                        "finalize",
                        *fn_finalize,
                        driver,
                        client_ptr
                    );
                    let _ = frame_release(*fn_release, driver);
                    return Err(err);
                }
            };
            if ret_plane != 0 {
                let _ = frame_native!(
                    lg_frame_guard_finalize,
                    "finalize",
                    *fn_finalize,
                    driver,
                    client_ptr
                );
                let _ = frame_release(*fn_release, driver);
                return Err(NativeCaptureError {
                    backend: "libvtcapture",
                    operation: "vtCapture_planeInfo",
                    code: ret_plane,
                }
                .into());
            }
            let (width, height, stride) = match checked_plane_dimensions(&plane_info) {
                Ok(layout) => layout,
                Err(error) => {
                    let _ = frame_native!(
                        lg_frame_guard_finalize,
                        "finalize",
                        *fn_finalize,
                        driver,
                        client_ptr
                    );
                    let _ = frame_release(*fn_release, driver);
                    return Err(error);
                }
            };

            let rgb_size = match nv12_lengths(width, height, stride) {
                Ok((_, _, size)) => size,
                Err(error) => {
                    let _ = frame_native!(
                        lg_frame_guard_finalize,
                        "finalize",
                        *fn_finalize,
                        driver,
                        client_ptr
                    );
                    let _ = frame_release(*fn_release, driver);
                    return Err(error);
                }
            };

            info!(
                "[+] vtCapture plane layout: stride={}, plane={}x{} (target: {}x{}), active={}x{}",
                stride,
                width,
                height,
                target_width,
                target_height,
                plane_info.active_w,
                plane_info.active_h
            );

            // 5. Start streaming capture process
            let ret_proc = match frame_native!(
                lg_frame_guard_process,
                "process",
                *fn_process,
                driver,
                client_ptr
            ) {
                Ok(rc) => rc,
                Err(err) => {
                    let _ =
                        frame_native!(lg_frame_guard_stop, "stop", *fn_stop, driver, client_ptr);
                    let _ = frame_native!(
                        lg_frame_guard_postprocess,
                        "postprocess",
                        *fn_postprocess,
                        driver,
                        client_ptr
                    );
                    let _ = frame_native!(
                        lg_frame_guard_finalize,
                        "finalize",
                        *fn_finalize,
                        driver,
                        client_ptr
                    );
                    let _ = frame_release(*fn_release, driver);
                    return Err(err);
                }
            };
            if ret_proc != 0 {
                let _ = frame_native!(lg_frame_guard_stop, "stop", *fn_stop, driver, client_ptr);
                let _ = frame_native!(
                    lg_frame_guard_postprocess,
                    "postprocess",
                    *fn_postprocess,
                    driver,
                    client_ptr
                );
                let _ = frame_native!(
                    lg_frame_guard_finalize,
                    "finalize",
                    *fn_finalize,
                    driver,
                    client_ptr
                );
                let _ = frame_release(*fn_release, driver);
                return Err(anyhow!("vtCapture_process failed with code {}", ret_proc));
            }

            info!("[+] vtCapture_process accepted the existing client ID (stream setup is part of preprocess)");

            Ok(Self {
                driver,
                client_id,
                width,
                height,
                stride,
                rgb_buffer: vec![0; rgb_size],
                is_stopped: false,
                last_addr0: 0,
                stale_count: 0,
                raw_stats_enabled: false,
                last_raw_stats: None,
                fn_current_buff_info: *fn_current_buff_info,
                fn_stop: *fn_stop,
                fn_postprocess: *fn_postprocess,
                fn_finalize: *fn_finalize,
                fn_release: *fn_release,
                _lib: lib,
            })
        }
    }

    /// Explicitly stops and finalizes capture to release the hardware scaler in the webOS kernel.
    pub fn stop_stream(&mut self) {
        if self.is_stopped || self.driver.is_null() {
            return;
        }
        let client_ptr = self.client_id.as_ptr() as *const c_char;
        for result in [
            frame_native!(
                lg_frame_guard_stop,
                "stop",
                self.fn_stop,
                self.driver,
                client_ptr
            ),
            frame_native!(
                lg_frame_guard_postprocess,
                "postprocess",
                self.fn_postprocess,
                self.driver,
                client_ptr
            ),
            frame_native!(
                lg_frame_guard_finalize,
                "finalize",
                self.fn_finalize,
                self.driver,
                client_ptr
            ),
        ] {
            if let Err(error) = result {
                warn!("LG capture cleanup: {error:#}");
            }
        }
        self.is_stopped = true;
        info!("[-] Released vtCapture hardware scaler channel.");
    }
}

/// Convert validated NV12 to RGBA with explicit BT.601 limited-range colourimetry.
/// This preserves the capture assumption; TV source mode is not capture metadata.
pub fn convert_nv12_to_rgba(
    width: u32,
    height: u32,
    stride: u32,
    y_plane: &[u8],
    uv_plane: &[u8],
    rgb_buffer: &mut [u8],
) -> Result<()> {
    let (y_len, uv_len, rgb_len) = nv12_lengths(width, height, stride)?;
    if y_plane.len() < y_len || uv_plane.len() < uv_len || rgb_buffer.len() < rgb_len {
        return Err(anyhow!("NV12 plane or output buffer is too short"));
    }
    let output_stride = width
        .checked_mul(4)
        .ok_or_else(|| anyhow!("RGBA stride overflow"))?;
    let (luma, y_stride) = prepare_nv12_plane(y_plane, width as usize, stride, height as usize, 16);
    let (chroma, uv_stride) = prepare_nv12_plane(
        uv_plane,
        width.div_ceil(2) as usize * 2,
        stride,
        height.div_ceil(2) as usize,
        0,
    );
    let image = yuv::YuvBiPlanarImage {
        y_plane: &luma,
        y_stride,
        uv_plane: &chroma,
        uv_stride,
        width,
        height,
    };
    yuv::yuv_nv12_to_rgba(
        &image,
        &mut rgb_buffer[..rgb_len],
        output_stride,
        yuv::YuvRange::Limited,
        yuv::YuvStandardMatrix::Bt601,
        yuv::YuvConversionMode::Professional,
    )
    .context("NV12 BT.601 limited-range conversion failed")
}

/// yuv iterates complete stride-sized rows. Repack minimal final rows rather than silently
/// skipping them; normalize footroom before scalar/SIMD conversion. Ordinary planes are borrowed.
fn prepare_nv12_plane(
    plane: &[u8],
    row_width: usize,
    stride: u32,
    rows: usize,
    minimum: u8,
) -> (std::borrow::Cow<'_, [u8]>, u32) {
    let full_len = (stride as usize).checked_mul(rows);
    let needs_normalizing = minimum > 0
        && plane
            .chunks(stride as usize)
            .take(rows)
            .any(|row| row[..row_width].iter().any(|value| *value < minimum));
    if let Some(full_len) = full_len.filter(|len| *len <= plane.len()) {
        if !needs_normalizing {
            return (std::borrow::Cow::Borrowed(&plane[..full_len]), stride);
        }
    }
    // Dimensions and minimal row lengths were checked by nv12_lengths before this helper.
    let mut packed = Vec::with_capacity(row_width * rows);
    for row in plane.chunks(stride as usize).take(rows) {
        packed.extend(row[..row_width].iter().map(|value| (*value).max(minimum)));
    }
    (std::borrow::Cow::Owned(packed), row_width as u32)
}

pub(super) fn nv12_lengths(width: u32, height: u32, stride: u32) -> Result<(usize, usize, usize)> {
    let (w, h, s) = (width as usize, height as usize, stride as usize);
    let chroma_width = w
        .checked_add(1)
        .ok_or_else(|| anyhow!("NV12 width overflow"))?
        / 2
        * 2;
    if w == 0 || h == 0 || s < chroma_width {
        return Err(anyhow!("invalid NV12 dimensions or stride"));
    }
    let y_len = (h - 1).checked_mul(s).and_then(|n| n.checked_add(w));
    let uv_len = (h / 2 + h % 2 - 1)
        .checked_mul(s)
        .and_then(|n| n.checked_add(chroma_width));
    let rgb_len = w.checked_mul(h).and_then(|n| n.checked_mul(4));
    match (y_len, uv_len, rgb_len) {
        (Some(y), Some(uv), Some(rgb))
            if y <= isize::MAX as usize
                && uv <= isize::MAX as usize
                && rgb <= isize::MAX as usize =>
        {
            Ok((y, uv, rgb))
        }
        _ => Err(anyhow!("NV12 frame size overflow")),
    }
}

impl ScreenCapture for VtCapture {
    fn acquire_frame(&mut self) -> std::result::Result<CapturedFrame<'_>, CaptureError> {
        self.last_raw_stats = None;
        if self.is_stopped {
            return Err(CaptureError::RestartRequired {
                backend: "libvtcapture",
                reason: "stream stopped",
            });
        }

        let mut buf_info = LibVtCaptureBufferInfo {
            start_addr0: std::ptr::null_mut(),
            start_addr1: std::ptr::null_mut(),
            size0: 0,
            size1: 0,
        };

        let ret = frame_native!(
            lg_frame_guard_current_buffer,
            "current-buffer",
            self.fn_current_buff_info,
            self.driver,
            &mut buf_info
        )?;
        if ret != 0 || buf_info.start_addr0.is_null() {
            return Err(CaptureError::Unavailable(NativeCaptureError {
                backend: "libvtcapture",
                operation: if buf_info.start_addr0.is_null() {
                    "vtCapture_current_capture_buff_info (null buffer)"
                } else {
                    "vtCapture_current_capture_buff_info"
                },
                code: ret,
            }));
        }

        let curr_addr = buf_info.start_addr0 as usize;
        if curr_addr == self.last_addr0 {
            self.stale_count += 1;
            // Unchanged address for > 1.5s (90 frames at 60 FPS) indicates frozen video pipe / timing change
            if self.stale_count >= 90 {
                warn!("Hardware capture buffer address unchanged for 1.5s: resetting pipeline to prevent display mute...");
                self.stop_stream();
                return Err(CaptureError::RestartRequired {
                    backend: "libvtcapture",
                    reason: "buffer address unchanged for 90 acquisitions",
                });
            }
        } else {
            self.last_addr0 = curr_addr;
            self.stale_count = 0;
        }

        let (y_len, uv_len, _) = nv12_lengths(self.width, self.height, self.stride)?;
        if buf_info.size0 < 0
            || (buf_info.size0 as usize) < y_len
            || (!buf_info.start_addr1.is_null()
                && (buf_info.size1 < 0 || (buf_info.size1 as usize) < uv_len))
        {
            self.stop_stream();
            return Err(anyhow!("capture driver returned undersized frame planes").into());
        }
        // SAFETY: the driver reports non-null plane pointers and sizes covering these slices.
        // The capture handle remains active for the duration of this conversion.
        let y_plane = unsafe { std::slice::from_raw_parts(buf_info.start_addr0, y_len) };
        if !buf_info.start_addr1.is_null() {
            let uv_plane = unsafe { std::slice::from_raw_parts(buf_info.start_addr1, uv_len) };
            if self.raw_stats_enabled {
                self.last_raw_stats = Some(central_nv12_stats(
                    self.width,
                    self.height,
                    self.stride,
                    y_plane,
                    uv_plane,
                )?);
            }
            convert_nv12_to_rgba(
                self.width,
                self.height,
                self.stride,
                y_plane,
                uv_plane,
                &mut self.rgb_buffer,
            )?;
        } else {
            if self.raw_stats_enabled {
                return Err(anyhow!("capture frame has no NV12 chroma plane").into());
            }
            let width = self.width as usize;
            let stride = self.stride as usize;
            for (row, pixels) in self.rgb_buffer.chunks_exact_mut(width * 4).enumerate() {
                for (x, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let y_val = y_plane[row * stride + x];
                    pixel.copy_from_slice(&[y_val, y_val, y_val, 255]);
                }
            }
        }

        Ok(CapturedFrame {
            data: &self.rgb_buffer,
            width: self.width,
            height: self.height,
            is_bgra: false,
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn is_real_hardware(&self) -> bool {
        true
    }
}

impl Drop for VtCapture {
    fn drop(&mut self) {
        if !self.driver.is_null() {
            let client_ptr = self.client_id.as_ptr() as *const c_char;
            if !self.is_stopped {
                let _ = frame_native!(
                    lg_frame_guard_stop,
                    "stop",
                    self.fn_stop,
                    self.driver,
                    client_ptr
                );
                let _ = frame_native!(
                    lg_frame_guard_postprocess,
                    "postprocess",
                    self.fn_postprocess,
                    self.driver,
                    client_ptr
                );
                let _ = frame_native!(
                    lg_frame_guard_finalize,
                    "finalize",
                    self.fn_finalize,
                    self.driver,
                    client_ptr
                );
            }
            let _ = frame_release(self.fn_release, self.driver);
            self.driver = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_maps_cover_only_contiguous_readable_intervals() {
        let maps = "00001000-00002000 rw-p 00000000 00:00 0\n00002000-00003000 r--p 00000000 00:00 0\n00003000-00004000 -w-p 00000000 00:00 0\n00005000-00006000 r--p 00000000 00:00 0\n";
        let spans = parse_readable_spans(maps).unwrap();
        assert!(span_is_readable(0x1800, 0x1000, &spans));
        assert!(!span_is_readable(0x2800, 0x1000, &spans));
        assert!(!span_is_readable(0x3800, 0x100, &spans));
        assert!(!span_is_readable(0x2800, 0x2800, &spans));
        assert!(span_is_readable(0x5000, 0x1000, &spans));
        assert!(!span_is_readable(0x5000, 0x1001, &spans));
        assert!(!span_is_readable(usize::MAX - 2, 10, &spans));
        assert!(parse_readable_spans("not-a-mapping rw-p").is_err());
    }

    #[test]
    fn mapping_probe_checks_full_ranges_and_rejects_overlap_without_reading() {
        let layout = DescriptorSummary {
            y_present: true,
            uv_present: true,
            y_size: 0x1000,
            uv_size: 0x800,
        };
        let maps =
            "00001000-00003000 rw-p 00000000 00:00 0\n00004000-00005000 r--p 00000000 00:00 0\n";
        // Deliberately fake pointer values: no actual allocation exists.
        let mut descriptor = LibVtCaptureBufferInfo {
            start_addr0: 0x1800usize as *mut u8,
            start_addr1: 0x4000usize as *mut u8,
            size0: 0x1000,
            size1: 0x800,
        };
        check_descriptor_readable_mappings(&descriptor, layout, maps).unwrap();
        descriptor.start_addr1 = 0x2800usize as *mut u8;
        assert!(check_descriptor_readable_mappings(&descriptor, layout, maps).is_err());
        descriptor.start_addr1 = 0x5000usize as *mut u8;
        assert!(check_descriptor_readable_mappings(&descriptor, layout, maps).is_err());
    }

    #[test]
    fn buffer_descriptor_is_checked_as_opaque_metadata_only() {
        // Deliberately invalid non-null pointers. The validator must not
        // dereference, copy or create slices from either address.
        let descriptor = LibVtCaptureBufferInfo {
            start_addr0: std::ptr::dangling_mut::<u8>(),
            start_addr1: std::ptr::dangling_mut::<u8>(),
            size0: 320 * 180,
            size1: 320 * 90,
        };
        let valid = validate_buffer_descriptor(&descriptor, (320, 180, 320)).unwrap();
        assert!(valid.y_present && valid.uv_present);
        assert_eq!(valid.y_size, 57600);
        assert_eq!(valid.uv_size, 28800);

        let missing_uv = LibVtCaptureBufferInfo {
            start_addr1: std::ptr::null_mut(),
            ..descriptor
        };
        assert!(validate_buffer_descriptor(&missing_uv, (320, 180, 320)).is_err());

        let undersized = LibVtCaptureBufferInfo {
            size0: 57599,
            ..descriptor
        };
        assert!(validate_buffer_descriptor(&undersized, (320, 180, 320)).is_err());

        let negative = LibVtCaptureBufferInfo {
            size1: -1,
            ..descriptor
        };
        assert!(validate_buffer_descriptor(&negative, (320, 180, 320)).is_err());

        let huge = LibVtCaptureBufferInfo {
            size0: 17 * 1024 * 1024,
            ..descriptor
        };
        assert!(validate_buffer_descriptor(&huge, (320, 180, 320)).is_err());
    }

    #[test]
    fn process_probe_cleanup_stops_before_postprocess_finalize_release() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn stop(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 0);
            0
        }
        unsafe extern "C" fn post(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 1);
            0
        }
        unsafe extern "C" fn finalise(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 2);
            0
        }
        unsafe extern "C" fn release(_: *mut c_void) {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 3);
        }

        CALLS.store(0, Ordering::SeqCst);
        let mut session = C5MetadataSession {
            driver: std::ptr::dangling_mut::<c_void>(),
            client_id: [0; 128],
            initialized: true,
            preprocess_attempted: true,
            process_attempted: true,
            released: false,
            stop,
            postprocess: post,
            finalize: finalise,
            release,
        };
        session.cleanup().unwrap();
        session.cleanup().unwrap();
        drop(session);
        assert_eq!(CALLS.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn process_probe_attempts_full_cleanup_on_stop_refusal() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn stop(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 0);
            17
        }
        unsafe extern "C" fn post(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 1);
            0
        }
        unsafe extern "C" fn finalise(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 2);
            0
        }
        unsafe extern "C" fn release(_: *mut c_void) {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 3);
        }

        CALLS.store(0, Ordering::SeqCst);
        let mut session = C5MetadataSession {
            driver: std::ptr::dangling_mut::<c_void>(),
            client_id: [0; 128],
            initialized: true,
            preprocess_attempted: true,
            process_attempted: true,
            released: false,
            stop,
            postprocess: post,
            finalize: finalise,
            release,
        };
        let err = session.cleanup().unwrap_err().to_string();
        assert!(err.contains("stop returned 17"));
        assert_eq!(CALLS.load(Ordering::SeqCst), 4);
        assert!(session.released);
    }

    #[test]
    fn metadata_probe_cleanup_runs_once_in_correct_order_without_a_device() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn fake_stop(_: *mut c_void, _: *const c_char) -> c_int {
            panic!("metadata-only probe must not call stop")
        }
        unsafe extern "C" fn fake_post(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 0);
            0
        }
        unsafe extern "C" fn fake_final(_: *mut c_void, _: *const c_char) -> c_int {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 1);
            0
        }
        unsafe extern "C" fn fake_release(_: *mut c_void) {
            assert_eq!(CALLS.fetch_add(1, Ordering::SeqCst), 2);
        }

        CALLS.store(0, Ordering::SeqCst);
        let mut session = C5MetadataSession {
            driver: std::ptr::dangling_mut::<c_void>(),
            client_id: [0; 128],
            initialized: true,
            preprocess_attempted: true,
            process_attempted: false,
            released: false,
            stop: fake_stop,
            postprocess: fake_post,
            finalize: fake_final,
            release: fake_release,
        };
        session.cleanup().unwrap();
        session.cleanup().unwrap();
        drop(session);
        assert_eq!(CALLS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn metadata_probe_release_after_init_refusal_skips_postprocess_and_finalize() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static RELEASED: AtomicBool = AtomicBool::new(false);
        unsafe extern "C" fn unexpected(_: *mut c_void, _: *const c_char) -> c_int {
            panic!("not initialized, should not clean up as initialized");
        }
        unsafe extern "C" fn fake_release(_: *mut c_void) {
            RELEASED.store(true, Ordering::SeqCst);
        }
        RELEASED.store(false, Ordering::SeqCst);
        let mut session = C5MetadataSession {
            driver: std::ptr::dangling_mut::<c_void>(),
            client_id: [0; 128],
            initialized: false,
            preprocess_attempted: false,
            process_attempted: false,
            released: false,
            stop: unexpected,
            postprocess: unexpected,
            finalize: unexpected,
            release: fake_release,
        };
        session.cleanup().unwrap();
        assert!(RELEASED.load(Ordering::SeqCst));
    }

    #[test]
    fn test_vtcapture_struct_sizes() {
        assert_eq!(std::mem::size_of::<LibVtCaptureProperties>(), 20);
        assert_eq!(std::mem::size_of::<LibVtCapturePlaneInfo>(), 20);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(std::mem::size_of::<LibVtCaptureBufferInfo>(), 16);
    }

    #[test]
    fn c5_plane_validation_rejects_oversized_unaligned_and_missing_geometry() {
        let mut plane = LibVtCapturePlaneInfo {
            stride: 320,
            plane_w: 320,
            plane_h: 180,
            ..Default::default()
        };
        assert_eq!(checked_plane_dimensions(&plane).unwrap(), (320, 180, 320));
        plane.stride = 200;
        assert!(checked_plane_dimensions(&plane).is_err());
        plane.stride = 8200;
        assert!(checked_plane_dimensions(&plane).is_err());
        plane.stride = 320;
        plane.plane_h = 0;
        assert!(checked_plane_dimensions(&plane).is_err());
        plane.plane_h = 181;
        assert!(checked_plane_dimensions(&plane).is_err());
        plane.plane_h = 180;
        plane.plane_w = 319;
        assert!(checked_plane_dimensions(&plane).is_err());
        plane.plane_w = 320;
        assert!(checked_plane_dimensions(&plane).is_ok());
    }

    #[test]
    fn test_vtcapture_fails_gracefully_when_library_missing() {
        let res = VtCapture::try_new(160, 90);
        assert!(res.is_err());
    }

    #[test]
    fn central_nv12_probe_uses_only_the_centre_and_checks_plane_lengths() {
        let (width, height, stride) = (20, 20, 22);
        let mut y = vec![10u8; stride * height];
        let mut uv = vec![128u8; stride * (height / 2)];
        for row in 2..18 {
            for col in 2..18 {
                y[row * stride + col] = 100;
            }
        }
        for row in 1..9 {
            for col in (2..18).step_by(2) {
                uv[row * stride + col] = 64;
                uv[row * stride + col + 1] = 192;
            }
        }
        let stats =
            central_nv12_stats(width as u32, height as u32, stride as u32, &y, &uv).unwrap();
        assert_eq!(stats.y_min, 100);
        assert_eq!(stats.y_mean, 100.0);
        assert_eq!(stats.y_max, 100);
        assert_eq!(stats.u_mean, 64.0);
        assert_eq!(stats.v_mean, 192.0);
        assert_eq!(stats.luma_samples, 256);
        assert!(
            central_nv12_stats(width as u32, height as u32, stride as u32, &y, &uv[..20]).is_err()
        );
    }

    #[test]
    fn colored_nv12_fixture_locks_current_bt601_conversion() {
        let mut rgba = [0u8; 16];
        convert_nv12_to_rgba(2, 2, 2, &[100; 4], &[64, 192], &mut rgba).unwrap();
        for pixel in rgba.chunks_exact(4) {
            assert_eq!(pixel, &[200, 71, 0, 255]);
        }
    }

    #[test]
    fn test_nv12_conversion_black_and_white_reference() {
        // 2x2 test image
        let width = 2u32;
        let height = 2u32;
        let stride = 2u32;

        // Test 1: Standard video black (Y=16, U=128, V=128)
        let y_black = [16u8; 4];
        let uv_neutral = [128u8; 2]; // 1 pair for 2x2 NV12
        let mut rgb_out = [0u8; 16];

        convert_nv12_to_rgba(width, height, stride, &y_black, &uv_neutral, &mut rgb_out).unwrap();

        // Every pixel should decode to black (0, 0, 0, 255)
        for pixel in rgb_out.chunks(4) {
            assert_eq!(pixel, &[0, 0, 0, 255]);
        }

        // Test 2: Standard video white (Y=235, U=128, V=128)
        let y_white = [235u8; 4];
        convert_nv12_to_rgba(width, height, stride, &y_white, &uv_neutral, &mut rgb_out).unwrap();

        // Every pixel should decode to full white (255, 255, 255, 255)
        for pixel in rgb_out.chunks(4) {
            assert_eq!(pixel, &[255, 255, 255, 255]);
        }
    }

    #[test]
    fn test_nv12_conversion_hardware_stride_handling() {
        // Test that a plane with stride larger than width correctly skips padding bytes
        // 2x2 image embedded inside stride=4 buffer
        let width = 2u32;
        let height = 2u32;
        let stride = 4u32;

        // Row 0: [235, 235, padding, padding]
        // Row 1: [16, 16, padding, padding]
        let y_data = [235u8, 235u8, 0, 0, 16u8, 16u8, 0, 0];
        // UV row 0 (applies to both Y rows): [128, 128, padding, padding]
        let uv_data = [128u8, 128u8, 0, 0];
        let mut rgb_out = [0u8; 16];

        convert_nv12_to_rgba(width, height, stride, &y_data, &uv_data, &mut rgb_out).unwrap();

        // Row 0 pixels must be white
        assert_eq!(&rgb_out[0..4], &[255, 255, 255, 255]);
        assert_eq!(&rgb_out[4..8], &[255, 255, 255, 255]);

        // Row 1 pixels must be black (verifying stride=4 advanced correctly to index 4)
        assert_eq!(&rgb_out[8..12], &[0, 0, 0, 255]);
        assert_eq!(&rgb_out[12..16], &[0, 0, 0, 255]);
    }

    #[test]
    fn test_nv12_conversion_odd_width_boundary() {
        // 3x2 image to test boundary condition where width is not a multiple of 2
        let width = 3u32;
        let height = 2u32;
        let stride = 4u32;

        let y_data = [235u8, 235u8, 235u8, 0, 16u8, 16u8, 16u8, 0];
        let uv_data = [128u8, 128u8, 128u8, 128u8];
        let mut rgb_out = [0u8; 24]; // 3 * 2 * 4

        convert_nv12_to_rgba(width, height, stride, &y_data, &uv_data, &mut rgb_out).unwrap();

        // Row 0: 3 white pixels
        assert_eq!(&rgb_out[0..4], &[255, 255, 255, 255]);
        assert_eq!(&rgb_out[4..8], &[255, 255, 255, 255]);
        assert_eq!(&rgb_out[8..12], &[255, 255, 255, 255]);

        // Row 1: 3 black pixels
        assert_eq!(&rgb_out[12..16], &[0, 0, 0, 255]);
        assert_eq!(&rgb_out[16..20], &[0, 0, 0, 255]);
        assert_eq!(&rgb_out[20..24], &[0, 0, 0, 255]);
    }

    #[test]
    fn nv12_rejects_short_planes_and_invalid_stride() {
        let mut output = [0u8; 16];
        assert!(convert_nv12_to_rgba(2, 2, 2, &[16; 3], &[128; 2], &mut output).is_err());
        assert!(convert_nv12_to_rgba(2, 2, 2, &[16; 4], &[128; 1], &mut output).is_err());
        assert!(convert_nv12_to_rgba(3, 2, 3, &[16; 6], &[128; 4], &mut [0; 24]).is_err());
        assert!(convert_nv12_to_rgba(2, 2, 2, &[16; 4], &[128; 2], &mut [0; 15]).is_err());
    }

    #[test]
    fn yuv_conversion_matches_bt601_reference_and_bounds_quantization_changes() {
        let mut max_legacy_difference = 0;
        for y in [0u8, 15, 16, 17, 64, 100, 128, 200, 234, 235, 255] {
            for u in [0u8, 16, 64, 128, 192, 240, 255] {
                for v in [0u8, 16, 64, 128, 192, 240, 255] {
                    let mut rgba = [0; 16];
                    convert_nv12_to_rgba(2, 2, 2, &[y; 4], &[u, v], &mut rgba).unwrap();
                    let luma = (f64::from(y.max(16)) - 16.0) * 255.0 / 219.0;
                    let cb = (f64::from(u) - 128.0) * 255.0 / 224.0;
                    let cr = (f64::from(v) - 128.0) * 255.0 / 224.0;
                    let reference = [
                        luma + 1.402 * cr,
                        luma - (0.114 * 1.772 * cb + 0.299 * 1.402 * cr) / 0.587,
                        luma + 1.772 * cb,
                    ];
                    for (actual, expected) in rgba[..3].iter().zip(reference) {
                        assert!(
                            actual.abs_diff(expected.round().clamp(0.0, 255.0) as u8) <= 1,
                            "YUV={y},{u},{v}: {rgba:?}, expected={reference:?}"
                        );
                    }
                    assert!(rgba.chunks_exact(4).all(|p| p == &rgba[..4] && p[3] == 255));
                    {
                        let c = 298 * (i32::from(y) - 16).max(0);
                        let d = i32::from(u) - 128;
                        let e = i32::from(v) - 128;
                        let legacy = [
                            (c + 409 * e + 128) >> 8,
                            (c - 100 * d - 208 * e + 128) >> 8,
                            (c + 516 * d + 128) >> 8,
                        ];
                        for (actual, old) in rgba[..3].iter().zip(legacy) {
                            max_legacy_difference =
                                max_legacy_difference.max(actual.abs_diff(old.clamp(0, 255) as u8));
                        }
                    }
                }
            }
        }
        assert!(
            max_legacy_difference <= 1,
            "legacy difference={max_legacy_difference}"
        );
        // Minimal last rows are valid; padding and extra destination bytes stay untouched.
        for size in [40, 64] {
            let mut rgba = vec![17; size];
            convert_nv12_to_rgba(3, 3, 4, &[235; 11], &[128; 8], &mut rgba).unwrap();
            assert!(rgba[..36]
                .chunks_exact(4)
                .all(|p| p == [255, 255, 255, 255]));
            assert!(rgba[36..].iter().all(|v| *v == 17));
        }
        let mut rgba = [17; 4];
        convert_nv12_to_rgba(1, 1, u32::MAX, &[0], &[128; 2], &mut rgba).unwrap();
        assert_eq!(rgba, [0, 0, 0, 255]);
    }
}
