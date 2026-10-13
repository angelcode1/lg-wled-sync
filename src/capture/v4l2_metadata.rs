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

// Linux _IOR('V', 0, struct v4l2_capability), sizeof=104.
// The existing querycap command invokes only this ioctl.
#[cfg(target_os = "linux")]
const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;

// Linux _IOWR('V', 2, struct v4l2_fmtdesc), sizeof=64.
#[cfg(target_os = "linux")]
const VIDIOC_ENUM_FMT: libc::c_ulong = 0xc040_5602;

// The C5 is 32-bit ARM. v4l2_format = u32 type + union raw_data[200].
// _IOWR('V', 4, sizeof(v4l2_format)) = 0xc0cc5604 on ARM32.
// This deliberately avoids a host-64-bit definition with pointer members.
#[cfg(target_os = "linux")]
const VIDIOC_G_FMT: libc::c_ulong = 0xc0cc_5604;

const MPLANE_CAPTURE: u32 = 9;
const MAX_ENUM_FORMATS: u32 = 16;

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

#[repr(C)]
#[derive(Default)]
struct V4l2FormatDescription {
    index: u32,
    kind: u32,
    flags: u32,
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct V4l2Format {
    kind: u32,
    raw_data: [u8; 200],
}

#[derive(Debug)]
pub struct V4l2EnumeratedFormat {
    pub index: u32,
    pub fourcc: String,
    pub description: String,
    pub flags: u32,
}

#[derive(Debug)]
pub struct V4l2CurrentFormat {
    pub width: u32,
    pub height: u32,
    pub fourcc: String,
    pub num_planes: u8,
    pub plane_stride: Vec<u32>,
    pub plane_sizeimage: Vec<u32>,
}

#[derive(Debug)]
pub struct V4l2FormatReport {
    pub formats: Vec<V4l2EnumeratedFormat>,
    pub list_truncated: bool,
    pub current: Option<V4l2CurrentFormat>,
    pub current_unsupported: bool,
    pub ioctl_calls: u32,
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

fn fourcc_string(v: u32) -> String {
    v.to_le_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}

fn sanitized_description(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    bytes[..end]
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect()
}

fn parse_mplane_format(raw: &V4l2Format) -> Result<V4l2CurrentFormat> {
    if raw.kind != MPLANE_CAPTURE {
        bail!("VIDIOC_G_FMT returned an unexpected buffer type");
    }
    let value = |pos: usize| -> u32 {
        u32::from_ne_bytes(raw.raw_data[pos..pos + 4].try_into().unwrap())
    };
    let num_planes = raw.raw_data[180];
    if num_planes > 8 {
        bail!("VIDIOC_G_FMT plane count exceeds V4L2 multi-planar maximum");
    }
    let mut plane_stride = Vec::new();
    let mut plane_sizeimage = Vec::new();
    for idx in 0..usize::from(num_planes) {
        // v4l2_pix_format_mplane: 5 u32 fields, then 8 plane records
        // of 20 bytes: sizeimage u32, bytesperline u32, reserved u16[6].
        let start = 20 + idx * 20;
        plane_sizeimage.push(value(start));
        plane_stride.push(value(start + 4));
    }
    Ok(V4l2CurrentFormat {
        width: value(0),
        height: value(4),
        fourcc: fourcc_string(value(8)),
        num_planes,
        plane_stride,
        plane_sizeimage,
    })
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
fn open_verified_video60() -> Result<std::fs::File> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};

    if std::path::Path::new("/var/luna/preferences/vt_devmode").exists() {
        bail!("C5 querycap refused: VT developer marker exists");
    }
    verify_c5_sysfs()?;
    if std::mem::size_of::<V4l2Capability>() != 104 {
        bail!("V4L2 querycap structure does not match expected 104-byte ABI");
    }
    let metadata =
        std::fs::symlink_metadata(VIDEO60).context("cannot inspect capture device before query")?;
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

