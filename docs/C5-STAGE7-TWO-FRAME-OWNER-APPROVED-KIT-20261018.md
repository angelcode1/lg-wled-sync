# C5 Stage 7: owner-approved bounded two-frame comparison kit

**Status 2026-10-18:** the owner explicitly approved **preparing**
a Stage 7 test kit. Source implementation is complete and
CI build **39131994753** passed:
https://github.com/angelcode1/lg-wled-sync/actions/runs/39131994753

No Stage 7 capture has been performed on the owner's C5 by this
preparation. The existing Stage 1 installation is unchanged.

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
