# C5 Stage 3: single handle lifecycle — PASSED on LG C5

## Actual Stage 3 C5 result (2026-10-18)

Owner approved and ran the Stage 3 installer on their LG C5 webOS 10.
The status check preceding installation returned `STAGE1_HASHES_PASS`.
The diagnostic used an exact own-name regular/dev LS2 client and only a
temporarily granted `capture.client` group. Its observed output:

```text
C5_STAGE3_TEMPORARY_ACG_GRANTED__HANDLE_ONLY_NO_PIXELS
C5_HANDLE_TEST_BEGIN client=org.webosbrew.lg-hue-sync.c5-probe max_handles=1 no_pixels=true
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_DESTROYED returned_true=true
C5_HANDLE_LIFECYCLE_PASS created=1 destroyed=1 no_pixels=true
C5_STAGE3_HANDLE_LIFECYCLE_PASS__NO_CAPTURE
C5_STAGE3_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
```

The owner's final separate `status` command returned
`STAGE1_HASHES_PASS` and
`ABSENT=/var/home/root/c5-ls2-stage3-backup`. Thus the
original Stage 1 executable, manifest, role and zero-ACG client-permission
files were restored and their stored hashes passed. Nothing was left
with persistent `capture.client` access.

**Confirmed on this C5:** the `createHandle {}` reply contained a
numeric `handle`, and a `destroyHandle {"handle":...}` call from the
same registered LS2 client returned success. The single-handle
lifecycle is viable in the tested happy path.

**Not confirmed:** whether `setProperties`, `setOptions`,
`setOutput` or `execute` work on C5; captured pixels, actual FPS,
CPU overhead, automatic cleanup after a client crash or a lost
create/destroy reply; behaviour under protected content.
There is no evidence of a video stream or native VT buffer ownership.
Continue to avoid LG role impersonation, wildcard LS2 grants, or
screenshots on unknown/protected media.

## Grounded status

On the owner's real LG OLED42C5 webOS 10, Stage 2 returned:

```text
C5_STAGE2_TEMPORARY_ACG_GRANTED__NO_VIDEO_CAPTURE
LS2_CAPTURE_PREFLIGHT name=org.webosbrew.lg-hue-sync.c5-probe method=getCapability no_pixels=true
LS2_GETCAPABILITY_REPLY {"returnValue":true}
LS2_GETCAPABILITY_DONE unregistered=true capture=false
C5_STAGE2_GETCAPABILITY_PASS__NO_CAPTURE
C5_STAGE2_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
```

The original Stage 1 binary and restricted zero-ACG role are restored.
The temporary Stage 2 capture group is **not** persistently enabled.
The native, exact owned LS2 identity and `capture.client` access to
`getCapability` both succeeded. There were **no capture handles, images,
or screenshot requests in Stage 2**.

## Implemented lifecycle diagnostic (now verified)

The Rust binary now includes the explicitly opt-in command:

```sh
/var/home/root/c5-ls2-probe test-ls2-handle-lifecycle \
  --acknowledge-handle-lifecycle
```

It uses one named LS2 client, one GLib context and one callback slot:

1. `LSRegister` our own name.
2. `com.webos.service.capture/createHandle {}`, max 2 seconds.
3. Require an explicit non-null `handle` of numeric or string form.
4. `com.webos.service.capture/destroyHandle {"handle":...}`, max 2 seconds.
5. Check `returnValue:true`, unregister and free the GLib context.

The handle response structure is grounded in [webos-iptv-player's
client](https://github.com/lennylxx/webos-iptv-player/blob/7d8fd30e927ee9d09ba7979b053f9a25ac932420/scripts/tv-capt-remote.js),
**not confirmed on C5**. On missing/invalid handle, denied call,
timeout or cleanup error, fail closed and report. There is no image,
`execute`, `setOutput`, VT buffer or WLED activity.

**Important:** this test confirms the normal-path cleanup, not orphan
handle cleanup. If a future client aborts or loses the create reply,
server-side state may remain unknown. Never auto-retry a failed create
or escalate permissions to attempt recovery.

## Permissions, deployment and rollback

Stage 3 would reuse the *temporary* private `capture.client` ACG
and exact outbound destination from Stage 2. It must be granted only
for the bounded test, using the same Stage 1 backup, verified file
hashes, `ls-control scan-services`, and automatic Stage 1 policy
restoration and scan afterward. This ACG grants more than these two
methods, even though the code calls only them. Do not borrow LG's
`lightingcontroller` identity or add `all` privileges.

Stage 3 **was explicitly approved and executed successfully**. Any
future configuration or single clear `execute` frame is a separate
stage requiring another approval, known-clear gating, secure ephemeral
file handling and cleanup verification. Do not infer throughput or
frame-buffer safety from successful handle management.
