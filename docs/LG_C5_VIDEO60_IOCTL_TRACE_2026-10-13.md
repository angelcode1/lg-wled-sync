# LG C5 — verified live /dev/video60 native ioctl trace, 2026-10-13

**Scope:** OLED42C5PSA.AAUQLJD, kernel `5.4.268-324` (aarch64
kernel, armv7 vendor userspace). Existing restricted, positively
Clear-gated native metadata capture, **not a new capture implementation**.

The owner executed the pinned `test-frame-vma-flags
--acknowledge-native-capture` under
`/sys/class/video4linux/video60/dev_debug=7`, with original debug
value `0`, unique BEGIN/END markers, and automatic restoration.

## Verified experiment result

- Native command exit `0`; independently classified **3/3 spaced
  Clear** media samples; `DISPLAY_OUTPUT` plane `320x180`,
  stride `320`, `VM_IO=true`, `VM_PFNMAP=true` on Y and UV,
  RSS zero, no content reads. Normal `stop → postprocess → finalize
  → release` completed successfully.
- `DEV_DEBUG_ORIG=0` and `DEV_DEBUG_NOW=0`; tagged kernel
  trace had BEGIN and END and `TRACE_LINES=26`.
- No `/var/luna/preferences/vt_devmode`; `fuser` detected
  no /dev/video60 owner after the run. No permissions, LS2 role,
  vendor marker or source policies changed.
- Tagged kernel time BEGIN `4911.744347`, END `4913.968769`.
  Device open `4913.828820`, stream on `4913.918646`,
  stream off `4913.935729`, close `4913.951120`.
  Streaming interval ≈ **17.083 ms**. One short diagnostic
  cannot establish continuous-frame stability.

## Observed ioctl and file-operation sequence

Chronology reconstructed directly from the owner's `dev_debug=7`
output. Extended controls use the **standard V4L2 G_EXT_CTRLS/
S_EXT_CTRLS API** with driver-specific *control IDs*; this trace
does not show custom, nonstandard ioctl command numbers.

| # | Operation | Audited value / inference |
|---|---|---|
| 1 | open `/dev/video60` | `v4l2_open` succeeds; node belongs to platform `v4l2_vt` |
| 2 | `VIDIOC_QUERYCAP` | `driver=o22`, `card=vt`, `version=0x000504ff`, `device_caps=0x04201000` |
| 3 | `VIDIOC_G_EXT_CTRLS` | user-class ID `0x00982a00`, payload **40 bytes** |
| 4 | `VIDIOC_S_FMT` | `VIDEO_CAPTURE_MPLANE`, requested **320×180**, `pixelformat=0`, `num_planes=0` *in the post-ioctl debug snapshot* |
| 5 | `VIDIOC_S_EXT_CTRLS` | ID `0x00982a03`, payload **16 bytes** |
| 6 | `VIDIOC_G_EXT_CTRLS` | ID `0x00982a03`, payload **16 bytes** |
| 7 | `VIDIOC_REQBUFS` | `count=5`, `VIDEO_CAPTURE_MPLANE`, `V4L2_MEMORY_MMAP` |
| 8 | `VIDIOC_G_EXT_CTRLS` | ID `0x00982a01`, payload **20 bytes** |
| 9 | `VIDIOC_G_EXT_CTRLS` | ID `0x00982a02`, payload **28 bytes** |
| 10–15 | `VIDIOC_QUERYBUF` and mmap | indices **0, 1, 2**; two `mmap` calls per queried index, so **6 total mappings** |
| 16 | `VIDIOC_G_CTRL` | ID `0x00982a07` returned **60** |
| 17 | `VIDIOC_S_CTRL` | ID `0x00982a08`, value **2** (vendor-specific meaning; **not necessarily 2 fps**) |
| 18 | `VIDIOC_STREAMON` | `VIDEO_CAPTURE_MPLANE` |
| 19 | `VIDIOC_STREAMOFF` | same capture type; ≈17 ms later |
| 20 | `VIDIOC_REQBUFS` | `count=0`, `V4L2_MEMORY_MMAP`: queue teardown |
| 21 | release | `v4l2_release`, successful |

**Buffer count is resolved at the ioctl/API level:** the metadata
probe sets `_LibVtCaptureProperties.buf_cnt=3`; the audited native
`preprocessVtCapture` copies this value to `VtDriver+0xac`
(`0x1209e–0x120b6`), and `requestBuffer` copies that same
field into `v4l2_requestbuffers.count` before
`ioctl(VIDIOC_REQBUFS)` (`0x102fa–0x10320`). V4L2 core's
`dev_debug` prints the request structure **after** the ioctl
handler, not on entry. Hence `count=5` in the successful trace is
the count **returned by the driver**, consistent with it increasing
the originally requested **3** to **5**; this is explicitly permitted
by the V4L2 API. The vendor's `initBuffer` and
`preprocessVtCapture` loops still use the internal count **3**,
so only `QUERYBUF(0..2)` and the six Y/UV mappings are observed.
Exactly *why* the driver grants five buffers (minimum allocation,
internal latency, etc.) is unknown without driver-level evidence.
Unqueried buffers are not permission to access or reassign them.

