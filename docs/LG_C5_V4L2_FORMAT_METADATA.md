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
