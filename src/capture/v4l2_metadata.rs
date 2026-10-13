//! C5 V4L2 metadata-only probe; never initiates a capture stream.
//! The node is root:composit on the audited TV, but this code must not
//! modify ownership, group membership, SELinux or vendor permissions.
use anyhow::{anyhow, bail, Context, Result};

const VIDEO60: &str = "/dev/video60";
const SYSFS: &str = "/sys/class/video4linux/video60";

const CAP_VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
const CAP_READWRITE: u32 = 0x0100_0000;
const CAP_STREAMING: u32 = 0x0400_0000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;

// Linux _IOR('V', 0, struct v4l2_capability) where sizeof(cap)=104.
// This is the sole ioctl used by this module.
#[cfg(target_os = "linux")]
const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;

#[repr(C)]
#[derive(Default)]
struct V4l2Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[derive(Debug)]
pub struct V4l2QuerycapReport {
    pub driver: String,
    pub card: String,
    pub bus_info: String,
    pub version: String,
    pub capabilities: u32,
    pub device_caps: u32,
    pub effective_caps: u32,
    pub capture_mplane: bool,
    pub streaming: bool,
    pub readwrite: bool,
}

fn fixed_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn summarize(raw: &V4l2Capability) -> V4l2QuerycapReport {
    let effective_caps = if raw.capabilities & CAP_DEVICE_CAPS != 0 {
        raw.device_caps
    } else {
        raw.capabilities
    };
    V4l2QuerycapReport {
        driver: fixed_string(&raw.driver),
        card: fixed_string(&raw.card),
        bus_info: fixed_string(&raw.bus_info),
        version: format!(
            "{}.{}.{}",
            (raw.version >> 16) & 0xff,
            (raw.version >> 8) & 0xff,
            raw.version & 0xff
        ),
        capabilities: raw.capabilities,
        device_caps: raw.device_caps,
        effective_caps,
        capture_mplane: effective_caps & CAP_VIDEO_CAPTURE_MPLANE != 0,
        streaming: effective_caps & CAP_STREAMING != 0,
        readwrite: effective_caps & CAP_READWRITE != 0,
    }
}

#[cfg(target_os = "linux")]
fn verify_c5_sysfs() -> Result<()> {
    let dev = std::fs::read_to_string(format!("{SYSFS}/dev"))
        .context("cannot read C5 capture sysfs device number")?;
    if dev.trim() != "81:7" {
        bail!("C5 querycap refused: /sys video60 device identity is not 81:7");
    }
    let name = std::fs::read_to_string(format!("{SYSFS}/name"))
        .context("cannot read C5 capture sysfs name")?;
    if name.trim() != "vt-capture-dev" {
        bail!("C5 querycap refused: unexpected V4L2 device name");
    }
    let uevent = std::fs::read_to_string(format!("{SYSFS}/device/uevent"))
        .context("cannot read C5 capture platform driver")?;
    if !uevent.lines().any(|line| line == "DRIVER=v4l2_vt") {
        bail!("C5 querycap refused: driver is not v4l2_vt");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn query_c5_video60_capabilities() -> Result<V4l2QuerycapReport> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};

    if std::path::Path::new("/var/luna/preferences/vt_devmode").exists() {
        bail!("C5 querycap refused: VT developer marker exists");
    }
    verify_c5_sysfs()?;
    if std::mem::size_of::<V4l2Capability>() != 104 {
        bail!("V4L2 querycap structure does not match expected 104-byte ABI");
    }
    let metadata = std::fs::symlink_metadata(VIDEO60)
        .context("cannot inspect capture device before query")?;
    if !metadata.file_type().is_char_device() {
        bail!("C5 querycap refused: /dev/video60 is not a character device");
    }
    let expected = libc::makedev(81, 7);
    if metadata.rdev() != expected {
        bail!("C5 querycap refused: /dev/video60 does not have major=81 minor=7");
    }

    // O_RDONLY only. No retry using O_RDWR, root permission override,
    // alternative node, other ioctl, or capture mode.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(VIDEO60)
        .context("open O_RDONLY video60 refused; no retry with alternate permissions")?;

    let opened = file.metadata().context("cannot fstat opened video60 node")?;
    if !opened.file_type().is_char_device() || opened.rdev() != expected {
        bail!("opened V4L2 node identity changed; refusing query");
    }

    let mut capability = V4l2Capability::default();
    // SAFETY: fd is a verified char-device descriptor; the kernel writes
    // only the 104-byte v4l2_capability struct at the supplied valid address.
    // No frame buffers are allocated, mapped, or dereferenced.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_QUERYCAP, &mut capability) };
    if rc < 0 {
        return Err(anyhow!(
            "VIDIOC_QUERYCAP refused: {} (no fallback)",
            std::io::Error::last_os_error()
        ));
    }
    drop(file); // Explicitly release the sole device descriptor.
    Ok(summarize(&capability))
}

#[cfg(not(target_os = "linux"))]
pub fn query_c5_video60_capabilities() -> Result<V4l2QuerycapReport> {
    Err(anyhow!("C5 V4L2 querycap probe requires Linux"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_abi_and_code_are_stable() {
        assert_eq!(std::mem::size_of::<V4l2Capability>(), 104);
        #[cfg(target_os = "linux")]
        assert_eq!(VIDIOC_QUERYCAP, 0x8068_5600);
    }

    #[test]
    fn device_caps_take_precedence_when_flag_is_present() {
        let mut raw = V4l2Capability {
            capabilities: CAP_DEVICE_CAPS | CAP_STREAMING | CAP_READWRITE,
            device_caps: CAP_VIDEO_CAPTURE_MPLANE | CAP_STREAMING,
            ..Default::default()
        };
        raw.driver[..7].copy_from_slice(b"v4l2_vt");
        raw.version = (5 << 16) | (4 << 8) | 3;
        let report = summarize(&raw);
        assert!(report.capture_mplane && report.streaming);
        assert!(!report.readwrite);
        assert_eq!(report.version, "5.4.3");
        assert_eq!(report.driver, "v4l2_vt");
    }

    #[test]
    fn fallback_to_capabilities_without_device_flag() {
        let raw = V4l2Capability {
            capabilities: CAP_VIDEO_CAPTURE_MPLANE,
            device_caps: CAP_READWRITE,
            ..Default::default()
        };
        let report = summarize(&raw);
        assert_eq!(report.effective_caps, CAP_VIDEO_CAPTURE_MPLANE);
        assert!(!report.readwrite && report.capture_mplane);
    }

    #[test]
    fn fixed_string_stops_at_nul() {
        assert_eq!(fixed_string(b"vt-capture\0unused"), "vt-capture");
        assert_eq!(fixed_string(b"abc"), "abc");
    }
}
