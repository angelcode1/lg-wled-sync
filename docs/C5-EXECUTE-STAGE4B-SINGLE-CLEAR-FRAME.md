# C5 Stage 4B — single known-clear native execute (NOT run or approved)

**State on the owner's TV:** Stage 4A passed and the exact Stage 1
files, binary and restricted permissions were restored. The Stage 4B
code exists only in the repository. No new capture has been requested.

## Confirmed LG C5 prerequisites (2026-10-18)

The owner ran Stage 4A and observed:

```text
C5_STAGE4A_TEMPORARY_ACG_GRANTED__CONFIG_ONLY_NO_EXECUTE
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_CONFIGURED method=BLENDED format=JPEG width=320 height=180 no_execute=true
C5_HANDLE_DESTROYED returned_true=true
C5_HANDLE_CONFIGURATION_PASS created=1 configured=true destroyed=1 no_pixels=true
C5_STAGE4A_CONFIGURATION_PASS__NO_EXECUTE
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

**No `execute` call, frames or video-stream evidence yet.**

## Proposed one-frame test

The opt-in Rust command is:

```sh
/var/home/root/c5-ls2-probe test-ls2-single-clear-execute \
  --acknowledge-single-clear-execute
```

This is **not** the main app/daemon. It is a one-shot executable behind
a separate explicit CLI acknowledgement. It cannot run without a
temporarily installed, own-name `capture.client` permission grant.

The command must:

1. Independently classify foreground `com.webos.media` pipelines as
   `Clear` three times about one second apart, using the existing
   fail-closed media classifier. HDMI, secure/protected, idle, malformed
   or unknown pipeline metadata **always refuse** before LG capture
   handle creation.
2. Register the dedicated app-owned native Luna name and create at most
   one numeric/string capture handle.
3. Apply the **Stage 4A verified** 320×180 BLENDED/JPEG, quality-75
   properties/options, and a private fresh mode-0700 temporary
   directory. Each bounded LS2 call has a two-second reply window.
4. **Re-query the media classifier immediately before `execute`**.
   If no longer `Clear`, refuse, destroy the handle and unregister.
5. Call **exactly once** `execute {"handle":h}`. On `returnValue:true`,
   accept only a regular output file within the private directory
   (max 2 MiB), a JPEG SOI/EOI header and SOF dimensions exactly
   320×180. Do not decode or publish the image; report only byte
   length and request time.
6. Attempt `destroyHandle` regardless of execution/validation
   outcome, unregister the LS2 client, then remove the ephemeral JPEG
   and its owned directory in all normal Rust error paths.
7. The *separately reviewed and approved* installer must restore the
   original Stage 1 binary and exact hashes of all owned policy files
   and rescan LG's LS2 policies regardless of test pass/fail.

No screenshots or pixels are logged, transmitted or intentionally
retained. The JPEG necessarily exists transiently if `execute`
works. A process crash/hard reset cannot guarantee handle cleanup.
Do not retry automatically; do not restart LG `captureservice`.

The existing `capture.client` ACG itself is broader than these
requests, even though the command makes no additional capture method
calls. Never widen or spoof service names, ACGs or LG roles.

## Limits and next decision

- Successful `execute` proves one native session-based capture on this
  C5, **not** zero-copy, streaming, protected-content safety, useful
  frame rates, or low combined CPU usage.
- The independent classifier is sampled immediately before the call,
  but no asynchronous source-status API can guarantee the content
  never changes in the interval between checking and capture. Use only
  a manually controlled, known-clear local video; refuse if metadata
  is not `Clear`.
- A single-frame test cannot measure sustained performance. After
  a verified pass, prepare a **separately approved** bounded, e.g. five
  frame comparison with `executeOneShot`, capturing both our Rust
  process CPU and LG's `captureservice` CPU. Keep the production
  `executeOneShot` path unchanged until results justify replacement.

**Stage 4B requires new explicit user approval** before deploying
the temporary ACG or running this frame test. The Stage 4A approval
did not cover an `execute` call.
