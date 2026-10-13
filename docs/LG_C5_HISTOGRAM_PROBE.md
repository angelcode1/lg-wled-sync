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

## 2026-10-13: actual C5 native LS2 capture manifest and role content

The owner supplied `c5-ls2-capture-policy.txt` copied read-only from
the TV. This **confirms**, rather than infers, the installed policy:

- `screen-capture-webos.manifest.json` has `id:
  "screen-capture-webos"`. Its `roleFiles` are
  `com.webos.service.capture.role.json` **and**
  `com.webos.service.capture.testsuite.role.json`. It also references
  the `com.webos.service.capture.service` service file,
  `com.webos.service.capture.api.json`,
  `com.webos.service.capture.perm.json` and
  `com.webos.service.capture.groups.json`.
- Capture system role: **`exeName: /usr/sbin/captureservice`**,
  `type: regular`, `allowedNames:`
  `com.webos.service.capture`,
  `com.webos.service.capture.client*`,
  `com.webos.rm.client.*`,
  `com.webos.service.capturepermission`. Service-specific outbound
  destinations include `com.webos.media` for Resource Manager clients.
- Client permission file maps
  `com.webos.service.capture`,
  `com.webos.service.capture.client*` and
  `com.webos.service.capturepermission` to `["all"]`.
  **Do not copy this broad system-service ACG assignment to our
  standalone executable.**
- API permission file groups
  `capturepermission.security` for `checkPermission` and
  `setPermission`, and `capture.client` for
  `createHandle`, `execute`, `getCapability` and related
  capture APIs. Both groups are flagged `public:false`.
- Service file maps the dynamic
  `com.webos.service.capture` and
  `com.webos.service.capturepermission` names to
  `Exec=/usr/sbin/captureservice`.

The owner's actual histogram binary is
`/var/home/root/c5-histogram-probe`, which **does not match** that
system role's `exeName`. This is strong evidence for a separate
**native caller registration role** being necessary; it is not yet
proof that this is the sole cause of
`std::runtime_error("LSRegisterPubPriv FAILED")`.

In particular, a native library registration could require a
service-specific client name distinct from the primary executable
and an independent `ResourceManagerClient` registration.
Role matching, ACG method permission and capture-content restrictions
are separate decisions. A successful call to private Luna from root
`luna-send` does not test the application binary's role.

The manifest names a second `testsuite` role that was **not in the
original upload**. That and the older
`/usr/share/ls2/roles/prv/avcaptureinf-test.json` may explain how
LG registers a *different executable* for capture diagnostics.
Inspect them **read-only**, before designing any package manifest:

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
for f in \
  /usr/share/luna-service2/roles.d/com.webos.service.capture.testsuite.role.json \
  /usr/share/ls2/roles/prv/avcaptureinf-test.json \
  /usr/share/ls2/roles/pub/avcaptureinf-test.json \
  /usr/share/luna-service2/manifests.d/avcaptureinf.manifest.json; do
  echo
  echo "===== $f ====="
  if [ -f "$f" ]; then
    ls -l "$f"
    cat "$f"
  else
    echo NOT_FOUND
  fi