References: https://www.kernel.org/doc/html/v5.5/media/uapi/v4l/vidioc-reqbufs.html
and Linux V4L2 ioctl debug dispatch:
https://code.googlesource.com/linux/torvalds/linux/+/826c484166f0f74bd8fc09220f99dc937c9297cf/drivers/media/v4l2-core/v4l2-ioctl.c

**Missing operations are not proof of their absence.** `dev_debug=7`
deliberately suppresses `VIDIOC_QBUF`, `VIDIOC_DQBUF` and
`poll`. The exact LG ELF's `VtDriver::vtCaptureStartRoutine`
already shows a dequeue → publish index → immediate requeue worker
sequence. A descriptor is not a frame lease.

**Format caution:** `S_FMT` logs a zero FourCC/plane count in the kernel's **post-ioctl**
argument snapshot. This suggests the driver did not populate a
standard V4L2 pixel format; the native output plane dimensions are
supplied separately through LG's extended controls. It does not prove
that the produced native NV12 buffers have zero dimensions.
Independent `G_FMT` on the *idle* node returned success with
`width=height=0`, `num_planes=0`. These values do not constitute
a valid video format. Prefer the already working vendor
`vtCapture_planeInfo` for actual output geometry and mapping sizes.

## Extended-control IDs correlated to exact C5 ELF

Audited private LG library `/usr/lib/libvtcapture.so.1.0.0`,
SHA-256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`,
Build ID `9fb5e56d59c29ed242c0e4a66015c71601a4657e`.

| Control | Native symbol and ARM Thumb instruction evidence |
|---|---|
| `0x982a00` / 40 B, G_EXT | `VtDriver::getDeviceCapabilityInfo()`; constructs ID/size at `0xf9ec–0xf9f8`, calls G_EXT at `0xfa22–0xfa30` |
| `0x982a03` / 16 B, S_EXT | `VtDriver::setCapturePlanePropertes()`; ID/size at `0xfd40–0xfd50`, calls S_EXT at `0xfd70–0xfd7e` |
| `0x982a03` / 16 B, G_EXT | `VtDriver::getCapturePlanePropertes()`; ID/size at `0xfefe–0xff10`, calls G_EXT at `0xff38–0xff46` |
| `0x982a01` / 20 B, G_EXT | `VtDriver::getCapturePlaneInfo()`; ID/size at `0x1040a–0x10422`, calls G_EXT at `0x1043e–0x1044c` |
| `0x982a02` / 28 B, G_EXT | `VtDriver::getCaptureVideoWindowInfo()`; ID/size at `0x10c28–0x10c3c`, calls G_EXT at `0x10c5c–0x10c6a` |
| `0x982a07`, G_CTRL | `VtDriver::getCaptureFrameRate()`, at `0x101aa–0x101c8` |
| `0x982a08`, S_CTRL | `VtDriver::setCaptureFrameRate()`, at `0x100a2–0x100c0` |

The names are **internal LG library method names**, not public
driver ABI documentation. The semantics and lifetime requirements of
their payload structures require offline examination of the exact
vendor code or supported LG headers. Nothing here authorizes
mutation of the driver outside the permitted native API.

## Decision / next work

1. **Stop repeating standard QUERYCAP/ENUM_FMT/G_FMT probes.**
   The standard V4L2 lifecycle has been observed directly.
2. The vendor-side count accounting is now explained:
   `buf_cnt=3` requested, driver reports **5** after REQBUFS,
   and LG loops over **3** mapped buffers. Remaining offline
   work: investigate the driver's minimum-buffer requirement
   and whether an authorized *consumer-held* buffer mechanism
   exists. Do not infer buffer ownership from an extra allocation.
3. Review vendor extended-control payload structures and any driver
   module source/binary only if lawfully available. There is no need
   to send another active TV ioctl for this.
4. Retain the fingerprint-specific block on ordinary raw Rust
   pointer reads. Success of MMAP and native streaming does
   **not** resolve the immediate-requeue ownership race or DMA
   coherency. Do not copy mapped video bytes until an explicitly
   authorized stable-buffer acquisition contract is established.
5. Preserve positive independent 3× Clear classification and
   native DRM/HDCP refusals; Unknown and Protected remain blocked.

V4L2 per-node debug mask reference (Linux 5.4):
https://kernel.org/doc/html/v5.4/media/kapi/v4l2-dev.html

No vendor ELF or proprietary instruction stream is checked into this
repository; only independently verified behavioral findings and
symbol names/offsets are recorded.
