# LG C5 histogram-only hardware probe (opt-in)

This test targets the owner's LG OLED42C5PSA.AAUQLJD (webOS SDK 10),
which reports a 32-bit ARM EABI5 soft-float application ABI, aarch64
kernel and all four `vtCapture_*Histogram` symbols.

## What the probe does

- Calls the existing native Rust `HistogramCapture::try_new()` and
  `sample()` functions. No HyperDRM, HyperHDR or WLED is required.
- **May temporarily create** `/var/luna/preferences/vt_devmode` if absent,
  then automatically remove **only its own file** via the marker guardian.
- Acquires LG's exclusive VT histogram device for a few seconds.
  Stop competing PicCap, HyperDRM, or other capture clients first.
- Prints weighted RGB means for all 16 physical histogram regions,
  for 8 samples at a 600ms interval by default. No raw image/frame/pixel
  data is retained, no config is loaded, and **no LED/network output** occurs.
- Always requests native finalization when exiting normally.
  A native driver crash or hang cannot be ruled out until tested on hardware.
- Requires an explicit `--acknowledge-native-capture` switch. It does **not**
  automatically run, install a daemon, start on boot, or change HDCP policy.

This is a diagnostic of LG-provided **aggregate** capture measurements,
not a means of accessing DRM-decrypted frames.

## Observed C5 failure: no histogram samples yet

On 2026-10-12, the first live test on OLED42C5PSA.AAUQLJD
reported `LSRegisterPubPriv FAILED` as a C++ `std::runtime_error`
during native initialization. The CLI process terminated before the
first region was sampled. The capture library's exported histogram
symbols are real, but their runtime availability is still unproven.

