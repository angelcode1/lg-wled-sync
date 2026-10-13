# LG C5 /dev/video60: one-shot metadata-only QUERYCAP

## Hardware identity established on 2026-10-13

On OLED42C5PSA.AAUQLJD (kernel 5.4.268-324) the user reported:

- `/dev/video60`: character device `root:composit`, **major 81 minor 7**
- `/sys/class/video4linux/video60/dev`: `81:7`
- `name`: `vt-capture-dev`
- platform driver: `v4l2_vt`; modalias `platform:v4l2_vt`
- `fuser /dev/video60`: no owner **detected**, not proof that no
  kernel or other device participant exists
- `/var/luna/preferences/vt_devmode`: absent

Earlier live testing showed that LG's currentCaptureBuffInfo returns two
device-backed V4L2 MMAP planes (`VM_IO, VM_PFNMAP`), and that its native
worker immediately requeues the published index after DQBUF. The normal
Rust raw-pointer frame path is disabled for this exact C5 ELF.

## Why querycap?

`VIDIOC_QUERYCAP` only returns identification and capability flags;
it does not acquire or dequeue image frames. The diagnostic is a
**separate** opt-in executable command, not part of the sync loop.

```sh
lg-hue-sync test-v4l2-querycap --acknowledge-device-open
```

It verifies the exact sysfs name, driver, major/minor and char-device
type, rejects the developer marker, opens **only** `/dev/video60` once
with `O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_NOFOLLOW`, issues exactly one
`VIDIOC_QUERYCAP` ioctl and closes the FD. It logs driver/card/bus,
version, raw and effective capability flags; it does not allocate buffers,
start a stream, read pixels, change permissions or enable alternate
capture paths.

A refusal is terminal. Do **not** try `O_RDWR`, different root/group
privileges, another device node, `/dev/forcecapture`, `vt_devmode`
or wider LS2 roles to get around a refusal.

The command is deliberately not gated on active media classification:
it never accesses frame pixels or starts capture. Any future active
capture or buffer operation must retain three fresh spaced independent
`Clear` classifications and never access `Unknown` or `Protected`
video.

Even if QUERYCAP reports `VIDEO_CAPTURE_MPLANE` and `STREAMING`,
that is a *capability*, not an entitlement or a frame-ownership contract.
No second REQBUFS/STREAMON or unmanaged MMAP is allowed on this evidence.

Documentation: https://docs.kernel.org/userspace-api/media/v4l/vidioc-querycap.html
