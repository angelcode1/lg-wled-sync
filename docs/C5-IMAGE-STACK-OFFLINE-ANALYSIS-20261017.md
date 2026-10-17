# LG C5 image stack: offline evidence review (2026-10-17)

Evidence: privately supplied `c5-image-stack-20261017-223113.zip` (61 MB). The archive contains selected native TV files and `runtime.txt`; it does not contain /etc service roles, owned frame pixels, or hardware performance measurements. Do **not** check vendor libraries or private process maps into this repository.

## Observed platform

- Kernel: Linux 5.4.268-324, aarch64 (2024-09-30 build); userspace includes **ELF32 ARM EABI5** libraries. Runtime `getconf GNU_LIBC_VERSION` reports **glibc 2.35**.
- `/dev/video60` is named `vt-capture-dev` and owned `root:composit` with `0660` permissions.
- `/dev/video70` is named `gps-capture-dev`, also `root:composit` with `0660`. This name alone does **not** establish a safe or usable capture capability. No direct `/dev/video70` consumer is identified by the uploaded capture-specific ELF dependencies; the generic GStreamer v4l2 plugin contains a reference.
- `/dev/video240` is present as `dolby-metadata-dev`, which does not imply any raw-image access.
- Live process maps include `/usr/lib/libvtcapture.so.1.0.0`, `/usr/lib/libhalgal.so.2.0.1`, `/usr/lib/libgm-wayland.so.1.0.0`, `/usr/sbin/captureservice`, and the surface manager / compositor. The aggregated maps do not identify PID-to-library ownership; a per-PID attribution needs separate read-only collection.

## Capture paths recovered

