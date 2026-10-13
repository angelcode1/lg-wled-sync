//! Optional LG hardware histogram source.
//! ABI and record layout reconstructed from user-supplied HyperDRM 0.13.0
//! native/service.c and native/histogram.h. NOT verified on this TV.
//! This does not access decrypted pixels; each sample has 16 region histograms.
use crate::color::RgbColor;
use crate::output::perimeter::Point;
use anyhow::{bail, Context, Result};
use libloading::Library;
use serde::{Deserialize, Serialize};
use std::ffi::{c_void, CString};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::sync::atomic::{AtomicBool, Ordering};

// Exporting this C symbol is necessary for LG's native diagnostic calls.
// See native/lg_printlog.c. The ABI is verified at link time on Linux.
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn lg_printlog_anchor() -> libc::c_int;
}

const DEVICE: &str = "/usr/lib/libvtcapture.so.1";
const MARKER: &str = "/var/luna/preferences/vt_devmode";

// Exact C5 library supplied from the TV and disassembled offline (BuildID
// 9fb5e56d59c29ed242c0e4a66015c71601a4657e). Its VtHistogram::initHistogram
// returns literal 7 without initializing anything; its getHistogram returns
// 1/2 without writing the caller's output buffer. The API is stubbed in this
// binary; permissions cannot make it yield usable histogram samples.
// Fail before creating the temporary developer marker or calling vendor code.
const C5_STUBBED_VT_HISTOGRAM_SHA256: [u8; 32] = [
    0x64, 0x83, 0xfc, 0x0b, 0x82, 0xa9, 0xda, 0x6f, 0x65, 0xe1, 0x1e, 0x85, 0xf9, 0xdc, 0x43, 0xbb,
    0x16, 0x3d, 0xe8, 0x0f, 0xe2, 0xc3, 0xc8, 0x4b, 0x84, 0xaf, 0xa2, 0x51, 0xb0, 0xf6, 0x89, 0x2c,
];

fn is_known_stubbed_histogram_library(bytes: &[u8]) -> bool {
    openssl::sha::sha256(bytes) == C5_STUBBED_VT_HISTOGRAM_SHA256
}

// Safe fingerprint discriminator shared with the frame fallback policy.
// No native code is invoked, and no vendor permission is modified.
pub(crate) fn is_known_c5_vendor_elf() -> bool {
    fs::read(DEVICE)
        .map(|bytes| is_known_stubbed_histogram_library(&bytes))
        .unwrap_or(false)
}

static REVOKED: AtomicBool = AtomicBool::new(false);
extern "C" fn policy_revoked() -> bool {
    REVOKED.store(true, Ordering::SeqCst);
    true
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeHistogram {
    id: u32,
    bins: [[u16; 256]; 3],
}
impl Default for NativeHistogram {
    fn default() -> Self {
        Self {
            id: 0,
            bins: [[0; 256]; 3],
        }
    }
}
type Init = unsafe extern "C" fn(extern "C" fn() -> bool) -> *mut c_void;
type Preset = unsafe extern "C" fn(*mut c_void, i32) -> i32;
type Get = unsafe extern "C" fn(*mut c_void, *mut NativeHistogram) -> i32;
type Finish = unsafe extern "C" fn(*mut c_void) -> i32;

// LG's C++ libvtcapture can throw std::runtime_error when LS2 registration
// fails. Direct calls across the Rust FFI boundary terminate the process.
// Keep native exceptions inside a narrow C++ wrapper compiled for Linux.
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn lg_histogram_init_guarded(
        init: Init,
        callback: extern "C" fn() -> bool,
        error: *mut libc::c_char,
        cap: usize,
    ) -> *mut c_void;
    fn lg_histogram_preset_guarded(
        preset: Preset,
        handle: *mut c_void,
        value: i32,
        error: *mut libc::c_char,
        cap: usize,
    ) -> i32;
    fn lg_histogram_get_guarded(
        get: Get,
        handle: *mut c_void,
        records: *mut NativeHistogram,
        error: *mut libc::c_char,
        cap: usize,
    ) -> i32;
    fn lg_histogram_finish_guarded(
        finish: Finish,
        handle: *mut c_void,
        error: *mut libc::c_char,
        cap: usize,
    ) -> i32;
}

#[cfg(target_os = "linux")]
const LG_ERROR_LEN: usize = 192;

