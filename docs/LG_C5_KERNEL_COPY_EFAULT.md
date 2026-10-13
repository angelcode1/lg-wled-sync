# LG C5 kernel-copy EFAULT: safe next diagnostic

On 2026-10-13, on a known-unprotected MP4 in LG Media Discovery, the
`test-frame-kernel-copy` command passed three spaced Clear media checks but
returned `kernel-mediated self-copy refused: Bad address (os error 14)`,
`FRAME_KERNEL_COPY_EXIT=1`.

After the failure, the owner observed `VT_MARKER_ABSENT` and
`VIDEO60_NO_OWNER_DETECTED`. No pixel values were logged.

Prior successful diagnostics verified the same 320x180 display-output path:
`Y=57600`, `UV=28800`, stride `320`, one valid descriptor and full
readable `/proc/self/maps` range coverage. Those observations do not prove
CPU-readability. `EFAULT` could be caused by device/PFN-backed mappings,
unavailable pages or another kernel copy constraint; the log does not identify
whether the Y or UV copy failed. The origin remains unresolved.

Do not retry `test-frame-kernel-copy`, use `/proc/self/mem`, or replace the
failed copy with an unguarded pointer dereference.

The dedicated opt-in `test-frame-vma-flags` stage is separate and does **not**
call `process_vm_readv` or read frame content. It gathers `VmFlags` from
`/proc/self/smaps` for the live, validated descriptor mapping ranges and
only reports booleans for `io` (VM_IO), `pf` (VM_PFNMAP), `mm` (mixed
mapping), `dd` (do not dump), nonzero RSS and the number of VMA segments.
It logs neither mapped file names nor addresses, and runs the previously
tested guarded shutdown sequence on all return paths.

The command still acquires capture resources. Only run with explicit consent
and a known-clear local source, using the existing narrow executable-specific
LS2 role. Fail closed if `smaps` or any `VmFlags` are unavailable. No DRM
or HDCP bypass, privilege escalation, native capture mode fallback, or pixel
access should be attempted.

Passing a `VmFlags` inspection is **not** authorization for raw frame
dereferencing, and does not prove that `process_vm_readv` will succeed.
