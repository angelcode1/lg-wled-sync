# LG C5 histogram API: read-only ELF and LS2 audit (2026-10-18)

**Scope:** offline examination of the owner's `c5-histogram-readonly.qVZTzZ.zip`.
The archive has SHA-256
`dedcdb098d114dd6ddeb82178a6cb8d2e372953b9b8c1be41133f8053153c33a`;
all **119 downloaded files** matched the included SHA-256 receipt.
The path inventory selected 133 entries; 14 were uncollected
shared-library **symlink names**, while corresponding versioned
targets were downloaded. Files are 32-bit ARM ELF binaries.
No TV-side API was invoked during this offline review.

**This document records metadata/function analysis only. Do not check
proprietary LG binaries, keys, device IDs or other parts of this
collection into this repository.**

## Confirmed observations

### 1. C5 `libvtcapture` histogram ABI is present but stubbed

`/usr/lib/libvtcapture.so.1.0.0` SHA-256:
`6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`.
Build ID: `9fb5e56d59c29ed242c0e4a66015c71601a4657e`.
This matches the existing C5 stub fingerprint used by
`src/capture/histogram.rs`.

ARM Thumb offline disassembly at file virtual address `0x12c18`:
```text
VtHistogram::initHistogram():
  movs r0, #7
  bx   lr
```
This unconditional non-success return is decisive: a new permission
grant or calling order cannot make this exact function initialize a
hardware histogram.

`VtHistogram::getHistogram(_LibVtHistogram*)` at `0x12fe0`
has rejection/logging paths returning status `1` or `2`; no
sample-buffer population was found in the inspected function.
The `vtCapture_initHistogram`, `vtCapture_getHistogram`,
`vtCapture_setPresetHistogram`, and
`vtCapture_finalizeHistogram` C ABI symbols are exported, but
exports do not establish that the implementation works.

The vendor `/usr/bin/vtCaptureTestSuite` imports these four C
functions and has associated `_vtHistogram_*` wrappers.
**Do not execute this vendor test suite** merely to test a stub;
it can activate capture hardware.

### 2. `lightingcontroller` is a frame-based built-in LED consumer

`/usr/sbin/lightingcontroller` directly needs
`libvtcapture.so.1` and contains `lightingSyncMode`,
`videoSyncModeCondition`, `_vtCaptureInitialize`,
`_vtCaptureProcess`, `_vtCaptureCurrentCaptureBuffInfo`
and related native frame functions.

Its imports include `vtCapture_create`, `vtCapture_initEx`,
`vtCapture_preprocess`, `vtCapture_process`,
`vtCapture_currentCaptureBuffInfo`, `vtCapture_planeInfo`,
and teardown functions. **It does not import
`vtCapture_getHistogram` or the other histogram API symbols.**

The service's on-disk
`com.webos.service.lightingcontroller.api.json`
exposes only four ambient/gaming LED microcontroller version and
firmware-update methods. It exposes **no confirmed LS2 method
for reading 16 regional RGB values or protected-content histograms**.
The built-in video-sync mode is an interesting reference for TV
model-specific lighting, but whether it functions on this C5 and
whether it can report per-region colors are unverified.
Its role lists capture-service client names; this is not authorization
for a third-party app to impersonate those names.

### 3. `livepick` histogram is software image analysis

`/usr/sbin/livepick` exports `GetColorHistogram` and
`RegisterHISTOGRAMImage`, together with ImageMagick functions
such as `MagickMalloc`, `WriteHISTOGRAMImage` and
`GetColorTuple`. Offline disassembly of
`GetColorHistogram` shows calls into ImageMagick's color histogram
code, so this is not evidence of a display-pipeline or
hardware-protected histogram source.

The collected `com.webos.service.livepick` interface and
`*.api.json` enumerate scene/content/recognition and control
methods, but no returned regional screen-color histogram method.

### 4. Kernel-adaptor histogram strings are not a usable API contract

`/usr/lib/libkadaptor.so.2.0.1` has diagnostic strings including
`HST [0:Off,1:On,2:Fmt,3:HISTOGRAM,...]` and
`dump raw histogram`. It also exports many KADP graphics,
backlight, HDR and capture-related symbols, including
`KADP_HDR_SetDolbyAmbientLight`.

**No explicit exported `KADP_*Histogram`/public LS2
per-region RGB histogram provider was established** in this
bundle. These diagnostic strings do not prove a stable or
authorized read interface. They may refer to hardware diagnostics,
not a service API usable by our application; avoid enabling or
modifying protected capture settings.

### 5. The LG screenshot and compositor services are separate

The collected
`com.webos.service.capture.api.json` lists client methods such
as `createHandle`, `setProperties`, `setOptions`,
`setOutput`, `execute`, `executeOneShot`, and
`getClipRegions`. It provides no explicit histogram-result
method. The compositor's
`StarfishFullscreenCapture.qml` uses
`executeOneShot` for BLENDED screenshots, matching the
Stage 4B/5 work, but this is **not** a
protected-content histogram mechanism.

The live inventory identifies
`/dev/video60` as `vt-capture-dev`,
`/dev/video70` as `gps-capture-dev`,
and additional media V4L2 nodes. This collection only read
device names; no V4L2 device was opened.

## Summary matrix

| Candidate | Offline verdict | Appropriate next action |
| --- | --- | --- |
| `libvtcapture` `VtHistogram` | **Confirmed stub** on this exact C5 | Do not call or try privilege changes |
| `vtCaptureTestSuite` | References stubbed histogram functions | Inspect offline only |
| `lightingcontroller` `videoSyncMode` | Genuine *frame* capture consumer; not proven histogram | Inspect exported internal control flow and model enable conditions offline |
| `livepick` `GetColorHistogram` | ImageMagick image analysis | Deprioritize for screen-region capture |
| `libkadaptor` HST debug strings | Diagnostic hint, no authorized interface established | Optional source/ELF reference research; no hardware activation |
| `captureservice`/`WebOSCompositor` | Proven screenshot path, not histogram | Keep current clear-only permissions and controls |
| `lightingcontroller` LS2 methods | Microcontroller firmware/version operations only | Not a source of screen-color histogram samples |

**Current operational status:** the repository's DRM-first
`DrmCaptureRouter` retains `Protected → Histogram` when a
verified healthy histogram backend exists, otherwise
`Protected → Hold/black`. On this C5 the specific vendor histogram
is fingerprinted and refused before initialization; the
Stage 4B/5 clear-content screenshot experiments did not alter this.
No authorized protected-content aggregate-color source has yet
been proven on this C5.

## Optional second-pass evidence (strictly read-only)

The existing collector already includes all binaries necessary
to establish the above findings. A later second pass could gather
**only** the specific additional service/library ABI descriptors
referenced by `lightingcontroller`, e.g.
`libaudio-adaptation-layer-soundengine.so.1`,
`libdile_i2c.so.1`, `libdile_gpio.so.1`,
`libdile_wav22.so.1`, and
`libluna-service2++.so.3`, if symbol dependency mapping
requires it. Do not gather private logs, captures, keys, or
other users' content.

No captures, test-suite executions, new ACGs, vendor-role edits,
device-driver accesses, or service restarts are approved by
this offline audit. Research vendor-supported aggregate/color
interfaces independently of clear-content screenshots.