#[cfg(target_os = "linux")]
fn native_exception(error: &[libc::c_char; LG_ERROR_LEN]) -> Option<String> {
    let end = error.iter().position(|&b| b == 0).unwrap_or(error.len());
    if end == 0 {
        return None;
    }
    let bytes: Vec<u8> = error[..end].iter().map(|&b| b as u8).collect();
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

unsafe fn guarded_init(init: Init) -> Result<*mut c_void> {
    #[cfg(target_os = "linux")]
    {
        let mut error = [0; LG_ERROR_LEN];
        let handle = unsafe {
            lg_histogram_init_guarded(init, policy_revoked, error.as_mut_ptr(), error.len())
        };
        if let Some(message) = native_exception(&error) {
            bail!("LG native init exception: {message}");
        }
        Ok(handle)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(unsafe { init(policy_revoked) })
    }
}

unsafe fn guarded_preset(preset: Preset, handle: *mut c_void, value: i32) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        let mut error = [0; LG_ERROR_LEN];
        let status = unsafe {
            lg_histogram_preset_guarded(preset, handle, value, error.as_mut_ptr(), error.len())
        };
        if let Some(message) = native_exception(&error) {
            bail!("LG native preset exception: {message}");
        }
        Ok(status)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(unsafe { preset(handle, value) })
    }
}

unsafe fn guarded_get(get: Get, handle: *mut c_void, records: *mut NativeHistogram) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        let mut error = [0; LG_ERROR_LEN];
        let status = unsafe {
            lg_histogram_get_guarded(get, handle, records, error.as_mut_ptr(), error.len())
        };
        if let Some(message) = native_exception(&error) {
            bail!("LG native sampling exception: {message}");
        }
        Ok(status)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(unsafe { get(handle, records) })
    }
}

unsafe fn guarded_finish(finish: Finish, handle: *mut c_void) -> Result<i32> {
    #[cfg(target_os = "linux")]
    {
        let mut error = [0; LG_ERROR_LEN];
        let status =
            unsafe { lg_histogram_finish_guarded(finish, handle, error.as_mut_ptr(), error.len()) };
        if let Some(message) = native_exception(&error) {
            bail!("LG native finalize exception: {message}");
        }
        Ok(status)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(unsafe { finish(handle) })
    }
}

pub struct HistogramCapture {
    handle: *mut c_void,
    get: Get,
    finish: Finish,
    _library: Library,
}

