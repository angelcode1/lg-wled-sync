# C5 /dev/video60 ownership-safe capture: staged investigation

**Hardware:** OLED42C5PSA.AAUQLJD, webOS 10. Proven LG `libvtcapture.so.1.0.0` SHA-256
`6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`.
This document does not include proprietary vendor bytes.

## Where the ABI audit leaves us

The C5 maps `V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE` buffers with `V4L2_MEMORY_MMAP` and
`mmap(PROT_READ|PROT_WRITE, MAP_SHARED)` on `/dev/video60`. It dequeues a buffer,
updates the shared current index, and immediately requeues that same buffer.
`vtCapture_currentCaptureBuffInfo` reports addresses and plane sizes but grants
no consumer ownership or cache synchronization. The ordinary Rust raw-frame
path is deliberately disabled for this exact vendor ELF.

Live Y/UV descriptor mappings were `VM_IO` and `VM_PFNMAP`; an attempted
`process_vm_readv` copy returned `EFAULT`. A later metadata-only test
confirmed the mapping flags and clean native shutdown. The kernel-copy command
is now gated against these special VMAs.

## Next step: sysfs-only identification (zero device opens)

Use `scripts/c5_v4l2_sysfs_inventory.sh` on the TV from a read-only SSH
session. The collector only reads `/sys/class/video4linux/video60`, reports
`/dev/video60` permissions, checks the `vt_devmode` marker and optionally
inspects best-effort `fuser` ownership. It does **not** open the device, issue
`ioctl`, allocate buffers, start capture, register LS2 clients, or read pixels.
A missing sysfs entry or driver symlink is an observation, not authorization
to improvise a different capture path.

Check that sysfs major/minor and driver owner correspond to the device
node. A character node by itself does not prove an authorized, independent
capture owner is possible.

## Mandatory gates for any future independently owned V4L2 capture

1. **Entitlement and ownership:** demonstrate a supported authorization path
   to the capture node under the existing narrow LG caller identity, and no
   competing owner. Never use root/direct V4L2 access to defeat a vendor
   refusal, whitelist requirement, scrambling classification or HDCP policy.
2. **Immutable source policy:** three fresh spaced `Clear` classifications
   on a known-unprotected local source immediately before any native capture;
   `Unknown` and `Protected` remain fail-closed. No mode/source fallback.
3. **Non-mutating device discovery:** if separately authorized, the next
   experiment could open the exact proven node once with `O_RDONLY |
   O_NONBLOCK | O_CLOEXEC` and call only `VIDIOC_QUERYCAP` before closing.
   A successful query is metadata, not approval to stream.
4. **One queue owner:** no second `VIDIOC_REQBUFS` or `VIDIOC_STREAMON`
   while LG's own capture thread/another owner is active. Do not pass
   `currentCaptureBuffInfo` pointers across processes: they belong to the
   caller's mmap address space, not the child.
5. **Completed buffer lease:** any eventual reader must own a dequeued
   buffer until conversion completes; `QBUF` only after processing. Respect
   driver bytesused, per-plane length/stride, cache coherency and buffer
   error flags. Driver/permission failures are terminal, not retries with
   different privileges.
6. **Fault containment:** a child process can isolate `SIGBUS`/faults but
   cannot solve buffer ownership or DMA cache synchronization. No unguarded
   pointer copy until the preceding gates are satisfied.
7. **Output safety:** never emit synthetic/mock colors as real screen data,
   never keep unapproved frame data or publish to WLED during diagnostics;
   cleanly release resources even when metadata reads fail.

Kernel V4L2 references:
- https://cdn.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-qbuf.html
- https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-querycap.html
- https://docs.kernel.org/6.12/userspace-api/media/v4l/mmap.html

This analysis is an engineering safety plan, not confirmation of direct
C5 capture access. Use only proven functions and the actual TV's results.