| Component | Native evidence | Relevance and limitation |
|---|---|---|
| Video VT | `/usr/lib/libvtcapture.so.1.0.0`, Build ID `9fb5e56d59c29ed242c0e4a66015c71601a4657e`, SHA256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`; references `/dev/video60`, exports `vtCapture_currentCaptureBuffInfo` | **Exactly the previously guarded vendor build**. Its `VtHistogram::initHistogram()` Thumb function is 4 bytes (`07 20 70 47`): returns constant 7. No new safe CPU buffer lease or histogram function found. |
| Legacy DILE VT | `/usr/lib/libdile_vt.so.0.1.0` exports 4/6-byte stub-like `DILE_VT_*` functions | No proof of a functioning legacy video capture path. Do not enable fallback merely because the file exists. |
| LG capture service | `/usr/sbin/captureservice` imports `HAL_GAL_CaptureFrameBuffer`, links `libvtcapture`, contains `VideoCapture`, `GraphicCapture`, `DisplayCapture`, `blendedCapture`, `execute`, `executeOneShot`, `stop`, `getCapability`; registers `com.webos.service.capture` and a separate permission service | Highest-value alternative **investigation**: LG-managed, permission-checked capture with PNG/JPEG support. No proof of authorized high-rate, full-content, or HDR capture. |
| Built-in compositor screenshots | `/usr/lib/qml/WebOSCompositor/controllers/fullscreen/StarfishFullscreenCapture.qml` builds a request using `method: BLENDED` and calls `com.webos.service.capture/executeOneShot` with output path/format/width/height | **Proves first-party screenshot use of the capture service**. It does not establish that a third-party app is entitled to the same method, that video is included, or that continuous screenshot streaming is viable. |
| UI framebuffer | `/usr/lib/libhalgal.so.2.0.1` exports `HAL_GAL_CaptureFrameBuffer`, `HAL_GAL_GetFrameBufferList`, `HAL_GAL_CaptureFrameBufferEx`; capture service links this library | Candidate separate UI/OSD source when authorized, independently from VT video frames. Requires ABI and content coverage validation. |
| Legacy UI stack | `libgm-wayland.so.1.0.0` exports `GM_*Window`, `GM_GetWindowSurfaceInfo`, bitmap utility | Visible UI surface management; no evidence that it provides unprotected TV video pixels. |
| Qt/Wayland compositor | `libWebOSCoreCompositor.so.0.0.1` has `WebOSScreenShot::take`, `QQuickWindow::grabWindow`, `glReadPixels`; compositor plugin has `StarfishCompositor::prepareScreenCapture` | Screenshots/individual compositor surfaces, not automatically final post-video display frames. Requires cautious read-only entitlement inspection before any integration. |
| Vendor test suite | `/usr/bin/vtCaptureTestSuite` imports VT and HAL_GAL; embedded menu includes one-shot/continuous capture, RGB conversion and histogram commands | **Do not run**: these commands can open capture devices, affect V4L2 ownership, or exercise known-stubbed histogram code. It is useful for offline ABI/string review only. |

The binaries include vendor source-build path strings under `lib32-screen-capture-webos`; this gives class names and operation names but does **not** provide source code or validated ABI.

## Rust implementation priorities

1. **Keep C5 VT guards.** The SHA256 exactly matches the known frame/histogram blocked build. Preserve unknown/protected fail-closed routing, no `DILE_VT` fallback, no unsafe `currentCaptureBuffInfo` CPU pointer copy, and no second queue owner.
2. **Add a feature-gated capture-service metadata adapter only after permission verification.** Start with read-only `getCapability`/service availability; don't invoke `executeOneShot` or capture from Rust automatically. A screenshot file is not an owned, leased video buffer.
3. **Evaluate an authorized UI/OSD backend independently.** `libhalgal` is a more specific UI-path candidate than speculative direct V4L2 reads. No bypassing protected content; validate frame provenance.
4. **Separate source availability from physical capability.** Distinguish VT video, HAL_GAL UI, screenshot status, known-protected/unknown source, and stream latency in diagnostics.
5. **Measure one-shot cadence / capture coverage only after deliberate TV-side approval** using known-unprotected local UI or own test pattern. Don't claim Ambilight FPS from API availability.
6. Maintain the 32-bit ARM toolchain target. Do not build an ARM64 userspace binary solely because `uname -m` says `aarch64`.

## Minimal next read-only evidence

On a Mac (replace the host):

```sh
TV_HOST=192.168.8.X
ssh "root@$TV_HOST" 'luna-send -n 1 -f luna://com.webos.service.capture/getCapability "{}" 2>&1'
ssh "root@$TV_HOST" 'sh -s' <<'SH'
for p in /proc/[0-9]*/maps; do
  grep -q -E '/usr/lib/libvtcapture|/usr/lib/libhalgal|/usr/sbin/captureservice' "$p" 2>/dev/null || continue
  pid=${p#/proc/}
  pid=${pid%/maps}
  printf 'PID=%s COMM=' "$pid"
  cat "/proc/$pid/comm" 2>/dev/null
  grep -E '/usr/lib/libvtcapture|/usr/lib/libhalgal|/usr/sbin/captureservice' "$p" 2>/dev/null | awk '{print $NF}' | sort -u
done
SH
```

A capture service permission rejection is a valid result and must not be worked around by widening roles. The archive does not contain the LS2 service manifests and roles needed to assert entitlement. Record the denied response without mutation.

**No existing binary, service, permissions, capture session, or TV preferences were modified by this offline review.**

## Runtime `getCapability` reply and binary verification

TV response from `luna://com.webos.service.capture/getCapability` with `{}`: `{"returnValue":true}`, with no capability fields. This confirms an accepted response, **not** a successful screenshot, authorized video pixel access, a valid capture path, or a supported real-time mode.

The shipped `/usr/sbin/captureservice` ELF has dynamic symbol `CaptureProperties::processGetCapability(LSHandle*, LSMessage*, void*)` at Thumb address `0x1d65d`, size **4 bytes**. Offline Thumb disassembly of the same user-provided ELF shows a branch at `0x1d65c` directly to `CaptureProperties::processGetClipRegions` at `0x1d530`. The `processGetProperties`, `processGetOptions`, and several related read-only handlers also branch into this same implementation. Therefore an empty success response is not surprising, and repeatedly calling `getCapability` cannot establish what images are actually available.

The ELF contains explicit `permissionClient` and `forceCaptureClient` parsers and references `/etc/palm/whitelist-conf.json` and `/home/root/whitelist-conf.json`. Inspect the existing files **read-only**; do not add clients or override roles. Avoid sharing unrelated configuration or device identifiers from these files.