done
TV
```

This read-only test does not initialise VT capture, call
`setPermission`, or modify LG-owned security config. The
`vt_devmode` marker has already been cleared after the failed probe;
do not recreate it for a permissions experiment. Do not assume that
approving a caller for Luna makes protected HDMI histograms available.

## 2026-10-13: LG native test-suite role confirms a separate client model

The owner supplied the contents of the additional installed role files,
completing the manifest's capture-test-suite reference:

`/usr/share/luna-service2/roles.d/com.webos.service.capture.testsuite.role.json`:

```json
{
  "exeName": "/usr/bin/vtCaptureTestSuite",
  "type": "regular",
  "allowedNames": [
    "com.webos.service.capture.testsuite.*",
    "com.webos.service.capture.client*",
    "com.webos.rm.client.*"
  ],
  "permissions": [
    {
      "service": "com.webos.service.capture.testsuite.*",
      "outbound": ["*"]
    },
    {
      "service": "com.webos.service.capture.client*",
      "outbound": ["*"]
    },
    {
      "service": "com.webos.rm.client.*",
      "outbound": ["com.webos.media"]
    }
  ]
}
```

This is an installed example of a **regular role for a separate test
executable**; it does not require changing or impersonating the vendor
`/usr/sbin/captureservice` role. It makes a standalone role tied to
our eventual stable executable path a plausible LS2 integration
solution. Actual native registration names and the cause of
`LSRegisterPubPriv FAILED` remain unverified.

The older `avcaptureinf-test` roles are for
`/opt/webos/tests/avcaptureinf/gtest-avcaptureinf`.
Its **private** role allows wildcard names `"*"` and wildcard
inbound/outbound permissions, while its public role allows
`gtest-avcaptureinf`; its manifest
`avcaptureinf.manifest.json` registers those two old-format roles.
**Do not copy these wildcard grants** into our application.

### Next: prove what the library registers (read-only)

First collect local filename and native symbol evidence, without
starting the vendor tests or changing the TV:

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== installed LG native test executable ====='
ls -l /usr/bin/vtCaptureTestSuite 2>&1 || true
echo '===== current root capture probe ====='
ls -l /var/home/root/c5-histogram-probe
echo '===== marker remains absent ====='
ls -l /var/luna/preferences/vt_devmode 2>&1 || true
echo '===== capture registration log availability ====='
for f in /var/log/messages /var/log/ls-hubd.log /var/log/syslog; do
  [ -f "$f" ] && ls -l "$f"
done
TV

GSTRINGS="$(brew --prefix binutils)/bin/gstrings"
"$GSTRINGS" -a ~/Downloads/c5-native-libs/libvtcapture.so.1.0.0 \
  | grep -iE -C 4 \
    'com[.]webos[.]service[.]capture[.]client|com[.]webos[.]rm[.]client|hue_sync_service|service name :|LSRegister' \
  | head -140
```

The string snippets are hints, not proof of the full LS2 name passed to
`LSRegister`; confirming the exact caller requires analysis of the
installed vendor library's instruction-level call site, or a narrowly
scoped native registration diagnostic. No request to `setPermission`
or to alter HDCP is justified by these role files.

**Do not deploy a guessed LS2 role or retry histogram capture yet.**
Any future role should use the actual stable installed executable
path and only registration names shown to be required.

## 2026-10-13: offline native ARM32 library disassembly (no TV execution)

The owner supplied a **private** tarball containing the exact installed
ELF32 ARM soft-float libraries:

| Library | ELF BuildID (SHA-1 build identity) |
|---|---|
| `libvtcapture.so.1.0.0` | `9fb5e56d59c29ed242c0e4a66015c71601a4657e` |
| `libresource_mgr_client.so.1.0.0` | `c33db0a8fa20446e45ad4f03b495ff9e3929f722` |

Both were inspected offline with `readelf`, `strings` and LLVM's
**Thumb-v7** disassembler. Most of `libvtcapture` uses Thumb instructions
(symbol entry points have odd addresses); a naive ARM-mode disassembly
gives **incorrect instructions**.

### Confirmed code-level registration chain

- `LS2Interface::attach(_GMainLoop*)` at `0x0000d558`
  calls `LS2ServiceBase::attach(_GMainLoop*)` through PLT at
  `0x0000d56a`.
- `LS2ServiceBase::attach` at `0x0000bab4`
  obtains the service name from a virtual method and calls
  **`LSRegister`** through the PLT at `0x0000bada`.
  The PLT entry at `0x9414` resolves through the ARM relocation table
  to `LSRegister`. (The adjacent entry at `0x9408` is *not*
  `LSRegister` and must not be confused with it.)
- `LS2Interface::makeServiceName` at `0x0000daec` constructs a
  name using the literal
  `com.webos.service.capture.client` at `0x0001afdc`,
  opens `/dev/urandom`, reads four bytes and reduces a generated
  numeric value modulo `10000`. It formats the numeric suffix as
  decimal text. The expected registered family is therefore
  **`com.webos.service.capture.client*`**, matching the actual
  allowedNames in LG's `vtCaptureTestSuite` role.
  The specific suffix varies and **must not be hardcoded**.
- The capture library also creates `ResourceManagerClient` instances;
  the separate `libresource_mgr_client` includes
  `com.webos.rm.client.` as a literal. The *observed* C5 Resource
  Manager bus clients have generated suffixes, aligning with the
  `com.webos.rm.client.*` test-suite role wildcard.