A related historical case is
[hyperion-webos issue #89](https://github.com/webosbrew/hyperion-webos/issues/89),
where a native capture probe also reported `LSRegisterPubPriv FAILED`.
Its thread includes a separate application permission issue, but
**we have not confirmed the same cause on this C5**.

The revised backend adds a C++ wrapper for synchronous exceptions in
native initialization, preset, sampling and finalization; it should
return a Rust error for this specific exception instead of killing the
CLI. It does not resolve the Luna registration requirements or protect
against unrelated driver crashes.

**Do not rerun the original crashing binary.** In particular,
`scripts/provision_luna.sh` is a legacy script for a different
installed executable and uses broad Luna privileges. Do not run it as
a speculative workaround. The current standalone test path is
`/var/home/root/c5-histogram-probe`, which differs from the legacy
role's `exeName`.

Read-only TV checks:

```bash
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== marker after crash ====='
ls -l /var/luna/preferences/vt_devmode 2>&1 || true
echo '===== test binary ====='
id
ls -l /var/home/root/c5-histogram-probe
readlink -f /var/home/root/c5-histogram-probe
echo '===== competing capture processes ====='
ps | grep -Ei 'piccap|hyperdrm|histogram|lg-hue-sync' | grep -v grep || true
echo '===== Luna registration-related file names ====='
for dir in /var/luna-service2 /var/luna-service2-dev; do
  for sub in manifests.d roles.d client-permissions.d; do
    if [ -d "$dir/$sub" ]; then
      ls "$dir/$sub" | grep -iE 'capture|histogram|hyperion|hue.sync|wled' || true
    fi
  done
done
echo '===== library dependencies ====='
ls -l /usr/lib/libresource_mgr_client.so* 2>&1 || true
TV
```

No system service restart, role change or HDCP modification is
needed for these checks. Inspect any security-relevant file contents
locally before sharing them.

## Verified crash aftermath: developer marker survived (C5)

The owner subsequently observed
`/var/luna/preferences/vt_devmode` as a zero-byte, root-owned file
with mode `0600` and a creation timestamp matching the 21:41 probe;
the marker was absent in the preflight. No competing capture process
appeared in `ps`. Therefore the original guardian **did not complete
cleanup after the native abort**. We cannot determine from this evidence
whether the guardian was killed with the SSH process group or failed for
another reason.

New startup policy in the fork: **reject any pre-existing marker**,
including a possibly stale one, instead of reusing it and silently
proceeding. The guardian calls `setsid()` in the forked child before
waiting for the parent pipe to close; this reduces the chance that an
SSH process-group teardown kills cleanup. It remains best-effort, not a
guarantee against process/cgroup kills, kernel failures or driver aborts.

Only after confirming that **no** capture owners are active, an operator
may remove the *specific* leftover marker if its owner, mode, type,
size and provenance match the observed failed probe. Never remove an
unknown or actively owned marker as a blanket workaround. The safe
manual cleanup is limited to:

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
set -eu
p=/var/luna/preferences/vt_devmode
active="$(ps | grep -Ei 'piccap|hyperdrm|histogramhyperion|lg-hue-sync|c5-histogram-probe' | grep -v grep || true)"
if [ -n "$active" ]; then
  echo "$active"
  echo 'ABORT: possible capture owner running'
  exit 1
fi
if [ ! -e "$p" ]; then
  echo 'marker already absent'
  exit 0
fi
ls -l "$p"
if [ ! -f "$p" ] || [ -L "$p" ] ||
   [ "$(stat -c %u "$p")" != 0 ] ||
   [ "$(stat -c %s "$p")" != 0 ] ||
   [ "$(stat -c %a "$p")" != 600 ]; then
  echo 'ABORT: marker does not match the failed probe'
  exit 1
fi
rm -- "$p"
test ! -e "$p" && echo STALE_MARKER_CLEARED
TV
```

The Mac's previously downloaded
`~/Downloads/libvtcapture.so.1.0.0` was later missing. This is not a
C5 library issue: `/usr/lib/libvtcapture.so.1.0.0` was confirmed to exist
on the TV and can be recopied read-only. Use a dedicated local directory
so downloaded LG binaries are not confused with cross-build artifacts.
Keep vendor binaries local; share selected `gstrings` output instead.

## 2026-10-12: native strings identify LS2 identities and separate policy layers

Read-only extraction of the **on-device C5** 32-bit vendor libraries by the
owner found the following *literal strings* (not yet verified registration
requests or dynamic policy state):

**`libvtcapture.so.1.0.0`**
- `LSRegister`; `com.webos.service.capture.client`
- `com.webos.service.capturepermission`
- `Permission::registerPermissionSubscribe`,
  `Permission::processCheckPermission`, `checkPermission`, `whitelist`
- `Client not found on the whitelist.`
- `your process has no permissions!! [%s]`
- `LIB_CAPTURE_REFUSED_PERMISSION_DENIED`
- `LIB_CAPTURE_REFUSED_WATCHING_SCRAMBLED`
- `LIB_CAPTURE_REFUSED_WATCHING_SECURE_VIDEO`
- `hue_sync_service`
- `force capture allow.` (diagnostic text only; not an instruction)
- `ResourceManagerClient` and `VtDriver::initVtCapture`

**`libresource_mgr_client.so.1.0.0`**
- `com.webos.rm.client.`
- `ResourceManagerClientInit`, `ums.resource_mgr_client`,
  `com.webos.media`

These strongly motivate checking the executable-specific Luna role and the
native client's *actual* service name, but **do not establish** that
`com.webos.service.capture.client` is the exact name that failed registration
or that `hue_sync_service` is a usable entitlement. There may be
independent failures at bus registration, the capture permission whitelist,
resource-manager allocation and DRM/security policy.

**Important:** the native capture library explicitly reports refusal of
secure and scrambled video. This evidence leaves DRM-protected HDMI support
**unverified**; do not claim histogram access to protected Prime Video or
try to disable the refusal conditions.

The on-TV `vt_devmode` file was confirmed stale and removed by the owner
after checking for competing capture processes (`STALE_MARKER_CLEARED`).

### Read-only LS2 and static role discovery

```bash
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== registered capture services ====='
ls-monitor -l | grep -Ei 'capture|capturepermission|webos.rm.client' || true

echo '===== capture permission API registration ====='
ls-monitor -i com.webos.service.capturepermission 2>&1 || true

echo '===== capture API registration ====='
ls-monitor -i com.webos.service.capture 2>&1 || true

echo '===== Luna security directories ====='
for d in /usr/share/luna-service2 /usr/share/luna-service2-dev \
         /etc/luna-service2 /var/luna-service2 /var/luna-service2-dev; do
  [ -d "$d" ] && ls -ld "$d" "$d"/roles.d "$d"/manifests.d 2>/dev/null || true
done

echo '===== only exact capture-related role text matches ====='
for d in /usr/share/luna-service2/roles.d /usr/share/luna-service2-dev/roles.d \
         /etc/luna-service2/roles.d /var/luna-service2/roles.d \
         /var/luna-service2-dev/roles.d; do
  [ -d "$d" ] || continue
  grep -lE 'com\\.webos\\.service\\.capture\\.client|com\\.webos\\.service\\.capturepermission|hue_sync_service' \
    "$d"/*.json 2>/dev/null || true
done
TV
```

For reverse-engineering the failure origin, a local, private ELF
disassembly of the two vendor libraries may identify the **actual**
`LSRegister` arguments. Keep the proprietary binaries out of Git and
do not assume that a coincidental string establishes a caller relationship.

## Verified running C5 capture services (2026-10-12)

The owner's read-only `ls-monitor -l` output now shows the native
system capture process `/usr/sbin/captureservice` registering:

- `com.webos.service.capture` (dynamic service)
- `com.webos.service.capturepermission` (dynamic service)
- `com.webos.service.capture.client3002` (client-only)
- `com.webos.rm.client._fT43KOib6U5FNp` (Resource Manager client)

Resource Manager clients belonging to `extinput-integration`, `tvrm`
and `starfish-media-pipeline` are registered separately.

`ls-monitor -i com.webos.service.capturepermission` shows
`checkPermission` and `setPermission` with `provides` including
`private`, `all`, and `capturepermission.security`. This is an
**access-control group descriptor**, not proof the histogram probe is
authorized. **Do not invoke `setPermission` for this investigation.**

`ls-monitor -i com.webos.service.capture` shows
`getCapability`, `getProperties`, `getOptions`, `isLocked`,
`createHandle`, `execute`, `lock`, `unlock` and other methods
under access controls including `capture.client`. Their existence
proves registration, **not** successful standalone client registration
or a supported HDCP histogram backend.

The owner's search of a few role directories for three exact strings
produced no filename matches. This does not establish absence: older
static roles may live in `/usr/share/ls2/roles/pub/` or `prv/`, names
may be granted through terminal `*` wildcards, and a service might
have a distinct client executable identity.

### Narrow, read-only role provenance query

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== static role directory layout ====='
for root in /usr/share/ls2/roles /var/palm/ls2/roles \
            /usr/share/luna-service2/roles.d \
            /var/luna-service2/roles.d /var/luna-service2-dev/roles.d; do
  [ -d "$root" ] || continue
  find "$root" -maxdepth 2 -type d -print
done
echo '===== files mentioning native capture identities ====='
for root in /usr/share/ls2/roles /var/palm/ls2/roles \
            /usr/share/luna-service2/roles.d \
            /var/luna-service2/roles.d /var/luna-service2-dev/roles.d; do
  [ -d "$root" ] || continue
  grep -RIlE --include='*.json' \
    'com[.]webos[.]service[.]capture|capturepermission|capture[.]client|hue_sync_service|captureservice' \
    "$root" 2>/dev/null | head -50 || true
done
echo '===== system capture service executable ====='
ls -l /usr/sbin/captureservice
echo '===== probe binary not to execute ====='
ls -l /var/home/root/c5-histogram-probe
TV
```

Only filenames are collected from role matching, not confidential role
contents. This is safer than calling `checkPermission` with unverified
schema or trying to override policy. Preserve `Unknown` for HDMI
protection and do **not** rerun the native probe until the registration
name and policy path are understood.

## 2026-10-13: standard role search returned no hits

The owner enumerated the following populated/available directory paths
on the C5: `/usr/share/ls2/roles/{pub,prv}`,
`/var/palm/ls2/roles/{pub,prv}`,
`/usr/share/luna-service2/roles.d`,
`/var/luna-service2/roles.d`, and
`/var/luna-service2-dev/roles.d`. Searching their
`*.json` file contents for literal `com.webos.service.capture`,
`capturepermission`, `captureservice`, and
`hue_sync_service` returned **no matching filenames**.

**Do not infer missing permissions from this.** The search did not count
files or inspect other suffixes and directories; roles can refer to
wildcard identities or be installed as manifests elsewhere. The LS2
documentation identifies the older `pub/prv` layout but the OEM TV
may have additional security data.

The next probe is deliberately *read-only* and bounded:

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== role file inventory ====='
for dir in /usr/share/ls2/roles/prv /usr/share/ls2/roles/pub \
  /var/palm/ls2/roles/prv /var/palm/ls2/roles/pub \
  /usr/share/luna-service2/roles.d \
  /var/luna-service2/roles.d /var/luna-service2-dev/roles.d; do
  [ -d "$dir" ] || continue
  echo "--- $dir ---"
  printf 'regular_file_count='
  find "$dir" -maxdepth 1 -type f | wc -l
  find "$dir" -maxdepth 1 -type f | head -12
done
echo '===== capture-related filenames across known LS2 roots ====='
find /usr/share/ls2 /var/palm/ls2 /usr/share/luna-service2 \
  /var/luna-service2 /var/luna-service2-dev -type f \
  \( -iname '*capture*' -o -iname '*resource*' \
     -o -iname '*hue*' \) -print 2>/dev/null | head -65

echo '===== possible security log reader ====='
command -v journalctl || true
command -v logread || true
ls -ld /var/log/journal /run/log/journal 2>/dev/null || true
TV
```

If system logs are available, examine only narrow entries around the
first test at local `2026-10-12 21:41`, targeting `ls-hubd`,
`LSHUB` or `LSRegister`. Do not restart system services,
modify LS2 roles, call `setPermission` or attempt to force capture
of protected streams. No further histogram init is justified until
we can identify the failed registration or other native initialization
preconditions.

## 2026-10-13: capture-security policy files positively identified

The C5 role-file inventory resolved the misleading earlier empty-content
search. Counts of regular files/symlinks at these locations:

- `/usr/share/ls2/roles/prv`: 118
- `/usr/share/ls2/roles/pub`: 109
- `/var/palm/ls2/roles/prv`: 1
- `/var/palm/ls2/roles/pub`: 1
- `/usr/share/luna-service2/roles.d`: 311
- `/var/luna-service2/roles.d`: 6
- `/var/luna-service2-dev/roles.d`: 16

The **exact filesystem paths confirmed present** by `find` are:

- `/usr/share/luna-service2/roles.d/com.webos.service.capture.role.json`
- `/usr/share/luna-service2/client-permissions.d/com.webos.service.capture.perm.json`
- `/usr/share/luna-service2/api-permissions.d/com.webos.service.capture.api.json`
- `/usr/share/luna-service2/groups.d/com.webos.service.capture.groups.json`
- `/usr/share/luna-service2/manifests.d/screen-capture-webos.manifest.json`
- `/usr/share/luna-service2/services.d/com.webos.service.capture.service`
- `/usr/share/luna-service2/roles.d/com.webos.service.capture.testsuite.role.json`
- `/usr/share/luna-service2/manifests.d/avcaptureinf.manifest.json`
- `/usr/share/ls2/roles/prv/avcaptureinf-test.json`
- `/usr/share/ls2/roles/pub/avcaptureinf-test.json`

**Only existence is verified.** We have not inspected their JSON values,
whether the manifest actually names each file, whether their client role
permits the standalone Rust executable path, or the precise identity
which fails `LSRegisterPubPriv`.

Read-only follow-up (do not invoke vendor capture or modify permissions):

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
for F in \
  /usr/share/luna-service2/manifests.d/screen-capture-webos.manifest.json \
  /usr/share/luna-service2/roles.d/com.webos.service.capture.role.json \
  /usr/share/luna-service2/client-permissions.d/com.webos.service.capture.perm.json \
  /usr/share/luna-service2/api-permissions.d/com.webos.service.capture.api.json \
  /usr/share/luna-service2/groups.d/com.webos.service.capture.groups.json \
  /usr/share/luna-service2/services.d/com.webos.service.capture.service \
  /usr/share/luna-service2/roles.d/com.webos.service.capture.testsuite.role.json; do
  echo
  echo "===== $F ====="
  if [ -f "$F" ]; then
    ls -l "$F"
    sed -n '1,180p' "$F"
  else
    echo NOT_FOUND
  fi
done
TV
```

These files may describe other services or policies: do not grant
`all` access or use system service registration names as the new
application identity without validating the actual LS2 contract.
The C5's system capture service has separately registered successfully,
but that does **not** prove it permits a new native capture client.
Preserve capture-denial and secure-video refusal behaviour.

## Build and download

GitHub Actions on the `feature/vendor-neutral-wled-drm-first` branch runs
full Rust unit tests/Clippy and cross-compiles for
`armv7-unknown-linux-gnueabi` with GLIBC <= 2.28. After a **green**
workflow run, its **ARMv7** job attaches a short-lived
`c5-histogram-probe-armv7` artifact containing `lg-hue-sync`
(7-day retention). Download it from the workflow's Artifacts panel.

Optional GitHub CLI on Mac:

```bash
gh run download RUN_ID --repo angelcode1/lg-wled-sync \
  -n c5-histogram-probe-armv7 -D ~/Downloads/c5-histogram-bin
file ~/Downloads/c5-histogram-bin/lg-hue-sync
```

## Historical TV probe procedure — do not rerun until Luna failure resolved

The commands below document the first hardware experiment. On this C5,
that experiment failed during LS2 registration and left a stale marker.
Do **not** run it again merely because a newer binary is available.

### TV preflight and explicit hardware test (historical)

First confirm that no other capture owner is running; do not kill
TV media pipelines, `ls-hubd`, or `extinput-integration`.

```bash
ssh root@192.168.8.184 \
  'ps | grep -Ei "piccap|hyperdrm|histogramhyperion|lg-hue-sync" | grep -v grep || true; \
   ls -l /var/luna/preferences/vt_devmode 2>&1 || true'

scp ~/Downloads/c5-histogram-bin/lg-hue-sync \
  root@192.168.8.184:/var/home/root/c5-histogram-probe
ssh root@192.168.8.184 'chmod 700 /var/home/root/c5-histogram-probe'

# Start a changing scene on the Fire TV's Prime Video app through HDMI1.
ssh -t root@192.168.8.184 \
  '/var/home/root/c5-histogram-probe test-histogram \
   --acknowledge-native-capture --samples 8 --interval-ms 600' \
  | tee ~/Downloads/c5-histogram-test.log

# Inspect the marker after the test (if present before, it is left alone).
ssh root@192.168.8.184 'ls -l /var/luna/preferences/vt_devmode 2>&1 || true'
```

Expected line format:
`HISTOGRAM_SAMPLE 1/8 changed_regions=0 00=RRR,GGG,BBB ... 15=RRR,GGG,BBB`.
`HISTOGRAM_RESULT` reports the count of valid and failed samples and
how many consecutive successful pairs included at least one meaningful
(4/255 RGB channel) region change.

**Interpretation**:
- Init fails: investigate error without altering TV HDMI or DRM settings.
- Init succeeds but zero channels / samples invalid: hardware not yet
  validated; avoid publishing a calibration.
- Two or more valid samples, some changes: aggregate histogram API appears
  functional on this specific HDMI session. This does **not** identify
  screen-edge geometry, prove HDCP authentication state, or establish
  DRM-secure coverage.
- Valid but no changes: inconclusive if the scene is static; compare
  changing scenes before claiming success or failure.

Do not add a G4 spatial mapping to C5 based on the 16 region IDs.
Spatial mapping requires a separate measured perimeter calibration.

## Current DRM routing

The Fire TV Prime Video HDMI1 resource envelope is still `Unknown`:
`SVP_CPB` is absent, but that is not evidence of an unprotected HDMI
link. This hardware histogram test bypasses **only the Rust router
selection**, not any content or HDCP protection system.