// LG initialization needs its developer marker. A guardian child uses only
// async-signal-safe libc operations after fork, and removes only the marker we
// created, matched by device/inode. If the marker already exists, fail closed:
// it might belong to another process OR be stale after a native abort.
// Do not create this guardian while another thread is concurrently forking.
struct MarkerGuard {
    writer: i32,
    pid: libc::pid_t,
}
impl MarkerGuard {
    fn begin() -> Result<Self> {
        use std::io::ErrorKind;
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(MARKER);
        let file = match result {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                bail!(
                    "LG developer marker already exists at {MARKER}; refusing native capture.                      Check for active VT owners and inspect the marker before manual cleanup"
                );
            }
            Err(error) => return Err(error).context("cannot create temporary LG developer marker"),
        };
        let metadata = file.metadata()?;
        let dev = metadata.dev();
        let ino = metadata.ino();
        drop(file);
        let path = CString::new(MARKER)?;
        let mut fds = [-1i32; 2];
        // SAFETY: all used paths and descriptors are created locally.
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if rc != 0 {
            let _ = fs::remove_file(MARKER);
            bail!("LG marker guardian pipe failed");
        }
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            // Child: never call Rust allocators or logging in a multithreaded
            // process after fork. Detach from the SSH process group so closing
            // a PTY after a native crash does not necessarily kill cleanup.
            // This is best-effort; cgroup kills and kernel failures still
            // require manual marker inspection.
            unsafe {
                if libc::setsid() < 0 {
                    libc::_exit(1);
                }
                libc::close(fds[1]);
                let mut waiter = libc::pollfd {
                    fd: fds[0],
                    events: libc::POLLIN | libc::POLLHUP,
                    revents: 0,
                };
                // Wait until the parent closes the write end or dies.
                // A fixed timeout could delete vt_devmode during a slow
                // native initialization while the owner was still alive.
                loop {
                    let result = libc::poll(&mut waiter, 1, -1);
                    if result >= 0 {
                        break;
                    }
                    if *libc::__errno_location() != libc::EINTR {
                        libc::close(fds[0]);
                        libc::_exit(1); // leave marker in place on unknown errors
                    }
                }
                libc::close(fds[0]);
                let mut found = std::mem::zeroed::<libc::stat>();
                let mut ok = 1;
                if libc::lstat(path.as_ptr(), &mut found) == 0
                    && found.st_dev as u64 == dev
                    && found.st_ino as u64 == ino
                {
                    ok = if libc::unlink(path.as_ptr()) == 0 {
                        0
                    } else {
                        1
                    };
                } else if libc::lstat(path.as_ptr(), &mut found) != 0 {
                    ok = 0; // already removed
                }
                libc::_exit(ok);
            }
        }
        unsafe {
            libc::close(fds[0]);
        }
        if pid < 0 {
            unsafe {
                libc::close(fds[1]);
            }
            let _ = fs::remove_file(MARKER);
            bail!("LG marker guardian fork failed");
        }
        Ok(Self {
            writer: fds[1],
            pid,
        })
    }
    fn finish(&mut self) -> Result<()> {
        if self.writer >= 0 {
            unsafe {
                libc::close(self.writer);
            }
            self.writer = -1;
        }
        if self.pid <= 0 {
            return Ok(());
        }
        let mut status = 0;
        let pid = self.pid;
        loop {
            let result = unsafe { libc::waitpid(pid, &mut status, 0) };
            if result == pid {
                break;
            }
            if result == -1
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            }
            bail!("LG marker guardian wait failed");
        }
        self.pid = 0;
        if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
            bail!("LG marker cleanup failed; manual inspection required");
        }
        Ok(())
    }
}
impl Drop for MarkerGuard {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

impl HistogramCapture {
    pub fn try_new() -> Result<Self> {
        #[cfg(target_os = "linux")]
        if unsafe { lg_printlog_anchor() } != 1 {
            bail!("LG native diagnostic ABI shim was not linked");
        }
        if std::mem::size_of::<NativeHistogram>() != 0x604 {
            bail!("LG histogram ABI record size mismatch");
        }
        // Best-effort exact-binary detection. If reading this file fails,
        // retain the existing dlopen error path for a useful diagnosis.
        if let Ok(bytes) = fs::read(DEVICE) {
            if is_known_stubbed_histogram_library(&bytes) {
                bail!(
                    "LG C5 native histogram unavailable: this exact libvtcapture build has a stubbed VtHistogram API (initHistogram always returns 7; getHistogram writes no bins). No VT initialization attempted"
                );
            }
        }
        // SAFETY: layout and four native signatures mirror the uploaded
        // HyperDRM 0.13.0 source. Runtime ABI still needs TV verification.
        unsafe {
            let library = Library::new(DEVICE).context("LG libvtcapture unavailable")?;
            let init: Init = *library.get(b"vtCapture_initHistogram\0")?;
            let preset: Preset = *library.get(b"vtCapture_setPresetHistogram\0")?;
            let get: Get = *library.get(b"vtCapture_getHistogram\0")?;
            let finish: Finish = *library.get(b"vtCapture_finalizeHistogram\0")?;
            REVOKED.store(false, Ordering::SeqCst);
            let mut marker = MarkerGuard::begin()?;
            // Always clean up the temporary developer marker, even when a
            // native C++ exception is returned as a Rust error.
            let init_result = guarded_init(init);
            if let Err(error) = marker.finish() {
                if let Ok(handle) = init_result.as_ref() {
                    if !handle.is_null() {
                        let _ = guarded_finish(finish, *handle);
                    }
                }
                return Err(error);
            }
            let handle = init_result?;
            if handle.is_null() {
                bail!("LG native histogram initialization returned null (underlying reason unavailable; check vendor logs and ELF support)");
            }
            match guarded_preset(preset, handle, 0) {
                Ok(0) => {}
                Ok(rc) => {
                    let _ = guarded_finish(finish, handle);
                    bail!("LG histogram preset 0 failed ({rc})");
                }
                Err(error) => {
                    let _ = guarded_finish(finish, handle);
                    return Err(error);
                }
            }
            Ok(Self {
                handle,
                get,
                finish,
                _library: library,
            })
        }
    }
    pub fn sample(&mut self) -> Result<[RgbColor; 16]> {
        if REVOKED.swap(false, Ordering::SeqCst) {
            bail!("LG revoked histogram capture");
        }
        let mut records = [NativeHistogram::default(); 16];
        let rc = unsafe { guarded_get(self.get, self.handle, records.as_mut_ptr()) }?;
        if rc != 0 {
            bail!("LG histogram read failed ({rc})");
        }
        let mut colors = [RgbColor::new(0, 0, 0); 16];
        for (i, record) in records.iter().enumerate() {
            let mut rgb = [0u8; 3];
            for (channel, bins) in record.bins.iter().enumerate() {
                let mut count = 0u64;
                let mut weighted = 0u64;
                for (value, &population) in bins.iter().enumerate() {
                    count += u64::from(population);
                    weighted += value as u64 * u64::from(population);
                }
                if count == 0 {
                    bail!("empty LG histogram channel (region {i}, channel {channel})");
                }
                rgb[channel] = ((weighted + count / 2) / count) as u8;
            }
            colors[i] = RgbColor::new(rgb[0], rgb[1], rgb[2]);
        }
        Ok(colors)
    }
}
impl Drop for HistogramCapture {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            if let Err(error) = unsafe { guarded_finish(self.finish, self.handle) } {
                tracing::warn!("LG native histogram cleanup failed: {error:#}");
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    #[default]
    Average,
    G4,
    Custom,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RimPoint {
    pub position: f32,
    pub region: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HistogramSettings {
    pub profile: Profile,
    /// The exact modelName measured by the calibration wizard.
    /// Custom mappings without a model identity are rejected.
    pub model: Option<String>,
    pub points: Vec<RimPoint>,
}
impl Default for HistogramSettings {
    fn default() -> Self {
        Self {
            profile: Profile::Average,
            model: None,
            points: Vec::new(),
        }
    }
}
fn valid_model_name(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 63
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
}

impl HistogramSettings {
    /// Source-specific spatial maps are not interchangeable across TVs.
    /// A missing model from Luna is not evidence of a match.
    pub fn matches_tv_model(&self, current: Option<&str>) -> bool {
        match self.profile {
            Profile::Average => true,
            Profile::Custom => self
                .model
                .as_deref()
                .zip(current)
                .is_some_and(|(expected, actual)| expected == actual),
            Profile::G4 => current.is_some_and(|m| {
                let bytes = m.as_bytes();
                bytes.len() >= 8
                    && bytes.starts_with(b"OLED")
                    && bytes[4].is_ascii_digit()
                    && bytes[5].is_ascii_digit()
                    && bytes[6] == b'G'
                    && bytes[7] == b'4'
            }),
        }
    }

    pub fn validate(&self) -> bool {
        if self.profile == Profile::Custom {
            if !(4..=16).contains(&self.points.len()) {
                return false;
            }
            let Some(model) = self.model.as_deref() else {
                return false;
            };
            if !valid_model_name(model) {
                return false;
            }
            // Reject partially mapped rims; a complete wizard measurement
            // provides at least one calibrated point per screen edge.
            let edges: std::collections::BTreeSet<u8> = self
                .points
                .iter()
                .map(|p| p.position.floor() as u8)
                .collect();
            if edges != [0, 1, 2, 3].into_iter().collect() {
                return false;
            }
        }
        self.points.len() <= 16
            && self.points.iter().enumerate().all(|(i, p)| {
                p.position.is_finite()
                    && (0.0..4.0).contains(&p.position)
                    && p.region < 16
                    && (i == 0 || p.position > self.points[i - 1].position)
            })
    }
    pub fn map_leds(&self, rgb: &[RgbColor; 16], pixels: &[Point]) -> Result<Vec<RgbColor>> {
        if !self.validate() {
            bail!("invalid histogram calibration profile");
        }
        if self.profile == Profile::Average {
            let mut sums = [0u32; 3];
            for color in rgb {
                sums[0] += u32::from(color.r);
                sums[1] += u32::from(color.g);
                sums[2] += u32::from(color.b);
            }
            let average = RgbColor::new(
                (sums[0] / 16) as u8,
                (sums[1] / 16) as u8,
                (sums[2] / 16) as u8,
            );
            return Ok(vec![average; pixels.len()]);
        }
        const G4: [(f32, u8); 10] = [
            (0.20784314, 7),
            (0.31764707, 3),
            (0.54901963, 11),
            (0.7529412, 12),
            (0.8392157, 13),
            (1.4941176, 10),
            (2.1843138, 9),
            (2.5490196, 1),
            (2.8666668, 5),
            (3.4, 15),
        ];
        let points: Vec<(f32, u8)> = if self.profile == Profile::G4 {
            G4.to_vec()
        } else {
            self.points.iter().map(|p| (p.position, p.region)).collect()
        };
        if points.len() < 2 {
            bail!("histogram spatial calibration is incomplete");
        }
        Ok(pixels
            .iter()
            .map(|p| {
                // Edge parameter t follows HyperDRM's clockwise normalized
                // perimeter: top x, right 1+y, bottom 3-x, left 4-y.
                let t = if p.y <= 0.00001 {
                    p.x
                } else if p.x >= 0.99999 {
                    1.0 + p.y
                } else if p.y >= 0.99999 {
                    3.0 - p.x
                } else {
                    4.0 - p.y
                };
                let right = points.iter().position(|&(pos, _)| pos > t).unwrap_or(0);
                let left = (right + points.len() - 1) % points.len();
                let mut span = points[right].0 - points[left].0;
                if span <= 0.0 {
                    span += 4.0;
                }
                let mut distance = t - points[left].0;
                if distance < 0.0 {
                    distance += 4.0;
                }
                let fraction = (distance / span).clamp(0.0, 1.0);
                let a = rgb[points[left].1 as usize];
                let b = rgb[points[right].1 as usize];
                let channel = |low: u8, high: u8| -> u8 {
                    (low as f32 * (1.0 - fraction) + high as f32 * fraction).round() as u8
                };
                RgbColor::new(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn c5_vendor_stub_fingerprint_does_not_match_normal_data() {
        assert!(!is_known_stubbed_histogram_library(b""));
        assert!(!is_known_stubbed_histogram_library(b"not an LG ELF"));
        assert_eq!(C5_STUBBED_VT_HISTOGRAM_SHA256.len(), 32);
    }

    #[test]
    fn spatial_mapping_requires_exact_tv_model_and_four_edges() {
        let points = [0.25, 1.25, 2.25, 3.25]
            .into_iter()
            .enumerate()
            .map(|(i, position)| RimPoint {
                position,
                region: i as u8,
            })
            .collect();
        let cfg = HistogramSettings {
            profile: Profile::Custom,
            model: Some("OLED65C5PSA".to_string()),
            points,
        };
        assert!(cfg.validate());
        assert!(cfg.matches_tv_model(Some("OLED65C5PSA")));
        assert!(!cfg.matches_tv_model(Some("OLED55C5PSA")));
        assert!(!cfg.matches_tv_model(None));
        let mut different = cfg.clone();
        different.model = None;
        assert!(!different.validate());
        different.model = Some("OLED65C5PSA".to_string());
        different.points.pop();
        assert!(!different.validate());
        let g4 = HistogramSettings {
            profile: Profile::G4,
            ..Default::default()
        };
        assert!(!g4.matches_tv_model(Some("OLED65C5PSA")));
        assert!(g4.matches_tv_model(Some("OLED65G4PSA")));
        assert!(HistogramSettings::default().matches_tv_model(None));
    }

    #[test]
    fn source_abi_size_and_weighted_means() {
        assert_eq!(std::mem::size_of::<NativeHistogram>(), 0x604);
        let mut r = NativeHistogram::default();
        r.bins[0][254] = 65535;
        r.bins[0][255] = 1;
        let count: u64 = r.bins[0].iter().map(|&population| u64::from(population)).sum();
        let weighted: u64 = r.bins[0]
            .iter()
            .enumerate()
            .map(|(value, &population)| value as u64 * u64::from(population))
            .sum();
        assert_eq!((weighted + count / 2) / count, 254);
    }
    #[test]
    fn default_is_safe_uniform_and_custom_is_validated() {
        let mut samples = [RgbColor::new(0, 0, 0); 16];
        samples[0] = RgbColor::new(160, 0, 0);
        let config = HistogramSettings::default();
        let output = config
            .map_leds(
                &samples,
                &[Point { x: 0.25, y: 0.0 }, Point { x: 1.0, y: 0.7 }],
            )
            .unwrap();
        assert_eq!(output, vec![RgbColor::new(10, 0, 0); 2]);
        let broken = HistogramSettings {
            profile: Profile::Custom,
            model: Some("OLED65C5PSA".to_string()),
            points: vec![],
        };
        assert!(broken.map_leds(&samples, &[]).is_err());
    }
}
