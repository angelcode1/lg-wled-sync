# LG C5: v4l2_vt platform driver packaging / source provenance

## What the owner actually observed, 2026-10-13

On `OLED42C5PSA.AAUQLJD` running `Linux 5.4.268-324`,
`aarch64`, the verified `/dev/video60` node is the
`v4l2_vt` platform driver (`81:7`, `vt-capture-dev`).
A known-good Clear-gated native probe successfully passed through
`video_ioctl2` and initiated a very brief V4L2 MMAP stream; see
[LG_C5_VIDEO60_IOCTL_TRACE_2026-10-13.md](LG_C5_VIDEO60_IOCTL_TRACE_2026-10-13.md).

The user's **read-only** driver inventory returned:

- `/sys/bus/platform/drivers/v4l2_vt/module` — **not present**
- `/sys/module/v4l2_vt` — **not present**
- `/proc/modules` — no `v4l2_vt` entry
- `/lib/modules/5.4.268-324` — directory existed but
  no matching `*vt*.ko*` was found

This is **strong evidence of a built-in platform driver**, but not
a source code or symbol-level identification. A built-in driver does
not have a loadable `.ko` by definition. Conversely, absence from
the checked module filename glob alone proves little (module could
have a different name). Sysfs module-link absence together with a
live bound platform driver is the stronger evidence.

## Safe final provenance collector

`scripts/c5_v4l2_builtin_provenance.sh` inspects the exact known
sysfs driver and device identity, then *reads only*:

- platform driver, device and optional `of_node` symlinks;
- `/proc/config.gz` or the kernel-matched config if exposed
  (report whether any relevant driver `CONFIG_...=y` flag exists);
- matching `/proc/kallsyms` **symbol names only**, with
  addresses/pointers intentionally suppressed.

Missing `/proc/config.gz`, build config, or symbol matches are
ordinary negative findings, not reasons to remount filesystems,
change `kptr_restrict`, enable debugfs, or read kernel memory.
On a stripped vendor kernel the build option and function names
may not be present even for a built-in driver.

The collector never opens the video node, issues ioctls, loads modules,
changes sysfs state, writes to `/dev/kmsg`, or reads video frames.
Existing DRM restrictions and native permission refusals remain
fully enforced.

## Upstream source search

LG publishes licensed source archives through its official
[LG Open Source product portal](https://opensource.lge.com/product)
and accepts specific source-code inquiries at
[LG Open Source inquiry](https://opensource.lge.com/inquiry).

**No matching, verified C5 `v4l2_vt` implementation has been
located yet.** Kernel version alone is insufficient to identify the
exact build. When obtaining an archive, match `OLED42C5PSA`,
the build's `5.4.268-324` identity, the reported platform
device `v4l2_vt`, and ideally the actual firmware software version.
A GPL kernel release may omit any separately licensed/proprietary
capture driver; verify contents instead of assuming inclusion.
Do not publish proprietary vendor ELF or a TV firmware dump.

## Engineering decision

No new V4L2 `REQBUFS` or `STREAMON` tests are needed to establish
the standard API sequence. Current risk remains **driver buffer
ownership/coherency**: LG's native worker dequeues, publishes and
requeues each frame without giving us a consumer-held buffer lease.
Continue static review of lawful source, driver symbols, or the exact
vendor ELF. The C5 raw-pixel path stays fingerprint-blocked until a
supported, authorization-preserving buffer ownership mechanism is
established. Only independently verified Clear media can ever pass
the existing active-capture gate.