**Important diagnostic correction:** `LSRegisterPubPriv FAILED` is
**not present as a literal string in either uploaded library**. The
two binaries therefore do not prove which function threw the observed
C++ exception. `libresource_mgr_client` dynamically depends on
`libums_connector.so.1` and imports
`UMSConnector::UMSConnector(..., UMSConnectorBusType, bool)`.
It is plausible that the Resource Manager's UMS connector produces the
exception during its own bus registration, instead of (or in addition
to) `libvtcapture`'s direct `LSRegister` call. **Do not call
the direct capture-client role mismatch the definitive root cause yet.**

Next offline library to request, if needed:
`/usr/lib/libums_connector.so.1` (and the real symlink target).
Inspect its `LSRegisterPubPriv FAILED` string and its registration
constructor path. Do not publish its vendor binary.

**Remaining limits:** No call parameters or return status were
recorded in the original native crash. The role for our actual
`/var/home/root/c5-histogram-probe` executable is absent from
the inspected capture manifest, but other roles/policies have not
been exhaustively ruled out. Neither the bus registration outcome
nor LG's later whitelist/security-video checks have been validated
on this standalone probe. Native DRM refusal must remain intact.

### Safe offline next step

```sh
# Copy only: does not run any library code on the TV.
ssh root@192.168.8.184 \
  'ls -l /usr/lib/libums_connector.so.1; readlink -f /usr/lib/libums_connector.so.1'
scp -O root@192.168.8.184:/usr/lib/libums_connector.so.1 \
  ~/Downloads/c5-native-libs/libums_connector.so.1

GSTRINGS="$(brew --prefix binutils)/bin/gstrings"
"$GSTRINGS" -a ~/Downloads/c5-native-libs/libums_connector.so.1 \
  | grep -iE -C 3 'LSRegisterPubPriv|LSRegister|invalid permissions|UMSConnector' \
  | head -110
```

No action to edit LS2 policy, restart the Luna hub, force native
capture or bypass secure-video refusal is warranted before this
exception's origin is understood.

## 2026-10-13: UMS public connector delegates to an implementation DSO

The owner copied `/usr/lib/libums_connector.so.1` from the LG C5 and
extracted its symbols/strings. The library exposes public
`UMSConnector` and `UMSConnector_impl` C++ method signatures and names
`libums_connector_impl.so.1` in its dynamic dependencies, alongside
`libluna-service2.so.3`. This indicates a separate **implementation
DSO** beneath the UMS connector and materially narrows the potential
origin of the native `LSRegisterPubPriv FAILED` exception.

The original grep included `UMSConnector` and was limited to
`head -110`: the large number of mangled symbols therefore **cannot
show whether `LSRegisterPubPriv FAILED` is absent** later in the same
library. The callsite/throw origin is still unverified.

Next *offline* diagnostic: inspect the full exact-error match in both
UMS libraries, and copy the implementation DSO only if it exists:

```bash
GSTRINGS="$(brew --prefix binutils)/bin/gstrings"
"$GSTRINGS" -a ~/Downloads/c5-native-libs/libums_connector.so.1 \
  | grep -iE -C 2 'LSRegisterPubPriv FAILED|LSRegisterPubPriv|LSRegister|invalid permissions' \
  || true

ssh root@192.168.8.184 \
  'ls -l /usr/lib/libums_connector_impl.so.1; readlink -f /usr/lib/libums_connector_impl.so.1'
scp -O root@192.168.8.184:/usr/lib/libums_connector_impl.so.1 \
  ~/Downloads/c5-native-libs/libums_connector_impl.so.1

"$GSTRINGS" -a ~/Downloads/c5-native-libs/libums_connector_impl.so.1 \
  | grep -iE -C 2 'LSRegisterPubPriv FAILED|LSRegisterPubPriv|LSRegister|invalid permissions' \
  || true
```

If the implementation exists, inspect ELF relocations / Thumb callsites
from the copied binary privately to determine whether it constructs or
throws the actual error, and which bus name is involved. Do not upload
proprietary vendor binaries into Git, grant broad LS2 roles, or restart
the hub for this diagnostic. The capture service's DRM refusal policies
must remain intact.

## 2026-10-13: offline verification corrects UMS exception attribution

