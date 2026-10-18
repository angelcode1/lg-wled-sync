# LG C5 Stage 9 — offline low-resolution colour and capture decision gate

**Status:** offline source/evidence analysis and synthetic 64×48 test. No TV command, LS2 permission, screenshot, V4L2 ioctl, native buffer dereference, vendor service execution, proprietary binary distribution, WLED/Hue output, production code change or device configuration change. This stage is independent of the two completed owner-controlled Stage 7 clear captures. The C5's original Stage 1 receipt-verified installation remains the observed baseline.

## Evidence and provenance (do not collapse evidence levels)

1. **Owner's measured clear-content JPEG paths:** Stage 7 produced two pairs of fully decoded 320×180 JPEGs. Native request times were 161 and 151 ms; one-shot times were 160 and 161 ms. The native handle was destroyed each time; the original Stage 1 roles/executable were restored with matching hashes. Host wrapper's heredoc `SSH_PID` bug was subsequently fixed and verified by offline scope and syntax checks; *the corrected `wait` was not rerun end-to-end*. The collected comparisons have **different quantization tables, no independently confirmed colour reference, no source-plane attestation, and no sustained-FPS measurement**.
2. **Existing production clear-only fallback:** `src/capture/blended.rs` uses LG `executeOneShot` with **PNG**, not the Stage 7 JPEG encoder. It invokes a bounded Luna client, validates and decodes one PNG, and feeds the common `ColorPipeline` 32×18 colour field. Stage 7 JPEG timings **must not** be portrayed as a production-PNG benchmark.
3. **Owner-supplied vendor binary static evidence:** the C5 `/usr/sbin/lightingcontroller` imports `libvtcapture.so.1`, obtains frame pointers, and uses `csc_YUV420toRGB` plus `kMeansClustering` (`13` appears as a clustering-call argument). The observed function enforces two plane lengths **0xc00/3072** and **0x600/1536**. These are arithmetically consistent with 64×48 YUV420 two-plane samples. They **do not prove NV12 vs NV21, chroma stride, primaries/range, a safe buffer lease, a usable third-party LS2 identity, or current-model runtime access**. Static symbol names do not authorize invoking that vendor pipeline.
4. **C5 VT ownership hazards remain:** the fingerprinted `libvtcapture` code requeues immediately after publishing a buffer index and supplies pointers without a consumer-held dequeue/return lease. Never treat a byte-count match or a pointer snapshot as permission to read a live video plane. C5 histogram initialization remains the known stub; do not infer protected-content support or remove the existing guarded fingerprint.
5. **Compositor/HAL evidence:** first-party `StarfishFullscreenCapture.qml` calls the *same* LG BLENDED one-shot capture service; `/usr/lib/libhalgal.so.2.0.1` exports framebuffer capture operations, which are UI/OSD investigation leads, **not** proof of whole-video, authorised, continuous, HDR, or fast capture. No new LS2 roles, factory identity, `forceCaptureClient`, or `capturepermission.security` methods are justified.

## Stage 9 executable test

`scripts/c5_stage9_yuv420_perimeter.py` uses an **independently implemented** 8-bit, tightly packed, 64×48, two-plane *assumed* NV12/BT.601 studio-range model. It does not link or execute LG code. It constructs synthetic four-chromatic-edge frames in memory, encodes 3072 Y and 1536 UV bytes, decodes them with explicit assumptions and compares centre-edge RGB averages against the original palette. There is no screenshot or saved image.

The synthetic assertions verify:

- Exactly **4,608 input bytes**; rejects truncated, extra, wrong-type or otherwise incompatible raw buffers.
- Four distinct reference edge colours with maximum nominal RGB error **at most 5** under the synthetic model, avoiding overlapping corners.
- Rotated frame is detected (at least 100-channel reference mismatch); NV21 chroma-order interpretation is detected as incorrect (at least 60-channel mismatch).
- A truly black reference is valid and decodes to nearly black; an image with no independently confirmed colour reference is **INCONCLUSIVE**, not automatically a passing colour-fidelity result.
- Rotating an image leaves its global mean unchanged while swapping spatial edges: a vendor-style *global* colour clustering output cannot by itself represent four independent Hue/WLED perimeter locations.
- The script makes no LS2 request, raw pointer dereference, hardware timing measurement or light connection.

Run from a checkout with Python 3.9 or newer:

```sh
python3 -m py_compile scripts/c5_stage9_yuv420_perimeter.py
python3 scripts/c5_stage9_yuv420_perimeter.py --selftest
```

Expected prefix:

```text
C5_STAGE9_NV12_SPATIAL_SELFTEST_PASS geometry=64x48 planes=3072+1536
```

**Optional and strictly offline:** if an already-existing **regular** 4,608-byte NV12 sample was obtained through independently authorized means, and the operator wants conditional numerical statistics, run:

```sh
python3 scripts/c5_stage9_yuv420_perimeter.py \
  --existing-nv12 ./already-owned-clear-64x48.nv12 --assume-bt601-limited
```

The output is explicitly `INCONCLUSIVE_UNVERIFIED_FORMAT_AND_SOURCE` regardless of how plausible its means look. An exact byte count is not proof of the colour matrix, frame plane or DRM status. The program refuses symlinks, special files and all file sizes other than 4,608 bytes, and does not write raw pixels or accept device paths. Do **not** manufacture a sample by reading live C5 vendor pointers.

