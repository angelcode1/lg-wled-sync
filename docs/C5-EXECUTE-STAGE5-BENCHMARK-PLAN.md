# C5 Stage 5 — native execute vs executeOneShot: completed benchmark

**Status: owner approved; executed on the real LG C5 on 2026-10-18; 5/5 native and 5/5 one-shot frames passed; full Stage 1 restoration and hash verification confirmed.**
Stage 4B established on 2026-10-18 that the app-owned Rust native
LS2 client can create/configure a reusable C5 capture handle,
request **one** 320×180 BLENDED/JPEG frame using
`com.webos.service.capture/execute`, and destroy the handle. Its
reply/file check passed for 1,587 bytes, SOI/EOI JPEG markers, 320×180
SOF dimensions, and one measured `execute` call latency of **161 ms**.
The temporary `capture.client` grant was removed, original Stage 1
files rescanned and SHA-256 checked (`STAGE1_HASHES_PASS`).

## Observed Stage 5 result (real-device operator logs, 2026-10-18)

The owner ran the approved Stage 5 Mac kit on the C5 with a source that
passed the `Clear` preflight and fresh per-frame classification.
Observed status and diagnostic markers:

```text
STAGE1_HASHES_PASS
C5_STAGE5_POLICY_VERIFIED own_name_only=true outbound_capture_only=true ACG=capture.client
C5_STAGE5_TEMPORARY_ACG_GRANTED__BOUNDED_5_PLUS_5_CLEAR_FRAMES
C5_STAGE5_PREFLIGHT stable_clear_checks=3 protected_unknown_refuse=true
C5_STAGE5_CPU_SOURCE pid=4788 start_ticks=1849 clock_hz=100
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_STAGE5_NATIVE_FRAME index=1 width=320 height=180 bytes=4600 request_ms=151
C5_STAGE5_NATIVE_FRAME index=2 width=320 height=180 bytes=4596 request_ms=154
C5_STAGE5_NATIVE_FRAME index=3 width=320 height=180 bytes=4645 request_ms=151
C5_STAGE5_NATIVE_FRAME index=4 width=320 height=180 bytes=4698 request_ms=151
C5_STAGE5_NATIVE_FRAME index=5 width=320 height=180 bytes=4669 request_ms=151
C5_HANDLE_DESTROYED returned_true=true
C5_STAGE5_NATIVE_PASS frames=5 handles=1 destroyed=1 frame_retained=false
C5_STAGE5_CPU phase=native wall_ms=969 rust_cpu_ms=19.28 luna_children_cpu_ms=47.95 lg_service_cpu_ms=30.00 total_measured_cpu_ms=97.23 one_core_cpu_pct=10.0
C5_STAGE5_ONESHOT_FRAME index=1 width=320 height=180 bytes=8847 request_ms=168
C5_STAGE5_ONESHOT_FRAME index=2 width=320 height=180 bytes=9008 request_ms=164
C5_STAGE5_ONESHOT_FRAME index=3 width=320 height=180 bytes=8990 request_ms=159
C5_STAGE5_ONESHOT_FRAME index=4 width=320 height=180 bytes=8698 request_ms=158
C5_STAGE5_ONESHOT_FRAME index=5 width=320 height=180 bytes=8843 request_ms=161
C5_STAGE5_ONESHOT_PASS frames=5 no_frame_retained=true
C5_STAGE5_CPU phase=oneshot wall_ms=892 rust_cpu_ms=11.80 luna_children_cpu_ms=93.91 lg_service_cpu_ms=40.00 total_measured_cpu_ms=145.71 one_core_cpu_pct=16.3
C5_STAGE5_BENCH_PASS native_frames=5 oneshot_frames=5 format=JPEG dimensions=320x180 one_shot_quality_75_requested=true one_shot_quality_verified=false no_frame_retained=true
C5_STAGE5_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

The independently rerun `status` reported
`ABSENT=/var/home/root/c5-ls2-stage5-backup`, and all four original
Stage 1 executable and policy files passed their stored SHA-256 receipts.
The temporary `capture.client` authorization is no longer installed.

### Derived comparison — five frames per method (exploratory only)

| Quantity | Native reusable handle | executeOneShot |
| --- | ---: | ---: |
| Successfully validated JPEG marker and SOF geometry | 5/5 | 5/5 |
| Mean *request* latency | 151.6 ms | 162.0 ms |
| Mean JPEG file bytes | 4,641.6 B | 8,877.2 B |
| Total wall duration, including media queries and session overhead | 969 ms | 892 ms |
| Rust process CPU | 19.28 ms | 11.80 ms |
| `luna-send` children CPU | 47.95 ms | 93.91 ms |
| LG `captureservice` CPU | 30.00 ms | 40.00 ms |
| Total measured CPU | 97.23 ms | 145.71 ms |
| One-core-equivalent CPU utilization during phase | 10.0% | 16.3% |

Five native frames consumed 48.48 ms (~33.3%) less **measured CPU
time**, and their mean capture request was 10.4 ms (~6.4%) faster.
However native **whole-phase wall time was longer** (969 vs 892 ms)
because setup and other overhead are included. Aggregate measured CPU
is the sum of only the three sampled process categories, **not total
system consumption**. LG service usage is quantized in 10-ms CPU
ticks at `clock_hz=100`, so its 10-ms difference is only one tick.

**Most important fairness limitation:** the native JPEG averaged
4.64 KB versus 8.88 KB for one-shot, and the one-shot quality-75
request was *not verified as applied*. The output image content
changed as the video played; neither format-equivalent JPEG quality
nor visual/decoded-content equivalence was proven. Therefore the CPU
and latency improvement is *not* a controlled quality-matched
benchmark, and **do not switch production capture based on it**.
Only JPEG file size, SOI/EOI and SOF dimensions were validated;
full JPEG decoding and meaningful frame content remain unverified.
The file bytes were not retained.

**Usefulness for ambient LEDs:** measured requests of about 150–162 ms
plus other work would cap sequential capture at only single-digit
frames/second in this configuration. This is insufficient evidence for
smooth high-FPS ambient lighting and does not show zero-copy
capability. The observed five-shot run must not be extrapolated to
sustained performance or to protected material.

Note: the shared lifecycle diagnostic currently prints
`max_frames=1` in `C5_HANDLE_TEST_BEGIN` even when Stage 5 actually
requests five frames. The five indexed native `C5_STAGE5_NATIVE_FRAME`
lines, `C5_STAGE5_NATIVE_PASS frames=5`, and source loop
`for number in 1..=frame_limit` provide the authoritative
verification. Correct the misleading label before any future kit.

### Next recommended investigation (separately authorized)

First perform an offline decoder-quality check using synthetic JPEG
test fixtures (without capturing new TV material) and source review
of how `executeOneShot` accepts/ignores JPEG quality fields.
Then, if a new on-device test is approved, use a deterministic,
self-owned, DRM-free test clip or calibrated still with a stable
pattern so both methods see equivalent scenes. Verify full JPEG
decode, pixel-level usefulness for sampling, and quality settings.
Separate session-setup time, capture RPC, media checks and per-frame
read/validation; sample the three CPU categories; abort on any
Unknown/Protected status. A follow-up high-FPS design should consider
the separately probed histogram/VT capture path only if its
protection policy can be enforced without bypassing LG security.
Do not change factory permissions or the working Stage 1 baseline.

## Questions Stage 5 should actually answer

- Does one configured, persistent LS2 handle reduce *per-frame* request
  latency compared with five `executeOneShot` calls on the same TV?
- Does eliminating repeated `luna-send` spawn and policy lookup cut
  client + helper CPU usage, after startup costs are separated?
- Does LG's `captureservice` consume more or less CPU with native
  session reuse? Count **service CPU as well as the Rust process**:
  Rust-only `getrusage(RUSAGE_SELF)` is insufficient and misses both
  `luna-send` children and LG's separate capture service.
- Does each frame produce a regular, bounded, structurally valid image,
  and can each handle and temporary file be reliably cleaned up?

## Original approved benchmark design (completed)

Use exactly five 320×180 BLENDED JPEG captures via the native
`execute` session and exactly five by the existing,
known-working `executeOneShot` path. Order deliberately (e.g. two
bounded phases, not concurrent). No unattended continuous service,
and no WLED, Hue or network image output.

1. Ask owner to display a personally controlled, unencrypted local
   video. Before **every** frame, repeat an independent
   `com.webos.media/getActivePipelines` classification; allow
   `Clear` only. Unknown/Protected/HDMI/failed media query aborts
   immediately, no bypass or retry.
2. For native phase create one numeric handle, set the already
   verified BLENDED 320×180 JPEG 75 properties, options and output
   path. Keep the *same* LS2 connection/handle. Change only the
   bounded private filename if necessary and verify whether the
   service overwrites a file safely; never reuse a leftover file.
   Call `execute` five times with per-request wall clocks.
3. For one-shot phase request five frames through the already proven
   `capture::luna_client` path; keep format, dimensions,
   quality and output type matched. If one-shot JPEG quality/format
   differs or cannot be configured equivalently, label results as
   **non-equivalent**, do not claim a fair CPU/image comparison.
4. Use a fresh mode-0700 private directory, no follows of symlinks,
   only regular files with expected file size and SOI/EOI/SOF JPEG
   dimensions. Record metadata (bytes, milliseconds) not image bytes.
   Delete each file after validation, and delete the directory on
   any normal Rust exit. No decoded frames, raw image logging,
   screenshot uploads or LED output.
5. Before/after each phase sample process CPU user+system ticks for
   our Rust binary, `luna-send` child CPU (via `RUSAGE_CHILDREN`
   or equivalent), and LG's `captureservice` via anchored /proc
   identity + utime/stime counters. Do not count process startup as
   an unlabelled frame-only measurement; report setup, request,
   read/validation and teardown timing separately.
6. Do not modify the installed production capture backend. The
   temporary ACG grant remains *only* `capture.client` for the
   owned registered client, and an outbound whitelist to
   `com.webos.service.capture`; exact Stage 1 backup, hashes,
   automatic restoration and independent `status` check are
   mandatory. Never impersonate factory Luna roles.
7. No frame should be attempted after a create/destroy failure,
   missing ACK, source-classification change, file-access error
   or CPU-sampling anomaly. A crash may leave a server-side
   handle despite the normal `destroyHandle` path, so no automatic
   retries or capture-service restart.

Interpretation: the Stage 4B value of 161 ms cannot prove throughput.
A 5+5 benchmark is still small-sample, exploratory evidence.
It may identify gross spawn overhead or unexpectedly high LG service
CPU, but it is not sufficient on its own for an unconditional
production switch. Repeat only after explicit approval.

## Activation gate

Stage 5 was separately approved, built with passing ARMv7 and Rust CI,
executed, then automatically restored to Stage 1. This document does
not approve any follow-up captures or privilege changes.
