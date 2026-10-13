# C5 V4L2 format metadata (no capture)

## Confirmed QUERYCAP

On 2026-10-13 the owner's verified `/dev/video60` returned
`driver=o22 card=vt bus_info= version=5.4.255`,
`capabilities=0x84201000`, `device_caps=0x04201000`, and
`effective_caps=0x04201000`. The flags mean
`VIDEO_CAPTURE_MPLANE`, `EXT_PIX_FORMAT`, and `STREAMING`.
The `READWRITE` flag is absent: the device does not advertise
frame delivery through `read()`. The query closed the descriptor,
left no `vt_devmode` marker or detected video60 owner, and
performed no capture operations.

The driver string `o22` is a literal returned by the device;
the separately observed platform driver identity is `v4l2_vt`.
Do not confuse the two or treat the vendor's `version=5.4.255`
as the running Linux kernel version (`5.4.268-324`).

## New opt-in diagnostic

```
lg-hue-sync test-v4l2-formats --acknowledge-device-open
```

The command verifies exactly the observed sysfs identity and major/minor
number, rejects any developer marker, opens `/dev/video60`
`O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_NOFOLLOW`, calls `QUERYCAP`,
then uses `VIDIOC_ENUM_FMT` for multi-planar capture type `9`
for at most 16 indexes. A terminating `EINVAL` **after** at least
one returned format is the documented end-of-list. It issues exactly
one `VIDIOC_G_FMT` to inspect the current metadata; `EINVAL` or
`ENOTTY` on G_FMT is reported as unsupported. Other failures stop
the operation. All descriptors close automatically on failure.

The result includes fourcc, description, flags, and, if available,
current width/height, plane count, plane bytes per line and sizeimage.
The latter is a **format snapshot** and does not confer an application
lease or demonstrate the device is streaming.

No `S_FMT`, `TRY_FMT`, `REQBUFS`, `QBUF`, `DQBUF`,
`STREAMON`, `MMAP` or pixel access is permitted by this diagnostic.
Do not retry using wider permissions or an alternative device if refused.

Kernel documentation:
- https://cdn.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-enum-fmt.html
- https://www.kernel.org/doc/html/v4.14/media/uapi/v4l/vidioc-g-fmt.html

## Live ENUM_FMT refusal (2026-10-13) and separate current-format query

The owner ran the format metadata probe on the verified device; the initial
`VIDIOC_QUERYCAP` succeeded, but **`VIDIOC_ENUM_FMT` index 0 returned
`ENOTTY` (errno 25)**. The command exited `1` without calling `G_FMT`.
Afterwards `vt_devmode` remained absent and fuser detected no video60 owner.
The format list and current FourCC therefore remain unknown.

Offline analysis of the owner's exact LG `libvtcapture.so.1.0.0`
(SHA-256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`)
found `VtDriver::setFMT` invokes the standard
`VIDIOC_S_FMT` ioctl `0xc0cc5605` at `0xfc42–0xfc50`.
This establishes that LG itself configures the V4L2 multi-planar
format; it does **not** prove the driver supports `G_FMT`.

The separately opted-in one-shot command

```sh
lg-hue-sync test-v4l2-current-format --acknowledge-device-open
```

opens exactly the verified `/dev/video60` node read-only and invokes
`VIDIOC_QUERYCAP` followed by **exactly one `VIDIOC_G_FMT`**
(`0xc0cc5604`). It never invokes ENUM_FMT, S_FMT or any buffer or
streaming ioctl. Unsupported/refused `G_FMT` is terminal: no fallback,
no privilege change, no different device, no vendor state mutation.

If supported, the output describes *the device's current format
metadata only*. It does not prove buffer ownership, CPU accessibility,
freshness, DMA synchronization, or authorization to start capture.
If rejected, stop standard format probing and prioritize the supported
vendor `vtCapture_planeInfo` metadata path instead.

## 2026-10-13: successful idle G_FMT with empty values

The independently selected one-shot G_FMT returned success (0) but
\`width=0 height=0 fourcc=.... num_planes=0\`, with no plane strides or
image sizes. Device was idle. These zeros are **not an actual capture
format**; the driver likely has no format configured in that state.

This repo's ARM32 G_FMT request is already **0xC0CC5604**, with a
204-byte \`v4l2_format\`. It does not issue the aarch64 native 208-byte
request \`0xC0D05604\`. A 64-bit kernel with ARM32 userspace uses
compat ioctl translation; sending the wrong request can silently
misalign returned fields. That specific mistake is not present here.

The earlier \`ENUM_FMT\` ENOTTY is due to the absent registered
enum op and fixed \`valid_ioctls\` set: it is not a transient
playback/permissions event. \`EXT_PIX_FORMAT\` is injected by V4L2
core and does not establish a special vendor driver capability.

## Preferred next diagnostic: existing capture + per-node dev_debug

\`scripts/c5_v4l2_dev_debug_trace.sh\` wraps the previously tested
\`/var/home/root/c5-histogram-probe test-frame-vma-flags
--acknowledge-native-capture\` with the device-specific sysfs
\`dev_debug=7\` trace. The known-good executable is compared against
the archived \`lg-hue-sync.59ec38e\` staging binary to avoid unexpected
behavior. The capture command itself performs three spaced \`Clear\`
checks, a single metadata-only native capture lifecycle and cleanup.

The trace checks \`video60\` driver identity, no developer marker,
no detected device owner, and writable \`dev_debug\` and \`/dev/kmsg\`.
It saves the original \`dev_debug\`, writes BEGIN and END markers to
the kernel log, enables mask 7 temporarily and uses a shell EXIT/signal
trap to restore the original state. It does not change LS2 roles,
touch the vendor marker, read pixels, or create a second capture
queue. Failure to restore must be treated as a serious error and
corrected immediately.

Linux 5.4 documents mask 0x01 as ioctl names, 0x02 as ioctl arguments,
0x04 as file operations; 0x08 controls noisy QBUF/DQBUF and read/write
logging and 0x10 controls poll. The trace is scoped to \`video60\`:
https://kernel.org/doc/html/v5.4/media/kapi/v4l2-dev.html

**Do not blindly replicate a traced ioctl sequence**. Standard
ioctls identify the vendor's operations, not an application-held buffer
lease, DMA synchronization guarantee or DRM authorization. A native
status refusal, \`Unknown\` or \`Protected\` playback is always a stop
condition. If there are no video60 messages, verify marker presence,
dev_debug availability and the actual successful invocation before
concluding the driver was not used.