    let opened = file
        .metadata()
        .context("cannot fstat opened video60 node")?;
    if !opened.file_type().is_char_device() || opened.rdev() != expected {
        bail!("opened V4L2 node identity changed; refusing query");
    }
    Ok(file)
}

#[cfg(target_os = "linux")]
fn querycap_fd(file: &std::fs::File) -> Result<V4l2QuerycapReport> {
    use std::os::fd::AsRawFd;
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
    Ok(summarize(&capability))
}

#[cfg(target_os = "linux")]
pub fn query_c5_video60_capabilities() -> Result<V4l2QuerycapReport> {
    let file = open_verified_video60()?;
    let report = querycap_fd(&file)?;
    drop(file);
    Ok(report)
}

/// On a single verified O_RDONLY descriptor: 1 QUERYCAP, up to 16 ENUM_FMT
/// requests, and at most one G_FMT. Never S_FMT, REQBUFS, QBUF or STREAMON.
#[cfg(target_os = "linux")]
pub fn query_c5_video60_formats() -> Result<V4l2FormatReport> {
    use std::os::fd::AsRawFd;

    let file = open_verified_video60()?;
    if std::mem::size_of::<V4l2FormatDescription>() != 64
        || std::mem::size_of::<V4l2Format>() != 204
    {
        bail!("V4L2 frame format metadata structure ABI does not match ARM32");
    }
    let caps = querycap_fd(&file)?;
    if !caps.capture_mplane {
        bail!("device does not advertise multi-planar capture");
    }
    let mut formats = Vec::new();
    let mut calls = 1u32; // QUERYCAP.
    let mut list_truncated = true;
    for index in 0..MAX_ENUM_FORMATS {
        let mut desc = V4l2FormatDescription {
            index,
            kind: MPLANE_CAPTURE,
            ..Default::default()
        };
        calls += 1;
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_ENUM_FMT, &mut desc) };
        if rc < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINVAL) && index > 0 {
                list_truncated = false;
                break;
            }
            return Err(anyhow!(
                "VIDIOC_ENUM_FMT index {index} refused: {error} (no fallback)"
            ));
        }
        formats.push(V4l2EnumeratedFormat {
            index,
            fourcc: fourcc_string(desc.pixelformat),
            description: sanitized_description(&desc.description),
            flags: desc.flags,
        });
    }

    let mut request = V4l2Format {
        kind: MPLANE_CAPTURE,
        raw_data: [0; 200],
    };
    calls += 1;
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_G_FMT, &mut request) };
    let (current, current_unsupported) = if rc == 0 {
        (Some(parse_mplane_format(&request)?), false)
    } else {
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINVAL | libc::ENOTTY) => (None, true),
            _ => {
                return Err(anyhow!("VIDIOC_G_FMT refused: {error} (no fallback)"));
            }
        }
    };
    drop(file);
    Ok(V4l2FormatReport {
        formats,
        list_truncated,
        current,
        current_unsupported,
        ioctl_calls: calls,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn query_c5_video60_formats() -> Result<V4l2FormatReport> {
    Err(anyhow!("C5 V4L2 format metadata probe requires Linux"))
}

#[cfg(not(target_os = "linux"))]
pub fn query_c5_video60_capabilities() -> Result<V4l2QuerycapReport> {
    Err(anyhow!("C5 V4L2 querycap probe requires Linux"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mplane_format_structs_and_fourcc() {
        assert_eq!(std::mem::size_of::<V4l2FormatDescription>(), 64);
        assert_eq!(std::mem::size_of::<V4l2Format>(), 204);
        #[cfg(target_os = "linux")]
        {
            assert_eq!(VIDIOC_ENUM_FMT, 0xc040_5602);
            assert_eq!(VIDIOC_G_FMT, 0xc0cc_5604);
        }
        assert_eq!(fourcc_string(0x3231_4d4e), "NM12");
        assert_eq!(sanitized_description(b"NV12\\rbad"), "NV12\\rbad");
    }

    #[test]
    fn parse_current_mplane_format_from_bounded_struct() {
        let mut fmt = V4l2Format {
            kind: MPLANE_CAPTURE,
            raw_data: [0; 200],
        };
        fmt.raw_data[0..4].copy_from_slice(&320u32.to_ne_bytes());
        fmt.raw_data[4..8].copy_from_slice(&180u32.to_ne_bytes());
        fmt.raw_data[8..12].copy_from_slice(&0x3231_4d4eu32.to_ne_bytes());
        fmt.raw_data[20..24].copy_from_slice(&57600u32.to_ne_bytes());
        fmt.raw_data[24..28].copy_from_slice(&320u32.to_ne_bytes());
        fmt.raw_data[40..44].copy_from_slice(&28800u32.to_ne_bytes());
        fmt.raw_data[44..48].copy_from_slice(&320u32.to_ne_bytes());
        fmt.raw_data[180] = 2;
        let parsed = parse_mplane_format(&fmt).unwrap();
        assert_eq!(parsed.width, 320);
        assert_eq!(parsed.height, 180);
        assert_eq!(parsed.num_planes, 2);
        assert_eq!(parsed.plane_stride, vec![320, 320]);
        assert_eq!(parsed.plane_sizeimage, vec![57600, 28800]);
        fmt.raw_data[180] = 9;
        assert!(parse_mplane_format(&fmt).is_err());
        fmt.kind = 1;
        assert!(parse_mplane_format(&fmt).is_err());
    }

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