## Architecture and latency decision

| Candidate | Evidence | Quality/permission/throughput limitation | Disposition |
| --- | --- | --- | --- |
| Existing `executeOneShot` PNG | Source code and earlier clear UI/video outputs | No production PNG full-loop p95/p99 benchmark; 1 process per request | Keep opt-in, unchanged |
| Stage 7 reusable LS2 JPEG handle | Two real owner-approved bounded calls; 151/161 ms request | Different encoder settings; unknown quality equivalence; no persistent-stream throughput proof | Experimental, do not promote |
| 64×48 YUV420 colour processing | Vendor static 3072+1536-byte buffer and conversion path; synthetic independent model | No safely leased data for this C5, assumed NV12/matrix; resolution does not improve capture RPC by itself | Offline algorithms only |
| HAL_GAL/UI/compositor framebuffer | Binary exports and first-party screenshot call path | UI-only vs video coverage, permission, frame ownership and cadence unknown | Read-only research lead |
| C5 direct VT pointers/`/dev/video60` | Verified immediate requeue and non-leased pointer snapshot | Data-race/ownership hazard, exact-build guard | **Rejected for live CPU reads** |
| Protected-content histogram/aggregate | Desired architectural outcome, but known C5 histogram stub | No verified authorized functioning data provider | **Unavailable on this build; hold/black** |

**Request-time bound is not display FPS.** For the two Stage 7 native JPEG calls, single-threaded strict sequential request-only ceilings are `1000/151 ≈ 6.62` and `1000/161 ≈ 6.21` per second. Those are optimistic upper bounds assuming zero scheduling, classification, file, JPEG decoding, processing and Hue/WLED transport cost. No 30/60 fps claim follows. At 30 fps, the complete sampling cadence must average 33.3 ms or better *with validated frame freshness*; at 60 fps, 16.7 ms. Reducing a 320×180 RGBA image (230,400 decoded bytes) to a nominal 64×48 NV12 sample (4,608 bytes) shrinks *pixel representation size* by a factor of 50 but does **not** eliminate a 150 ms capture RPC or fix buffer ownership.

If a legitimately supported fast path is ever found, retain an independently verified `Clear` media classification before capture and on every subsequent frame; reject Protected/Unknown/HDMI without fallback. Acquire exclusive/legitimate frame ownership; establish monotonic capture timestamps, source and geometry; validate format and reference colour; convert at most once to the common 32×18 colour field; drop stale frames (never queue unbounded); and preserve existing per-vendor mapping plus black/hold safety. Do not let a successful screenshot silently override the DRM-first state machine.

## Follow-up work permitted without a new TV experiment

1. Review existing offline ELF/API documentation and public interfaces for an authorised *read-only* capture metadata/provenance path; rank proof of client identity, content-plane coverage and lease safety before rate claims. Do not execute captures or dangerous vendor tools.
2. Add detached host-only timing simulations/queue-aging tests using **synthetic, clearly labelled** request latencies and frame freshness, without claiming hardware results.
3. Add Rust-side unit tests for stale/unknown/protected fail-closed behaviour only if they can be isolated from production routing changes; do not tune live output from synthetic data.
4. A future owner-approved device benchmark, if needed, must separately establish known-clear source identity, codec/matrix equivalence, exact process permission, clean teardown, baseline restoration, p50/p95/p99 end-to-end capture age and CPU. No device test is authorized by this document.

**Promotion gate:** independent bright reference and orientation accuracy, source provenance, valid pixel ownership, verified permission, sustainable freshness and CPU use, and owner approval. Until then preserve the Stage 1 state and production DRM routing.

## Public-interface cross-check (2026-10-19; no device test)

The official **webOS OSE** `com.webos.surfacemanager/captureCompositorOutput`
reference labels its ACG `surfacemanager.devutility` and explicitly says
the utility is intended for privileged components, not public use. It
also warns that its screenshot is restricted to **what the surface
manager composes** and *may omit video*. webOS OSE documentation is not
a blanket API/entitlement guarantee for consumer webOS TV firmware.
This strengthens the **UI-only until proven otherwise** classification
for a compositor/HAL candidate; do not create or grant that ACG or
invoke the method on this C5 merely because a public reference exists.

Community `webosbrew/hyperion-webos` documentation likewise treats
`libhalgal`/older `libgm` as **UI** capture sources separate from
`libvtcapture`/older `libdile_vt` **video** sources. Its historical
high-FPS anecdotes are hardware/version-specific and do not supersede
the C5 vendor hash, unsafe immediate-requeue pointer semantics or
owner-approved clear-only LS2 gate. Do not enable Hyperion force-capture
quirks or unsupported source fallbacks to chase headline frame rates.

Public references:

- https://www.webosose.org/docs/reference/ls2-api/com-webos-surfacemanager/
- https://github.com/webosbrew/hyperion-webos/blob/main/README.md

**Decision refinement:** prioritize an authorized, independently validated
**video-capable** source with explicit frame ownership and age over a
compositor API that may offer fast but UI-only pixels. Preserve the
separation of UI and video coverage in every benchmark report.
