# LG C5 Stage 12 — exact capture-source timestamp and frame-lease audit

**Scope:** offline-only source/API evidence and a bounded static scanner for *already owned* native ELF archives. No TV network request, capture, executable vendor code, V4L2 ioctl, image decoding, daemon update, LS2 role edits, WLED/Hue output or DRM experiment. No vendor binaries are committed or included in this kit. The original Stage 1 TV baseline is unchanged.

## New direct evidence: upstream Hyperion wrapper, not a C5 device test

Source reviewed: `webosbrew/hyperion-webos`, commit `00c932f092084b234b3f619b060a98fa134c12bd` (2026-10-19 retrieval, `main` at the time). These links pin the review to a specific source revision:

- [`unicapture/backends/libvtcapture.cpp`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/backends/libvtcapture.cpp#L228-L255): `capture_acquire_frame` uses `vtCapture_currentCaptureBuffInfo`, exposes Y and UV **pointers** and fills a generic frame descriptor. Its `capture_release_frame` body returns 0 with **no native driver buffer return**. The YUV420 format annotation in this upstream code explicitly expresses uncertainty (`ToDo: I guess?!`). Neither a pointer snapshot nor this wrapper-level `release_frame` callback establishes hardware/driver ownership.
- [`libvtcapture.cpp` capture_wait](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/backends/libvtcapture.cpp#L258-L308): detects changes to a current-buffer pointer. Contains an optional **forcecapture quirk**, which we must **not** enable or emulate on the audited C5.
- [`unicapture/backends/libhalgal.c`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/backends/libhalgal.c#L47-L57): comments explicitly warn that HAL_GAL initialization/capture can be bound to an EGL **thread-affine** context; executing capture on the wrong thread may produce `EGL_BAD_ACCESS`. [`capture_acquire_frame`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/backends/libhalgal.c#L121-L143) invokes `HAL_GAL_CaptureFrameBuffer` into a mapped framebuffer, and its wrapper-level `capture_release_frame` is again a no-op. This **does not** prove the HAL_GAL API lacks its own internal synchronization, or that it contains video pixels, or that our service has access.
- [`unicapture/unicapture.h`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/unicapture.h#L22-L54): `frame_info_t` includes format, dimensions, planes/strides and an internal ID, **not a source-frame timestamp**. The generic `capture_release_frame` function type requires a caller callback, but provides no hardware lease contract.
- [`unicapture/unicapture.c`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/unicapture.c#L181-L208): application `getticks_us()` values record time around waiting/acquiring/processing. [`FPS metrics`](https://github.com/webosbrew/hyperion-webos/blob/00c932f092084b234b3f619b060a98fa134c12bd/unicapture/unicapture.c#L365-L379) report application cadence and local elapsed timings, not documented originating content-frame timestamps. **A fast upstream loop is not evidence of independently fresh C5 video frames.**

**Reconciliation with privately supplied exact-C5 evidence:** the earlier C5 native-code audit of exact fingerprint `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c` showed that the vendor VT worker `DQBUF` → publishes index → immediately `QBUF` again, with no caller-held lease. The upstream wrapper's no-op release is consistent with **but does not independently replace** that native worker audit. Therefore **retain the exact-hash frame-path block**, no `process_vm_readv` retries, and no competing `/dev/video60` consumer.

## What an actual source timestamp would require

Official Linux V4L2 buffer documentation specifies that `struct v4l2_buffer` carries timestamp information; `V4L2_BUF_FLAG_TIMESTAMP_MASK` identifies the clock kind (including MONOTONIC), while `V4L2_BUF_FLAG_TSTAMP_SRC_MASK` describes the timestamp source. But **a V4L2 timestamp elsewhere inside LG's worker does not establish one exposed by `vtCapture_currentCaptureBuffInfo`, `executeOneShot`, Hyperion's `frame_info_t`, or our current `BlendedFrame`**. Even a valid timestamp is not an ownership lease or a DRM/media classification.

- https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/buffer.html
- https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-qbuf.html

The observed LG `executeOneShot` reply used by `src/capture/blended.rs` supplies `returnValue`, `capturedWidth`, `capturedHeight` and `writtenBytes`. The Rust-side `finished: Instant` is set **after** the request and decode. Thus it is not a source-sampling timestamp. An ELF symbol referencing `clock_gettime` could simply implement timeouts or tracing. A symbol referencing `release` could clean up a service or destroy a handle rather than return a V4L2 buffer.

## Reproducible host-side checks

Run from this kit using **Python 3.9+** (standard library only):

```sh
python3 scripts/c5_stage12_elf_contract_inventory.py --selftest
```

It refuses malformed/traversal/duplicate relevant ELF entries and symlinks and enforces file/buffer budgets. It does **not** extract or execute any binaries.

To recheck upstream source from an **existing local clone** of the named repository:

```sh
python3 scripts/c5_stage12_elf_contract_inventory.py --hyperion "$HOME/Projects/hyperion-webos"
```

Output fields such as `vt_release_callback_noop=true` and `descriptor_has_no_timestamp=true` are **checks against the supplied local source**, not a device capability result. If upstream has changed, review that actual revision; the script refuses unknown source-contract layouts.

For a previously downloaded, owner-held ELF ZIP containing one or more of `captureservice`, `libvtcapture.so.1.0.0`, `libhalgal.so.2.0.1`, `lightingcontroller`, `livepick`:

```sh
python3 scripts/c5_stage12_elf_contract_inventory.py --archive '/path/to/previously-owned-c5-native-archive.zip'
```

A result of `keyword_presence_only.timestamp_text=true` is a **lead for instruction-level symbol/call-site inspection**, not proof that output pixels have a timestamp. ELF hashing identifies exactly which offline build was examined. All `verified_source_timestamp`, `verified_buffer_lease` and `permission_proven` fields intentionally remain `false`; no static keyword scanner can promote those claims. It prints neither archive member contents nor image pixels and never tries to read the TV.

**Availability limitation:** the previously described proprietary `c5-image-stack-20261017-223113.zip` and `c5-capture-deep-20261017-230452.zip` could not be retrieved from files visible to this conversation's workspace at the time of this report. Therefore **no fresh native ELF inspection is claimed**. Existing older documentation remains the source of exact-C5 native-worker facts; this stage's new direct evidence is the separately pinned open-source Hyperion implementation.

## Decision and next experimental gate

| Interface | Stage 12 confidence | Remaining proof required |
| --- | --- | --- |
| Current C5 VT snapshot | **Blocked** by exact native-worker lease audit | A different, *documented and truly leased* supported interface—not a wrapper callback |
| Hyperion's generic VT `acquire/release` | **Not an actual driver lease** | Verified native acquire/return contract and source timing; upstream callback `release` is no-op |
| Hyperion HAL_GAL framebuffer | **UI research candidate only** | Entitlement, thread-safe ownership/synchronisation, video-plane scope, actual source time |
| Rust clear-only BLENDED PNG | **Experimental, unchanged** | C5 daemon identity permission, independently verified source sampling time or explicitly unknown age, real bright colour reference and throughput |
| Protected aggregate on exact C5 | **Blocked** | Real functioning authorized histogram provider; no fallback to screenshot/video pixels |

**Do not develop another simulated FPS stage.** Next progress should be an operator-approved read-only inspection of a *previously owned* C5 capture archive (if the archive is available) or a genuinely documented interface whose contract can settle frame source time or ownership. There is still no basis for 30–60 fps freshness, exact source-age guarantees, permission widening or device deployment. Production and Stage 1 stay unchanged.
