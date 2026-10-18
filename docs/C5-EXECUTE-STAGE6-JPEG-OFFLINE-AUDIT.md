# C5 Stage 6 — offline JPEG decoding, visual usefulness and quality audit

**State:** The real LG C5's Stage 5 benchmark successfully produced five
native and five one-shot JPEGs, then restored all Stage 1 files and
permissions with `STAGE1_HASHES_PASS`. **No Stage 6 TV request or capture
is authorized or necessary to run the checks documented here.** The
Stage 5 JPEG files were deleted, so their full decode, frame similarity
and compression profile can **not** be retroactively measured.

## What the completed Stage 5 evidence actually establishes

- Five reusable-handle native `execute` JPEGs returned success with
  320×180 SOF geometry, markers and bounded file sizes averaging
  4,641.6 bytes.
- Five separate `executeOneShot` JPEGs returned success with the
  same geometry, averaging 8,877.2 bytes.
- Average request latency: native 151.6 ms; one-shot 162.0 ms.
  Total measured five-frame CPU: native 97.23 ms; one-shot 145.71 ms.
- Overall native phase **took longer** (969 vs 892 ms). The CPU delta
  is exploratory, not a demonstrated throughput or image-quality win.
- Both phases were checking different frames of *playing* video;
  image content was not controlled to an identical frozen scene.
  The difference in JPEG sizes could be caused by scene complexity,
  encoding settings or both. **Do not infer effective JPEG quality
  from file size**.
- The code explicitly set `setOptions {"options":{"jpegQuality":75}}`
  for the reusable handle. The one-shot experiment sent a top-level
  `"jpegQuality":75` field, but **there is no verified C5
  `executeOneShot` acknowledgement that this value was honored**.
  Both paths can report success without reporting encoder parameters.
  Public webOS screenshot examples generally specify path, method,
  format and dimensions, not a confirmed quality-setting contract.
- Production `src/capture/blended.rs` currently uses
  `executeOneShot` with **PNG** and a separate PNG decoder.
  Stage 5's experimental one-shot JPEG request is **not** a
  direct production PNG-vs-native JPEG comparison.

## Committed offline analyzer

`scripts/c5_jpeg_audit.py` uses Pillow for a **full JPEG entropy decode**,
not just the SOI/EOI and Start-of-Frame checks used during Stage 4B/5.
Its JSON report includes bounded image size, sampling scheme,
quantization-table fingerprint, image luminance standard deviation,
and four edge-strip RGB means for ambient-light relevance. It compares
a pair of JPEGs with mean absolute RGB error and PSNR **only when
they represent the same static, known-clear scene**.

It intentionally never contacts Luna, the TV, SSH or WLED, and never
writes, logs or transmits image pixels. The operator supplies two
*already existing, locally held* JPEG files; it does not extract
previous transient frames. Symlinks, special files, unexpectedly
large files and incomplete JPEGs are refused.

The script uses Pillow **11.3.0** on the analysis computer, installed
in a private Python environment if it is not present. It is not a
runtime dependency of the Rust TV executable.

Example on a computer with Python and Pillow:

```sh
python3 scripts/c5_jpeg_audit.py --selftest
python3 scripts/c5_jpeg_audit.py --native ./native-static.jpg --oneshot ./oneshot-static.jpg
```

The selftest generates a 320×180 synthetic pattern entirely in memory,
encodes it at two distinct qualities, verifies full decode, compares
different quantization tables, rejects malformed JPEGs and flags an
intentionally low-detail solid-color frame.

**Interpretation caveats:** a fingerprint is not an authoritative JPEG
quality number, and a low-detail frame may be an entirely legitimate
black or plain-color screen. Full JPEG decode alone cannot prove that
LG captured the right video plane or that the content is not
protected. No system should turn these diagnostics into a DRM bypass.

## CI and device state

`.github/workflows/ci.yml` runs the synthetic-only test with
Pillow 11.3.0 inside a temporary Ubuntu virtualenv, then the usual
Rust formatting, tests, Clippy and ARMv7 build. The native diagnostic
label `max_frames` was fixed to reflect its actual configured
frame limit, without changing capture or policy semantics.

No Stage 6 app policy, binary or screenshot has been installed on
the TV. The current working baseline is still Stage 1 with no
persistent `capture.client` grant.

## Recommended next device study — NOT yet authorized

To resolve source-equivalence limitations, Stage 7 should use a
deterministic owner-controlled static test image presented as
independently verified clear content; never a streaming-service or
HDCP/unknown HDMI frame. Within one bounded, separately approved
temporary app-owned permission window, capture a tiny, reversible
sample from both paths at the same resolution and test their JPEG
decode, color-detail statistics and quantization settings **before
deleting them locally**. Keep any difference evaluation *on-device*
or transfer only if the owner separately approves transferring images.
Avoid blind repeated captures; restore all Stage 1 policy files,
receipts and scan status on completion. Protect/Unknown classification
must continue to stop capture, even in a synthetic test.

Only after visual/encoding comparability is established should
we consider a latency or CPU comparison under stationary source and
controlled encoder settings. Retain the production PNG backend until
repeated measured evidence supports a change.
