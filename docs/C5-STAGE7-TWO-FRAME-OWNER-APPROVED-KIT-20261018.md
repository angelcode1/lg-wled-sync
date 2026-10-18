# C5 Stage 7: owner-approved bounded two-frame comparison kit

**Status 2026-10-18:** the owner-approved Stage 7 test kit was built;
CI build **39131994753** passed:
https://github.com/angelcode1/lg-wled-sync/actions/runs/39131994753

**Owner-reported TV run, 2026-10-18:** the two bounded, known-clear
320×180 JPEGs were produced and fully decoded on the Mac, and the
original Stage 1 executable/LS2 policies passed independent SHA-256
verification after restore. The Mac wrapper then exited with
`SSH_PID: unbound variable` at line 356, **after** the successful
stream/decode. Do not treat this trailing shell error as a failed
capture or as proof that the remote scratch cleanup finished. See
the measured-result and follow-up section below. The working original
Stage 1 baseline was confirmed restored.

## Reproducible compiled artifact

- ARMv7 source build commit:
  `4f7c066a3a0d48fae7fd698c3175c2c70ed14921`.
- CI test/Rust fmt/Clippy: passed.
- ARMv7 ELF/WebOS glibc ≤2.28 check: passed.
- CI artifact: `c5-blended-armv7`, ID `12138640399`.
- Exact artifact ZIP SHA-256:
  `7b51cfaef5e9378f60b7e7a78d5915e63e87fb36007f8111cca3f893bbe61192`.
- ELF SHA-256:
  `1a328dae8dd93374936ea0f18702a58b7868c028448cbbf799922e874f0c2c09`.
- ELF Build ID:
  `b75967427903d87c1775b634c44466ff9c66d9f1`.
- Mac bundle:
  `c5-ls2-stage7-kit.zip`; SHA-256:
  `e1211daafc631bb381cfd46199376987430015fb81492e96e4705233598589ef`.
  The bundle includes the ARM ZIP plus the approved installer, Stage 6,
  Stage 7 and streaming decoders, README and per-file SHA manifest.
- Local bundle `unzip -t`, every per-file SHA-256, `sh -n`,
  synthetic Stage 6, synthetic Stage 7 and streaming decoder self-tests
  all passed. The streaming decoder refuses incomplete/malformed TAR/JPEG
  and saves no image files.

## Exact capture behavior

`src/ls2_stage7.rs` declares the separately acknowledged command:

```text
test-ls2-stage7-clear-pair --acknowledge-stage7-clear-pair
```

- Three initial independent `com.webos.media/getActivePipelines`
  `Clear` classifications, and a new one **before each capture**.
  Unknown, Protected and HDMI always refuse; there is no screenshot
  fallback or retry. Operator must present an independently known
  stationary, DRM-free local media/VDEC source.
- Only one native `createHandle` / `setProperties` /
  `setOptions` / `setOutput` / `execute` /
  `destroyHandle` cycle with 320×180 BLENDED/JPEG and
  `jpegQuality=75`. The native session is destroyed/unregistered
  before any one-shot action.
- Then at most **one** `executeOneShot` 320×180 BLENDED/JPEG
  with `jpegQuality=75` **requested**, not proven accepted.
- No histogram use, no protected frame requests, no recurring
  capture, no WLED/Hue connection and no factory-role impersonation.
  `src/main.rs` exposes only an extra opt-in CLI subcommand:
  the production daemon's PNG route is unchanged.

## Installer / transfer / rollback design

The Mac installer is derived from the **already successful Stage 5
stage/restore script**, but with a new
`/var/home/root/c5-ls2-stage7-backup` path and a two-frame-specific
CLI and log assertion.

`status` checks the original four Stage 1 executable/role/client
permission/manifest SHA-256 receipts.
`install` requires the operator's action, pins the exact
CI artifact SHA, validates the test binary, backs up all four
Stage 1 files **plus receipt**, and checks every backup byte before
temporarily granting only the owned
`org.webosbrew.lg-hue-sync.c5-probe` client's
`capture.client` and outbound-to-capture grant.

After the two-frame command succeeds and its bounded output names
are verified, the installer **restores the exact Stage 1 files,
rescans LS2, rechecks the Stage 1 hashes, and removes the backup
before sending any frame** to the Mac. It then streams precisely
`native.jpg` and `oneshot.jpg` from the owned, mode-0700 TV
scratch directory over authenticated SSH as TAR bytes into a Mac-side
Python process. The Mac process decodes both JPEGs in memory using
Pillow 11.3.0; it never writes image files. Only derived numeric
RGB-edge, JPEG quality fingerprints and decode metrics are printed.
A remote exit trap deletes the two named images and owned staging
files. The script never uploads imagery to ChatGPT or the network
outside the owner's local SSH connection.

`rollback` accepts only an intact, verified backup and restores
those exact four paths plus receipt. Interrupted SSH or a crashed
TV-side native handle can leave an orphan server-side handle or
temporary file. **Never automatically retry** after a failed call,
missing destruction acknowledgement or unknown state. Run `status`,
inspect errors, and use `rollback` if a stage backup exists.

### Operator prerequisites and commands

This is a device test: do not run for protected content.