The owner reported `LSRegisterPubPriv FAILED` in a shell
`gstrings` result apparently labelled `libums_connector.so.1`.
We obtained and examined the subsequent private archive
`c5-ums-registration-analysis.tar.gz`, which contains
**`libums_connector.so.1`, exactly 30,044 bytes**, an ELF32 ARM
EABI5 soft-float shared object. Its BuildID is
`be924adbee8011018d7def541f1fe961aea0e547` and its SHA-256 is
`d030887d54de9a1da8fc024a0550a22dcff1db1b4ebb55e9151475330e3e6261`.

**Correction of the preceding conclusion:** this specific,
hash-identified binary has **no** literal `LSRegisterPubPriv FAILED`,
no dynamic `LSRegisterPubPriv` import and no
`LSRegister` import. Its dynamic `DT_NEEDED` entry includes
`libums_connector_impl.so.1`; it imports
`UMSConnector::UMSConnector_impl::UMSConnector_impl(...,
UMSConnectorBusType, bool)`. In Thumb code,
`UMSConnector::UMSConnector` at `0x358c` forwards the caller's
bus-type parameter, name, loop and context into the
implementation constructor at `0x3664` through the PLT at
`0x3020`. These are real ELF code and relocation facts from the
uploaded wrapper.

The older, separately supplied
`libresource_mgr_client.so.1.0.0` calls the public
`UMSConnector` constructor from
`ResourceManagerClientInit(UMSConnectorBusType)`:
Thumb call at `0x69a0` into PLT `0x5590`, which resolves
via GOT `0x26e78` to the constructor. The Resource Manager
library constructs a generated `com.webos.rm.client.*` name.
The bus type is forwarded by these constructors. Later offline
analysis of the default ResourceManagerClient call path (below)
shows that the native histogram path supplies numeric bus enum **0**.

Therefore the previously pasted exact error text is **not
attributable to the hash-identified wrapper**. It may have come
from `libums_connector_impl.so.1`, a different local copy of
the same basename, or pasted output that did not correspond to
the archive. **We need to verify the implementation binary
before making that attribution.**

Next read-only step: compare both local file hashes and check the
implementation for exact error strings, then privately attach its
binary (not to GitHub). No vendor service modification, permission
grant or histogram restart is indicated.

```bash
mkdir -p ~/Downloads/c5-native-libs
scp -O root@192.168.8.184:/usr/lib/libums_connector_impl.so.1 \
  ~/Downloads/c5-native-libs/libums_connector_impl.so.1

GSTRINGS="$(brew --prefix binutils)/bin/gstrings"
for LIB in libums_connector.so.1 libums_connector_impl.so.1; do
  P="$HOME/Downloads/c5-native-libs/$LIB"
  echo "===== $LIB ====="
  file "$P"
  shasum -a 256 "$P"
  "$GSTRINGS" -a "$P" \
    | grep -F -C 2 'LSRegisterPubPriv FAILED' || true
done
```

The C5's separate Luna role and native DRM/permission
refusal still apply. No native histogram samples have been
collected.

## 2026-10-13: exact C5 UMS registration-failure branch confirmed offline

Private uploaded `c5-ums-impl-analysis.tar.gz` contains the ARM32
`libums_connector_impl.so.1` (79,256 bytes; symlink target on TV
`/usr/lib/libums_connector_impl.so.1.0.0`).

- SHA-256:
  `5cb6421805371cf36202b3840c3b07af5cef013979156f198a93c700b6971d0f`
- ELF BuildID:
  `4fd4913b63cb8fb30503145b959e28d0f62a0b8a`

Used `readelf -Wr` to resolve imported symbol names against the
**actual GOT addresses** and decoded Thumb-v7 constructor branches
with `llvm-objdump`. Important ARM PLT pitfall: a nonstandard
4-byte stub around `0x6158` shifts later entries; simply assigning
successive 12-byte PLT slots **misidentifies** the function.
The actual `LSRegisterPubPriv` GOT slot is `0x22e58`, reached from
PLT `0x6168`.

### Call path and enum value

Verified in the owner's C5 vendor binaries:

1. `libvtcapture.so.1.0.0`'
   `ResourceManagerClient::init` allocates the
   `uMediaServer::ResourceManagerClient` and calls its **default
   constructor** through PLT `0x9bbc` (resolved to the
   `uMediaServer::ResourceManagerClientC1Ev` symbol).
