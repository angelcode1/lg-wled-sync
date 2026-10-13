# LG C5 — V4L2 capture worker and current-frame ownership audit

**Scope:** verified ARM32 LG vendor library `libvtcapture.so.1.0.0`,
SHA-256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`,
Build ID `9fb5e56d59c29ed242c0e4a66015c71601a4657e`. Audited offline from
the owner-supplied `c5-private-abi.zip`. This document contains no vendor ELF
or copied proprietary disassembly. The Linux V4L2 MMAP API is not DRM bypass.

## Worker sequence: direct evidence

- `VtDriver::vtCaptureStartRoutine(void*)` at `0x11340`, function size
  `0x1f4`: invokes `dequeueBuffer(int*)` at `0x11430–0x11438`.
  On success (`r0 == 0`) it branches at `0x11444–0x11446` to `0x113b6`.
- At `0x113b6–0x113be` it loads the dequeued index and calls
  `VtDriver::setCurrentBufferIndex(index)`.
- At `0x113be–0x113c6` **the same index** is passed immediately to
  `VtDriver::queueBuffer(index)`. No consumer callback, lease, or waiting
  window intervenes.
- `dequeueBuffer` at `0x111dc–0x111ea` invokes
  `VIDIOC_DQBUF` (`0xc0445611`).
- `queueBuffer` invokes `VIDIOC_QBUF` (`0xc044560f`).
  The index is published before the driver regains ownership; on completion
  of QBUF the underlying buffer may be filled/reused by hardware.
- `setCurrentBufferIndex` at `0x105f0–0x10610` locks a mutex, stores
  the shared index and unlocks. This protects the *index update*, not the
  subsequent frame consumption. `currentCaptureBuffInfo` at
  `0xe96c–0xecbf` copies mapped plane pointers and lengths into its
  caller-provided record; it does not establish a buffer hold/release lease.

**Inference:** the library exposes a latest-frame *pointer snapshot*,
not a dequeued frame owned by the caller. The consumer can race the
native worker's immediate requeue and subsequent DMA writes. Even when
an ordinary CPU read succeeds, frame freshness, stable contents and
cache synchronization are not guaranteed by this ABI.

## Earlier live observations

On three independently spaced Clear media classifications, the C5
reported `DISPLAY_OUTPUT` 320x180, stride 320, Y 57,600 bytes and UV
28,800 bytes. `/proc/self/maps` covered both address ranges, while
`/proc/self/smaps` showed `VM_IO=true`, `VM_PFNMAP=true`,
`VM_MIXEDMAP=false`, `VM_DONTDUMP=true`, RSS zero for each plane.
A 3-byte `process_vm_readv` attempt returned `EFAULT`, and
native cleanup remained successful. Device-backed mappings can reject
page-pinning APIs; that alone does not prove CPU access is impossible.

## Safety decision and next options

- **Keep the exact-C5 fingerprint blocked from the general Rust
  `VtCapture::try_new()` frame path**, which otherwise calls
  `std::slice::from_raw_parts` on these pointers. Fail before create/init
  (no framebuffer copy, no pixel dereference, no new native capture
  session). The separate, explicit metadata diagnostics remain available.
- The normal sync pipeline already treats `MockCapture` as non-hardware
  and does not publish synthetic colors as real video data.
- Do **not** retry `process_vm_readv` on PFN/IO mappings or add an
  unguarded `memcpy`/Rust pointer load to this C5 path.
- An isolated child process prevents a signal from killing the parent,
  but **does not solve this buffer's driver ownership, DMA coherence, or
  page-fault behavior**. A child process cannot blindly dereference
  foreign-process vendor pointers.
- Next investigate whether the vendor API offers a true frame acquire
  and release method, or whether a separately audited V4L2 capture
  interface can hold a dequeued buffer until a bounded CPU read is
  complete. Any such path must keep the existing three-sample
  independently verified Clear gate and honor native refusals, avoid
  protected or unknown playback, and require explicit operator consent
  before touching video data.

## Reference behavior

Linux V4L2 MMAP documentation:
https://docs.kernel.org/6.12/userspace-api/media/v4l/mmap.html

V4L2 QBUF/DQBUF ownership contract:
https://docs.kernel.org/6.4/userspace-api/media/v4l/vidioc-qbuf.html

Kernel documentation specifies that the user processes a dequeued
buffer and enqueues it **when the data is no longer needed**. Access
after returning a buffer to the driver is not a sound ownership model.

The upstream `webosbrew/hyperion-webos` `libvtcapture` backend obtains
pointers via `vtCapture_currentCaptureBuffInfo` and converts NV12 via
libyuv; that is compatibility evidence, not proof of a safe C5 frame
lease. No `/dev/forcecapture`, policy bypass, extra LS2 permissions or
`vt_devmode` changes are authorized by this audit.
