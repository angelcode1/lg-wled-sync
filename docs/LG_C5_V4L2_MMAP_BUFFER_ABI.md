# LG C5 V4L2 MMAP buffer ABI — verified offline

Evidence: user-supplied `c5-private-abi.zip` with Thumb disassembly plus exact audited 32-bit ARM `libvtcapture.so.1.0.0`. **Do not upload the vendor binary to GitHub.**

Library SHA-256: `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`; Build ID: `9fb5e56d59c29ed242c0e4a66015c71601a4657e`.

## Native buffer path

| Method / address | Evidence |
|---|---|
| `VtDriver` constructor `0xf18a–0xf1ea` | Buffer type `9` = `V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE`; memory type `1` = `V4L2_MEMORY_MMAP` |
| `requestBuffer` `0x10314–0x10320` | `ioctl(..., 0xc0145608, ...)` = `VIDIOC_REQBUFS` |
| `queryBuffer` `0x10e92–0x10e9e` | `0xc0445609` = `VIDIOC_QUERYBUF`; reads each plane's length and offset |
| `queryBuffer` `0x10ef0–0x10f18` | Per-plane `mmap(NULL, length, PROT_READ\|PROT_WRITE, MAP_SHARED, fd, offset)`; saves returned mapped addresses |
| `queueBuffer` `0x110ae–0x110bc` | `0xc044560f` = `VIDIOC_QBUF` |
| `dequeueBuffer` `0x111dc–0x111ea` | `0xc0445611` = `VIDIOC_DQBUF` |
| `streamOnOff` `0x115a4–0x115b6` | `0x40045612` STREAMON and `0x40045613` STREAMOFF |
| `currentCaptureBuffInfo` `0xeaec–0xeb1c` | Retrieves a pair of previously mapped plane addresses and lengths from the internal ring buffer, rather than copying frame bytes |

**Conclusion:** the returned C5 descriptor pointers are **real V4L2 multi-planar device-backed MMAP addresses**, not arbitrary physical addresses to copy with kernel page-pinning APIs.

## Verified live C5 behavior

On independently verified Clear playback, display-output 320×180/stride 320 yielded Y 57,600 bytes and UV 28,800 bytes. Both plane VMAs were readable according to `maps`, but `smaps` reported `VM_IO=true`, `VM_PFNMAP=true`, `VM_MIXEDMAP=false`, `VM_DONTDUMP=true`, RSS=0, one segment each. A three-byte `process_vm_readv` probe returned `EFAULT`; VT marker and device-owner cleanup were successful.

V4L2 MMAP can legitimately use PFN-backed device mappings, which `process_vm_readv` need not be able to pin. **EFAULT from readv does not establish that an ordinary CPU load from the mapping will fail**, but neither does `PROT_READ` guarantee a safe CPU load, correct DMA coherency, or stable ownership.

## Remaining engineering gate

The native background `VtDriver::vtCaptureStartRoutine` handles dequeue/current-index/queue cycling. `currentCaptureBuffInfo` provides pointer metadata without an explicit hold/release API. Review buffer ownership, synchronization, and `clearBuffer` unmapping before any CPU read. A C++ try/catch does not catch `SIGBUS`/`SIGSEGV`; do not substitute unguarded Rust pointer dereferencing, `/proc/self/mem` or `process_vm_readv` retries.

Keep the narrow LS2 role, three fresh Clear classifications, fixed DISPLAY_OUTPUT path and no `vt_devmode` or DRM bypass. Do not commit vendor ELF bytes.

References: Linux kernel [V4L2 MMAP streaming](https://docs.kernel.org/5.17/userspace-api/media/v4l/mmap.html) and [QBUF/DQBUF ownership](https://cdn.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-qbuf.html).

Implementation guard: commit `27043a48432946c037c6d15e922cd4b5bc0cc0a8` rejects VM_IO/VM_PFNMAP/mixed mappings *before* kernel-copy syscalls.
