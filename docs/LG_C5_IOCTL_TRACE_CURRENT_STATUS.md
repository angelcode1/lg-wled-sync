# C5 format discovery result and known constraints

The TV's `v4l2_vt` /dev/video60 advertises multi-planar streaming
with `VIDIOC_QUERYCAP` but supplies no `VIDIOC_ENUM_FMT` operation
(`ENOTTY` at index zero). A separate `VIDIOC_G_FMT` using the
correct ARM32 `0xC0CC5604` request returned success but **zero
geometry and zero planes when idle**.

A successful live `dev_debug=7` trace on 2026-10-13 confirmed the
LG native lifecycle: `QUERYCAP → G_EXT_CTRLS(0x982a00) →
S_FMT(320x180) → S/G_EXT_CTRLS(0x982a03) → REQBUFS(5,MMAP) →
G_EXT_CTRLS(0x982a01,0x982a02) → QUERYBUF(0..2) →
6×mmap → G_CTRL(0x982a07=60) → S_CTRL(0x982a08=2) →
STREAMON → STREAMOFF → REQBUFS(0) → release`.

The 320×180, three two-plane-buffer arrangement is *not* proof that a
consumer holds buffer ownership. LG's capture worker immediately
requeues each dequeued index.

Read the authoritative timeline and offline control-name correlation in
[LG_C5_VIDEO60_IOCTL_TRACE_2026-10-13.md](LG_C5_VIDEO60_IOCTL_TRACE_2026-10-13.md).
