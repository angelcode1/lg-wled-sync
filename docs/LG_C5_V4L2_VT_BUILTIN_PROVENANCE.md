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

## Second live read-only provenance result: actual kernel symbol inventory

User ran `scripts/c5_v4l2_builtin_provenance.sh` on
2026-10-13. It completed with zero device opens, zero ioctls,
zero pixel reads and zero kernel mutations.

- `kernel=5.4.268-324`, `architecture=aarch64`
- bound `/sys/bus/platform/drivers/v4l2_vt` exists
- `/sys/bus/platform/devices/v4l2_vt` points to
  `../../../devices/platform/v4l2_vt`
- neither `/sys/bus/platform/drivers/v4l2_vt/module` nor
  `/sys/module/v4l2_vt` exists
- `/sys/bus/platform/devices/v4l2_vt/of_node` absent
- `KERNEL_CONFIG_NOT_EXPOSED` — no readable kernel
  configuration was found by the collector
- `/proc/kallsyms` **exposes named driver functions**; no kernel
  addresses were logged or retained

Strong combined evidence: `v4l2_vt` is built into this
kernel rather than loaded as a `.ko`. This is a live symbol-level
attribution, **not yet a matching-source identification**.

### Captured function names, grouped by engineering question

| Domain | Observed kernel symbol names | Engineering relevance |
|---|---|---|
| Registration/lifecycle | `v4l2_vt_register`, `v4l2_vt_probe`, `v4l2_vt_remove`, `v4l2_vt_fh_open`, `v4l2_vt_fh_release` | Built-in platform driver and file-handle lifecycle |
| Power | `v4l2_vt_resume`, `v4l2_vt_suspend`, `v4l2_vt_runtime_resume`, `v4l2_vt_runtime_suspend`, `v4l2_vt_pm_runtime_test` | Power domain state may affect capture readiness |
| Format | `v4l2_vt_vidioc_g_fmt_vid_cap`, `v4l2_vt_vidioc_g_fmt_vid_cap_mplane`, `v4l2_vt_vidioc_s_fmt_vid_cap`, `v4l2_vt_vidioc_s_fmt_vid_cap_mplane` | Both single- and multi-planar handler *symbols* exist; only multi-planar G_FMT/S_FMT were demonstrated on this video node |
| Queue and readiness | `v4l2_vt_vidioc_reqbufs`, `v4l2_vt_vidioc_dqbuf`, `v4l2_vt_poll`, `v4l2_vt_vidioc_expbuf` | Buffer allocation/dequeue, readiness, and potential DMABUF export path. **Export does not grant consumer ownership** |
| Controls | `v4l2_vt_vidioc_querycap`, `v4l2_vt_vidioc_g_ctrl`, `v4l2_vt_vidioc_s_ctrl`, `v4l2_vt_vidioc_g_ext_ctrls`, `v4l2_vt_vidioc_s_ext_ctrls` | Confirms the vendor-specific control IDs seen on video60 are handled by the built-in driver |
| Events | `v4l2_vt_vidioc_subscribe_event`, `v4l2_vt_vidioc_unsubscribe_event` | Event subscription functionality exists in code, exact event ABI unknown |
| Capture worker | `vt_capture_thread`, `vt_capture_notification` | A **kernel-side** capture worker and notification function exist, separate from LG library's **userspace** DQBUF/publish/QBUF worker |

**Presence in `kallsyms` does not guarantee an operation is
registered in the device's `v4l2_ioctl_ops` table.** The earlier
independently observed success of a correctly sized ARM32
`VIDIOC_G_FMT=0xC0CC5604` proves that the multi-planar G_FMT
entry point can be reached, but returned all-zero *idle* geometry.
`VIDIOC_ENUM_FMT` returned ENOTTY at index zero because the
driver registered no enum-format operation. The V4L2 core
adds `EXT_PIX_FORMAT` to QUERYCAP; it does not originate in
this LG driver.

The `v4l2_vt_vidioc_expbuf` **symbol is a new offline
investigation lead, not confirmation of a usable DMABUF FD**:
the ioctl may be unregistered for this node, may require its
own queue, and an exported DMA buffer by itself does not suspend
kernel capture writes or prevent the userspace thread from
immediately requeueing a buffer. No EXPBUF or buffer-access probe
is authorized by this result.

An older, **different-model** webOS 8.3 / C3 community log
contains `vt_capture_thread` and `v4l2_vt_vidioc_reqbufs`
kernel trace lines, including a five-buffer queue-setup result
and a kernel-side "Index ... locked" message. This is useful
historical context, **not** evidence that the C5 implements
the same locking or cache synchronization:
https://vuplus-support.org/wbb4/index.php?thread/146826-ambilight-f%C3%BCr-lg-smart-tv-mit-webos-piccap-hyperion-keine-extra-kosten-nur-tv-un/&pageNo=94

### Remaining unanswered questions; static source review only

1. Inspect `v4l2_vt_vidioc_reqbufs`, `v4l2_vt_vidioc_dqbuf`,
   and `vt_capture_thread` for index ownership, queue minimum,
   locking, DMA completion, cache invalidation and conditions
   for releasing buffers.
2. Inspect `v4l2_vt_vidioc_expbuf` and the registered
   `v4l2_ioctl_ops` table to determine whether the code
   supports export, under what conditions, and with what
   lifetime semantics. **Do not try exporting or mapping until
   those are established.**
3. Audit `vt_capture_notification` for whether it signals
   completion to userspace or is merely a driver-internal
   notification; no consumer callback contract is known.
4. Compare any lawfully obtained LG C5 kernel sources with
   `5.4.268-324`, the platform name `v4l2_vt`, and the
   symbol inventory before drawing conclusions. **The
   vanilla upstream Linux 5.4.268 tree is not a substitute
   for LG's vendor-modified kernel.**

