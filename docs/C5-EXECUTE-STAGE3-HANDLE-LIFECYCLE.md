# C5 Stage 3: single handle lifecycle (source prepared, NOT authorized to run)

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

## Proposed lifecycle test — not run yet

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

**Important:** `createHandle` and `destroyHandle` mutate LG capture
service state, unlike Stage 2. If `createHandle` succeeds internally
but its reply is lost or the process crashes, a handle may persist
until the service cleans up; that cleanup is not verified. Nothing in
Stage 2 proves the create/destroy API is safe on C5. No automatic retry.

## Permissions, deployment and rollback

Stage 3 would reuse the *temporary* private `capture.client` ACG
and exact outbound destination from Stage 2. It must be granted only
for the bounded test, using the same Stage 1 backup, verified file
hashes, `ls-control scan-services`, and automatic Stage 1 policy
restoration and scan afterward. This ACG grants more than these two
methods, even though the code calls only them. Do not borrow LG's
`lightingcontroller` identity or add `all` privileges.

This stage requires its **own explicit owner approval** prior to
installation/test, after CI verification and a reviewed Mac installer.
If a successful test returns created=1, destroyed=1, no_pixels=true,
proceed to a later separately approved single clear `execute` frame.
Do not infer hardware throughput or frame-buffer safety from successful
handle management.
