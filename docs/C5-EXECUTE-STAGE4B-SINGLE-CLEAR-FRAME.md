# C5 Stage 4B — one-frame native execute PASSED (2026-10-18)

**State on the owner's TV:** Stage 4A passed. Stage 4B was approved and
attempted once on 2026-10-18, but **its independent media classifier did
not return Clear**, so the diagnostic exited before LS2 registration,
before `createHandle`, and before `execute`. Its installer restored
the original Stage 1 executable and non-capture policy and verified all
stored SHA-256 hashes. No additional capture was requested.

## Second Stage 4B attempt — native frame confirmed

After the initial fail-closed attempt, the owner ran the same approved
Stage 4B kit again on 2026-10-18. The complete logs show the following
successful sequence on the LG C5:

```text
STAGE1_HASHES_PASS
C5_STAGE4B_POLICY_VERIFIED own_name_only=true outbound_capture_only=true ACG=capture.client
C5_STAGE4B_TEMPORARY_ACG_GRANTED__SINGLE_CLEAR_FRAME_ONLY
C5_HANDLE_TEST_BEGIN client=org.webosbrew.lg-hue-sync.c5-probe max_handles=1 config=true execute=true max_frames=1
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_CONFIGURED method=BLENDED format=JPEG width=320 height=180 execute=true
C5_SINGLE_EXECUTE_VALID_JPEG width=320 height=180 bytes=1587 execute_ms=161 frames=1
C5_HANDLE_DESTROYED returned_true=true
C5_SINGLE_EXECUTE_PASS created=1 executed=1 destroyed=1 frame_retained=false
C5_STAGE4B_SINGLE_CLEAR_EXECUTE_PASS__RESTORING_STAGE1
C5_STAGE4B_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

The separately invoked final `status` check reported
`ABSENT=/var/home/root/c5-ls2-stage4b-backup` and verified
the original Stage 1 executable and three app-owned policy files
against their stored SHA-256 hashes. Only the temporary app-owned
`capture.client` permission was granted, and it was restored to
the Stage 1 restricted policy. No factory roles were edited.

**Established:** `execute {"handle":h}` under one registered native
LS2 client did return `returnValue:true` and create an ephemeral
1,587-byte JPEG with the expected SOI/EOI markers and 320×180 SOF
dimensions, for a single measured `execute` RPC of 161 ms.
The same client then received `destroyHandle` success.
The code deleted its private output directory and did not log or
retain image bytes.

**Not established:** that this JPEG fully decodes or represents
the intended source scene (only header/size/dimensions were tested),
sustained frame rate, relative speed or CPU efficiency compared
with `executeOneShot`, zero-copy processing, frame accuracy or
protected-content behaviour. The 161-ms value is *one*
`execute` call's latency, not an average nor end-to-end setup cost.

The earlier failure is not evidence of a regression: that attempt's
media classifier did not return Clear, while the successful attempt
passed the separate preflight and pre-execute clear-media checks.
The exact reason those classifications differed is unknown; neither
raw media envelope was retained.

## Actual first Stage 4B execution — NO CAPTURE

The user-provided Mac installer output included:

```text
STAGE1_HASHES_PASS
C5_STAGE4B_POLICY_VERIFIED own_name_only=true outbound_capture_only=true ACG=capture.client
C5_STAGE4B_TEMPORARY_ACG_GRANTED__SINGLE_CLEAR_FRAME_ONLY
Error: C5_EXECUTE_REFUSE_MEDIA_NOT_CLEAR: no capture requested
C5_STAGE4B_EXECUTE_FAILED_OR_NOT_CLEAR__RESTORING_STAGE1
C5_STAGE4B_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

The final independent `status` check also reported
`ABSENT=/var/home/root/c5-ls2-stage4b-backup`.
This failure happened at the **initial clear-media classification
preflight**, before the code prints `C5_HANDLE_TEST_BEGIN`.
Therefore, this run created **no capture handle**, issued **no
`execute`**, and generated **no JPEG**. The installer's generic
"server-side handle cleanup may be unknown" message is inapplicable
to this particular pre-handle refusal; no orphan handle was created.

The raw `com.webos.media/getActivePipelines` reply was not logged,
so the underlying cause remains **undetermined**. The Rust classifier
returns `Clear` only for a well-formed foreground `type:"media"`
pipeline containing `VDEC`, with no `SVP_CPB` resource anywhere;
all HDMIs, idle and unknown shapes, failed/slow interactive Luna
requests, and potentially clear content not represented in this
resource format yield `Unknown`. A protected pipeline reports
`Protected`. No independent evidence yet distinguishes these cases.

**Next step (read-only, no ACG grant):** inspect a bounded live
`getActivePipelines {}` reply while the intended local video plays,
using the TV's `luna-send -i -f` with open stdin. Redact paths/titles
before sharing. Do **not** change the DRM classifier to classify HDMI
or Unknown as Clear, and do not retry Stage 4B automatically.

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

**This Stage 4A prerequisite had not called `execute`; the subsequent Stage 4B success above now confirms one native frame.**

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

Stage 4B was approved and subsequently executed successfully after
an initial media-gate refusal. Both attempts restored Stage 1.
Next: design a new, separately approved bounded benchmarking test.
Do not broaden `capture.client`, weaken the protected/unknown
source refusal, or automatically switch the production backend.