Public source route: LG's official
[product downloads](https://opensource.lge.com/product)
and [source inquiry](https://opensource.lge.com/inquiry).
Exact model `OLED42C5PSA.AAUQLJD`, platform
`v4l2_vt`, Linux `5.4.268-324`. No verified matching
vendor source archive was found through public searches.
The TV's board/SoC identifier and firmware software version
would help match a model-specific LG source package.

### Next non-invasive source-identification step

Read only the board family / SoC identity, without serial
numbers, MACs, unique device IDs, kernel pointers or changes:

```sh
ssh -T root@192.168.8.184 'sh -s' <<'TV'
set -eu
echo "===== LG C5 SOURCE MATCH IDENTITY ====="
uname -r
if command -v nyx-cmd >/dev/null 2>&1 && command -v timeout >/dev/null 2>&1; then
    timeout 6 nyx-cmd DeviceInfo query device_name || true
else
    echo "NYX_DEVICE_NAME_UNAVAILABLE"
fi
if [ -r /proc/device-tree/compatible ]; then
    echo "===== DEVICE TREE COMPATIBLE ====="
    tr '\000' '\n' < /proc/device-tree/compatible
else
    echo "DEVICE_TREE_COMPATIBLE_UNAVAILABLE"
fi
if [ -r /sys/devices/soc0/soc_id ]; then
    echo "===== SOC ID ====="
    cat /sys/devices/soc0/soc_id
fi
echo "C5_SOURCE_MATCH_IDENTITY_COMPLETE"
TV
```

Continue avoiding driver state changes, new V4L2 ownership,
direct PFNMMAP reads, `/dev/forcecapture`, developer marker
changes, or any protected/Unknown content.


## 2026-10-13: actual C5 platform identity confirmed

A subsequent **read-only** query returned:

```text
kernel=5.4.268-324
nyx-cmd DeviceInfo query device_name: o22n3
/proc/device-tree/compatible:
  lge,lg1213
  lge,lg1k
/sys/devices/soc0/soc_id: unavailable
```

This materially narrows the hardware lineage:

- **O22N3** is LG's **LX-family 2025 C5** hardware platform;
  the community `throwaway96/webos-kernel-drivers` source-identification
  guide lists it among LX platform codes:
  https://github.com/throwaway96/webos-kernel-drivers
- A published independent OLED65C5 review likewise names **O22N3**
  as the hardware platform:
  https://geex.x-kom.pl/recenzje/recenzja-lg-oled65c5/
- The publicly documented 2025 O22N3 firmware / OTA platform
  identifier is **`HE_DTV_W25G_AFABATAA`**, corresponding to
  the `W25G` family. A C5-family firmware extraction note supplies
  a cross-check, **not an exact-match C5 kernel source**:
  https://github.com/openlgtv/epk2extract/pull/143/files
- `lge,lg1213` and `lge,lg1k` are device-tree compatible family
  strings, **not model-unique identifiers**. Earlier O22
  televisions also use LG1213; a source archive identified only by
  that string must not be assumed compatible with O22N3 or the
  deployed `5.4.268-324` kernel.
- No `soc_id` is exposed in the checked path. **Do not invent or
  infer a numeric SoC identifier**.

### Source archive selection and provenance order

1. Exact TV model **`OLED42C5PSA.AAUQLJD`** /
   **`OLED42C5PSA`**, using the official LG TV open-source catalog:
   https://opensource.lge.com/product/list?ctgr=005&subCtgr=006
2. Board **`O22N3`** / firmware family
   **`HE_DTV_W25G_AFABATAA`**.
3. Running **`5.4.268-324` ARM64** kernel and its specific
   manufacturing firmware release and build metadata.
4. Presence of the actual vendor source for **`v4l2_vt`**,
   especially `v4l2_vt_vidioc_dqbuf`,
   `v4l2_vt_vidioc_expbuf`, `vt_capture_thread`,
   `vt_capture_notification`, and its V4L2 ioctl ops table.

**Source status:** Public search established this board family and
its firmware naming, but **no model-and-build-verified LG kernel
source archive containing `v4l2_vt` has yet been obtained**.
LG's product catalog is dynamically populated, so the absence of
a search-indexed item is not proof that an archive isn't listed.
A matching GPL archive may also exclude separately licensed vendor
driver files. Do not represent a general Linux 5.4 tree, C2 O22,
or C4 O22N2 source package as a C5 driver match.

LG's official source inquiry:
https://opensource.lge.com/inquiry

For a precise request, specify
`OLED42C5PSA.AAUQLJD`, `O22N3`,
`HE_DTV_W25G_AFABATAA`, `Linux 5.4.268-324`,
and ask which released source package includes the bound
`v4l2_vt` driver and its V4L2 buffer handlers.

### Last missing safe matching detail: firmware release

Do **not** dump all of `device_info.json` or `os_info.json`:
those files can contain serial numbers, unique identifiers,
hardware/network addresses and other unnecessary data.

Instead print only allowlisted non-unique version fields:

```sh
ssh -T root@192.168.8.184 'sh -s' <<'TV'
set -eu
echo "===== C5 SOURCE PACKAGE VERSION ====="
if [ -r /var/run/nyx/os_info.json ]; then
    grep -E '"(core_os_kernel_version|webos_manufacturing_version|webos_build_id|webos_release|webos_release_platformcode)"[[:space:]]*:' /var/run/nyx/os_info.json || true
else
    echo "NYX_OS_INFO_UNAVAILABLE"
fi
echo "C5_SOURCE_PACKAGE_VERSION_COMPLETE"
TV
```

This reads version metadata only, no video device, no kernel
memory, no vendor marker, and no identifying secrets.
