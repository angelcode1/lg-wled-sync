# C5 Stage 5 — controlled native execute vs executeOneShot benchmark (proposal)

**Status: design only; not authorized, not deployed or run.**
Stage 4B established on 2026-10-18 that the app-owned Rust native
LS2 client can create/configure a reusable C5 capture handle,
request **one** 320×180 BLENDED/JPEG frame using
`com.webos.service.capture/execute`, and destroy the handle. Its
reply/file check passed for 1,587 bytes, SOI/EOI JPEG markers, 320×180
SOF dimensions, and one measured `execute` call latency of **161 ms**.
The temporary `capture.client` grant was removed, original Stage 1
files rescanned and SHA-256 checked (`STAGE1_HASHES_PASS`).

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

## Proposed benchmark design (requires separate approval)

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

No Stage 5 binary, policy change, installer or device action is
authorized by this planning document. Only produce and run a new
artifact after explicit owner approval and passing GitHub CI.
