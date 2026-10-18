# C5 Stage 4A — configure a single native capture handle, without execute

**Status: completed successfully on the owner's LG C5 on 2026-10-18; original Stage 1 permissions and executable restored.**

## Verified real-device Stage 4A result

The owner ran the checksummed Stage 4A kit. Observed on the LG C5:

```text
STAGE1_HASHES_PASS
C5_STAGE4A_POLICY_VERIFIED own_name_only=true outbound_capture_only=true ACG=capture.client
C5_STAGE4A_TEMPORARY_ACG_GRANTED__CONFIG_ONLY_NO_EXECUTE
C5_HANDLE_TEST_BEGIN client=org.webosbrew.lg-hue-sync.c5-probe max_handles=1 config=true no_pixels=true
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_CONFIGURED method=BLENDED format=JPEG width=320 height=180 no_execute=true
C5_HANDLE_DESTROYED returned_true=true
C5_HANDLE_CONFIGURATION_PASS created=1 configured=true destroyed=1 no_pixels=true
C5_STAGE4A_CONFIGURATION_PASS__NO_EXECUTE
C5_STAGE4A_RESTORING_STAGE1_AFTER_TEST
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

The separate final status check verified the original Stage 1 Rust binary
and three custom policy files against their stored SHA-256 hashes; the
Stage 4A temporary backup directory was absent. No vendor roles,
LG-reserved names or wildcard ACG entries were introduced.

**What this proves:** one client-owned numeric handle accepts the exact
320×180 BLENDED/JPEG `setProperties`, quality-75 `setOptions`,
private JPEG-path `setOutput`, and `destroyHandle` in the same native
LS2 registration. Both normal cleanup and policy rollback passed.

**Still unknown:** the effect of `execute`, JPEG output validity, whether
a frame is produced at all, file-read or frame-copy overhead, whether
`execute` can be called repeatedly on the configured handle, and
the CPU load of both the Rust client and LG `captureservice`.
A successful `setOutput` ACK does not establish actual capture.

## What Stage 3 established

On 2026-10-18 the owner's real C5 webOS 10 responded successfully:

```text
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_DESTROYED returned_true=true
C5_HANDLE_LIFECYCLE_PASS created=1 destroyed=1 no_pixels=true
C5_STAGE1_RESTORED_AND_RESCANNED
STAGE1_HASHES_PASS
```

Those are normal-path lifecycle observations: the native client registers
as `org.webosbrew.lg-hue-sync.c5-probe`, receives a numeric handle, destroys
it successfully, and has its original non-capture permissions restored.

## Why configure without execute first?

Stage 3 did not verify `setProperties`, `setOptions`, or `setOutput`.
A different webOS client [tv-capt-remote.js](https://github.com/lennylxx/webos-iptv-player/blob/7d8fd30e927ee9d09ba7979b053f9a25ac932420/scripts/tv-capt-remote.js)
uses these operations on a persistent LS2 handle before *each*
`execute {handle}` request. This establishes a protocol example but
does **not** establish the method contracts or format availability on C5.

For Stage 4A, the Rust executable offers:

```sh
/var/home/root/c5-ls2-probe test-ls2-handle-configuration \
  --acknowledge-handle-configuration
```

It uses one native LS2 connection and one capture handle, and makes
only these calls with 2-second response deadlines:

1. `createHandle {}` and validate the numeric/string handle.
2. `setProperties {"handle":h,"properties":{"method":"BLENDED","width":320,"height":180,"format":"JPEG"}}`.
3. `setOptions {"handle":h,"options":{"jpegQuality":75}}`.
4. `setOutput {"handle":h,"file":{"path":"/tmp/lg-hue-sync-stage4-<pid>/single.jpg"}}`.
5. Attempt `destroyHandle {"handle":h}` regardless of setup success;
   unregister the client and delete the fresh private temporary directory.

The directory is created with mode `0700`, never reused, and removed
via Rust drop guard. No `execute`, screenshots, decoder, pixels,
VT buffer, WLED output or continuous capture loop is invoked.
The path must remain private and outside Kodi or media-library mounts.
If a setup RPC is rejected, stop and report it instead of trying other
formats or privilege levels automatically.

**Uncertainty:** `setOutput` may itself touch or open a file in LG's
service, so "no pixels" means only that our client never requests a
capture or reads any image; it does not prove LG performs no internal
work. Loss of the destroy acknowledgement or client crash remains
an orphan-handle risk. Do not auto-retry.

## Permission and operator controls

Stage 4A is a separately approved temporary use of exactly the same
app-owned `capture.client` ACG and outbound
`com.webos.service.capture` destination as Stages 2 and 3. The ACG
still permits other capture methods at policy level even though this
command doesn't call them. No factory/vendor policies, `all` group,
LG-reserved service names, or capturepermission mutators are used.

Use the proven Stage 3-style Mac installer pattern: pin and checksum
the passing ARMv7 artifact, confirm current `STAGE1_HASHES_PASS`,
back up the existing binary and three policy files, temporarily
apply only our Stage 2 policy, rescan with `ls-control`, execute once,
then restore exact Stage 1 files and rescan whether the test passed
or failed. Verify `STAGE1_HASHES_PASS` independently afterward.
A hard interruption can require owner-initiated recovery.

Stage 4A was explicitly approved, executed and automatically rolled back.
The installed baseline on the C5 is the original Stage 1 role and binary.

## After Stage 4A, Stage 4B needs another explicit approval

Only after configure/destroy passes should we test one `execute`
on an independently classified, stable **Clear** local source. The
same protection gate as the current `test-blended` is required,
including a fresh classification before the actual `execute` call.
If Unknown/Protected, refuse and destroy without any capture.

Stage 4B should use a single 320x180 JPEG, inspect only file size,
magic, dimensions and timing, delete the file and its private directory,
and report both Rust-process and `captureservice` overhead.
Do not log or retain raw image data, infer throughput from a single
frame, or automatically change the production backend. Compare a
bounded 5-sample native session against `executeOneShot` only
after this one-frame test and cleanup have been verified.