2. The default constructor in
   `libresource_mgr_client.so.1.0.0` at `0x6c4c` explicitly
   sets `r1 = 0` at `0x6d8a`, then calls
   `ResourceManagerClientInit(UMSConnectorBusType)`
   through PLT `0x552c` at `0x6dce`.
3. `ResourceManagerClientInit` saves this enum and passes it
   into the `UMSConnector` constructor through PLT `0x5590`
   at `0x69a0`, supplying a newly constructed
   `com.webos.rm.client.*` name (random suffix).
4. The public `libums_connector.so.1` wrapper forwards the
   arguments to `UMSConnector_impl`. The latter constructor is at
   `0x81f8` in `libums_connector_impl.so.1`.
5. In the implementation constructor, at `0x85b2` it checks
   enum `2`, at `0x85ba` enum `3`. For enum **0**, it takes
   the `LSRegisterPubPriv` branch with its third argument `0`
   at `0x85d8`. Enum `2` and enum `3` take separate
   PalmService and `LSRegister` paths, respectively.
   These are **numeric ABI values**, not externally verified
   enum label names.

### Exact error construction

The return value from `LSRegisterPubPriv` is tested at
`0x85e0`. A false result branches at `0x85e2` to
`0x9514`, the failing registration's cleanup/error block.
The literal `LSRegisterPubPriv FAILED` resides at
`0x108f4`. A PC-relative literal at `0x997c`
resolves from `0x959c` to that exact string; the code at
`0x959e` constructs `std::runtime_error` from it,
and `0x95ae` invokes `__cxa_throw`.

This is a **direct static match** to the TV's observed exception:

```text
terminate called after throwing an instance of 'std::runtime_error'
  what(): LSRegisterPubPriv FAILED
```

The implementation also holds
`LSRegisterPubPriv FAILED for name=%s !!` at `0x108cc`,
but the original crash output did **not** include a formatted name
or hub `LSError` details. The generated name family is
`com.webos.rm.client.*`; the one specific generated suffix was
not recorded. Do **not** claim a precise hub rejection reason.
Role/path mismatch remains a strong **hypothesis**, not a
directly captured `ls-hubd` verdict.

### Implication: distinct LS2 names, independent checks

The existing LG capture testsuite role grants two registrations:
`com.webos.rm.client.*` for the Resource Manager, and
`com.webos.service.capture.client*` for the VT
capture LS2 client. Its `exeName` is
`/usr/bin/vtCaptureTestSuite`. The system
`captureservice` role is also executable-specific.
Neither applies by path to
`/var/home/root/c5-histogram-probe`.

**Safer next engineering step (not deployed):** define an
application-specific regular LS2 role for the exact **stable installed
binary path**, allowing only the two verified dynamic registration
families. Start with outbound
`com.webos.media` for `com.webos.rm.client.*`, and
explicitly enumerate other destinations observed from
`libvtcapture` for the capture client rather than copying
LG's test-suite `outbound:["*"]`. Do not add
`capturepermission.security` or `["all"]` to the new
client without a separate permissions audit.

Before the next native histogram run, prefer a tiny
**registration-only** diagnostic for the Resource Manager identity
that prints `LSError` and then unregisters. It must neither
touch `vt_devmode` nor initialise VT capture. Its exact
executable path/role installation mechanism must be validated first;
the TV has **not** run such a test yet.

Any success only verifies LS2 registration, **not** native
whitelisting, video-permission checks or protected-HDMI capture.
Keep secure/scrambled video refusal intact and HDMI DRM state
`Unknown`.

## Registration-only diagnostic implementation (2026-10-13)

An executable-path-matched, **opt-in LS2 registration-only** diagnostic
has now been added to `src/ls2_probe.rs` and exposed as:

```bash
lg-hue-sync test-ls2-registration --acknowledge-luna-registration
```

It does **not** initialise native video capture or create the
`vt_devmode` marker. The proposed regular role is staged, **not
installed**, at `security/c5-registration-only/`. Do not use
the legacy broad `scripts/provision_luna.sh` on C5.
For the exact preflight, output interpretation, owner-controlled
role-review procedure and rollback considerations, see
[LG_C5_LS2_REGISTRATION_ONLY.md](LG_C5_LS2_REGISTRATION_ONLY.md).

No registration-only result has been observed on the TV yet.
The earlier histogram failure still remains unresolved until
the hub's `LSError` can be captured.

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