```sh
cd ~/Downloads
unzip -q c5-ls2-stage7-kit.zip
cd c5-ls2-stage7-kit
python3 -m venv .venv
.venv/bin/python -m pip install 'Pillow==11.3.0'
.venv/bin/python c5_stage7_stream_audit.py --selftest
sh c5-ls2-stage7-mac.sh status
# ONLY WHEN THE APPROVED LOCAL DRM-FREE STATIONARY SCENE IS SHOWING:
sh c5-ls2-stage7-mac.sh install
sh c5-ls2-stage7-mac.sh status
```

If the install does not restore, **do not repeat install**:

```sh
sh c5-ls2-stage7-mac.sh rollback
sh c5-ls2-stage7-mac.sh status
```

The two frames are compared for decoded edge-color concordance;
the one-shot JPEG quality is not established by requesting
`jpegQuality=75`. A single two-frame experiment cannot quantify
sustained throughput or prove hardware capture is permitted on
protected content. No production change should follow solely
from this sample.

## Owner-reported Stage 7 result and interpretation (2026-10-18)

The operator supplied the following terminal markers after running the
bounded kit on the real C5 against an owner-controlled clear source:

```text
C5_STAGE7_POLICY_VERIFIED own_name_only=true outbound_capture_only=true ACG=capture.client
C5_STAGE7_TEMPORARY_ACG_GRANTED__BOUNDED_1_PLUS_1_CLEAR_FRAMES
C5_HANDLE_CREATED kind=number no_frame_requested=true
C5_HANDLE_CONFIGURED method=BLENDED format=JPEG width=320 height=180 execute=true
C5_SINGLE_EXECUTE_VALID_JPEG width=320 height=180 bytes=7799 execute_ms=161 frames=1
C5_HANDLE_DESTROYED returned_true=true
C5_SINGLE_EXECUTE_PASS created=1 executed=1 destroyed=1 frame_retained=false
C5_STAGE7_ONESHOT_FRAME width=320 height=180 bytes=14441 request_ms=160 staged_private=true quality75_requested=true quality75_verified=false
C5_STAGE7_PAIR_READY native_frames=1 oneshot_frames=1 no_protected_fallback=true
C5_STAGE7_PAIR_PASS__RESTORE_BEFORE_STREAMING
C5_STAGE7_STAGE1_RESTORED_HASHES_PASS
C5_STAGE7_TWO_FRAME_TAR_STREAMED
C5_STAGE7_DECODE_PASS frames=2 full_decode=true raw_image_files_written=false
c5-ls2-stage7-mac.sh: line 356: SSH_PID: unbound variable
ABSENT=/var/home/root/c5-ls2-stage7-backup
STAGE1_HASHES_PASS
```

The final independent `status` reported all four original Stage 1
file hash checks as OK, and no Stage 7 backup directory. This verifies
the installed Stage 1 baseline, not necessarily every scratch file's
absence after the wrapper's abnormal exit.

### Image quality evidence

| Measurement | Native handle | One-shot |
| --- | ---: | ---: |
| JPEG bytes | 7,799 | 14,441 |
| Request latency | 161 ms | 160 ms |
| Dimensions | 320×180 | 320×180 |
| Full JPEG decode | Yes | Yes |
| Luminance standard deviation | 24.06 | 26.96 |
| Mean luminance | 11.38 | 12.25 |
| JPEG quantization fingerprint | `c560863d3b5ee431` | `df846ace420ab4c2` |

Two-image comparison: mean absolute RGB difference **10.405**,
PSNR **19.42 dB**, JPEG sampling matched but quantization tables
**differed**; the requested one-shot JPEG quality level was **not
verified**. Maximum per-channel centre-edge RGB differences were:
top **3.29**, right **12.27**, bottom **1.26**, left **0.25**.
Both frames passed the decoder's detail sanity check.

These are comparisons between two decoded images, not measurements
against an independent known RGB reference. The disparity at the right
edge could reflect source motion/scene differences, JPEG settings,
geometry/source-plane differences, or another confounder; none is
established here. `source_identity_confirmed_by_code=false`.
Do **not** infer equivalent encoders, pixel-level fidelity, stable
continuous FPS, access to protected content, or production suitability
from one pair. The two ~160 ms requests alone are not a throughput
benchmark. Histogram functionality was not tested in this experiment.

### Trailing Mac shell-script error and safe next actions

The error `SSH_PID: unbound variable` is consistent with a shell
executing with `set -u` and expanding an unset process variable
in post-stream cleanup. The installer script was distributed in the
separate Mac kit; it is **not tracked in this repository**, so the
precise faulty line and the intended process lifecycle have not
been independently inspected. Do not claim that the bundled installer
has been fixed.

Before any future on-device test:

1. On the Mac, inspect the existing kit script around line 356:
   `nl -ba c5-ls2-stage7-mac.sh | sed -n '330,370p'`.
   Initialize optional subprocess PIDs before their first use and
   guard each reference with `${SSH_PID:-}` where appropriate.
   Do not kill a guessed PID or automatically rerun the capture.
2. Read-only inspect the TV scratch namespace; independently check
   for leftover `/tmp/c5-ls2-stage7-*` paths. Do not infer complete
   scratch cleanup merely from a passing Stage 1 receipt.
3. A cleanup-only change should be shipped as a **new checksum-pinned
   Mac kit**, with shell syntax/lifecycle tests for cleanup before any
   repeat experiment. Existing Stage 1, permissions, and production
   behaviour should remain unchanged.

All Stage 7 metrics quoted above derive from operator-provided terminal
output, not from image files. No image was supplied to the repository.
