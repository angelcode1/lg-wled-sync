# LG C5: registration-only Luna diagnostic (no capture)

Status: **both independent LS2 name families now register and unregister successfully** on the owner's C5 after installing the reviewed stage-two regular role. Both were previously rejected individually with `-1027 Invalid permissions`. The native histogram has not produced a sample. HDMI HDCP remains `Unknown`.

## Purpose and provenance

Static ARM32 disassembly of the **owner-supplied C5** native libraries shows:

1. `libvtcapture` creates the default `uMediaServer::ResourceManagerClient`.
2. It passes bus type **0** through `UMSConnector` to `UMSConnector_impl`.
3. That implementation calls `LSRegisterPubPriv`. On failure, it throws
   the exact `std::runtime_error("LSRegisterPubPriv FAILED")` observed on
   the TV. The generated registration family is `com.webos.rm.client.*`.
4. LG's `vtCaptureTestSuite` role grants that family to a **different
   executable**. This does *not* authorise our test binary.

We still lack the Luna hub's actual rejection reason. This diagnostic is
intended to gather that reason independently of video capture.

## What was added

- CLI in the **existing** Rust `lg-hue-sync` binary:
  `test-ls2-registration --acknowledge-luna-registration`
- The subcommand dynamically opens `libluna-service2.so.3`, generates a
  process-unique service name `com.webos.rm.client._lghuesyncprobe<PID>`,
  attempts `LSRegisterPubPriv(name, ..., public_bus=false, ...)`, prints
  the `LSError` code/message on failure, and calls `LSUnregister` on
  success.
- It does **not** load `libvtcapture`, `libums_connector` or
  `libresource_mgr_client`; it does not create `vt_devmode`, initialise
  the VT device, call media/capture methods, save frames or send LED output.
- A **registration-only** manifest and role are staged under
  `security/c5-registration-only/` for human inspection:
  - `exeName: /var/home/root/c5-histogram-probe`
  - `type: regular`
  - `allowedNames: ["com.webos.rm.client.*"]`
  - `outbound: ["com.webos.media"]` for that single name family.
  - No `"all"`, `"capture.client"`, `"capturepermission.security"`,
    `"com.webos.service.capture.client*"` or wildcard outbound.
  - No client permission groups are granted.

This fixture can only test Resource Manager LS2 registration; it is
**not** a working histogram/capture role and should never be presented
as one. The exact role path must match the on-TV executable path; if
the application moves, regenerate and review it first.

The pre-existing `scripts/provision_luna.sh` is a **legacy C1 script
with broad privileged roles** and must **not** be used for C5. The
branch's `scripts/deploy.sh` no longer automatically invokes it or
reinstalls it at boot. This is a branch behaviour change for legacy
users and is intentional to fail closed.

## Offline verification (Mac / CI)

```bash
python3 -m unittest discover -s scripts -p test_c5_ls2_role.py -v
python3 -m json.tool security/c5-registration-only/role.json >/dev/null
python3 -m json.tool security/c5-registration-only/manifest.json >/dev/null
```

The CI test checks the exact executable, registered name family,
outbound destination and absence of broad grants.

## 2026-10-13: actual C5 registration-only baseline result (verified)

The owner downloaded the CI-tested ARMv7 build from run
`38334251772` (Rust tests/Clippy and ARMv7/glibc 2.28 jobs both
**passed**) and installed it at the exact role candidate path,
`/var/home/root/c5-histogram-probe`, after backing up the
previous binary to `c5-histogram-probe.pre-ls2`.
No security role was added. The on-TV output was:

```text
LS2_PROBE_ATTEMPT executable=/var/home/root/c5-histogram-probe name=com.webos.rm.client._lghuesyncprobe19292 private_bus=true capture=false
Error: LS2_PROBE_REJECTED name=com.webos.rm.client._lghuesyncprobe19292 code=-1027 message=Invalid permissions for com.webos.rm.client._lghuesyncprobe19292
```

This **directly establishes rejection of a generated Resource Manager
LS2 private-bus name by the current C5 hub policy** for this binary.
Unlike the previous `LSRegisterPubPriv FAILED` C++ exception, we now
have the LS2 rejection code and named caller. It is strong evidence
for the missing application-specific registration role, but the hub
message alone does not reveal every policy evaluation stage.

The registration-only test did not load VT or touch
`vt_devmode`. Do **not** rerun `test-histogram` or grant
capture ACG groups on the basis of this result.

### Controlled role installation experiment — owner must run explicitly

The two reviewed fixture files are
`security/c5-registration-only/{role,manifest}.json`; their content
was checked by CI. The following installs only these two diagnostic
files into the already-observed **development** LS2 directories, not
the stock `/usr/share` policy directories. This **changes the TV's
LS2 authorisation**, so review the contents before running it. Do
not use `scripts/provision_luna.sh`.

Get the known-good fixture revision (the exact tested commit, not a
moving branch) into a local directory:

```bash
mkdir -p ~/Downloads/c5-ls2-role-test
BASE=https://raw.githubusercontent.com/angelcode1/lg-wled-sync/53cd388d2bacda17b1d02314621d8f23b8fd1938/security/c5-registration-only
curl -fL "$BASE/role.json" -o ~/Downloads/c5-ls2-role-test/role.json
curl -fL "$BASE/manifest.json" -o ~/Downloads/c5-ls2-role-test/manifest.json
python3 -m json.tool ~/Downloads/c5-ls2-role-test/role.json
python3 -m json.tool ~/Downloads/c5-ls2-role-test/manifest.json
```

The only registration allowlist entry must be
`com.webos.rm.client.*`, exact executable
`/var/home/root/c5-histogram-probe`, type `regular`,
and Resource Manager outbound destination `com.webos.media`.
There is no `"all"` grant, capture role, wildcard outbound or
client permission file.

Stage files in the root user's private home; the `scp` does **not**
put them into a policy directory:

```bash
ssh root@192.168.8.184 'mkdir -p /var/home/root/c5-ls2-role-stage && chmod 700 /var/home/root/c5-ls2-role-stage'
scp -O ~/Downloads/c5-ls2-role-test/role.json ~/Downloads/c5-ls2-role-test/manifest.json \
  root@192.168.8.184:/var/home/root/c5-ls2-role-stage/
```

Owner-controlled apply-and-retake-baseline; **abort if files already
exist** (avoid clobbering unrelated roles). Do not reboot or restart
`ls-hubd`.

```bash
ssh root@192.168.8.184 'sh -s' <<'TV'
set -eu
R=/var/luna-service2-dev/roles.d/org.webosbrew.lg-hue-sync.c5-rm-probe.role.json
M=/var/luna-service2-dev/manifests.d/org.webosbrew.lg-hue-sync.c5-rm-probe.manifest.json
S=/var/home/root/c5-ls2-role-stage
BIN=/var/home/root/c5-histogram-probe

test -f "$BIN" && test -x "$BIN"
test ! -e "$R" && test ! -L "$R"
test ! -e "$M" && test ! -L "$M"
test -d /var/luna-service2-dev/roles.d
test -d /var/luna-service2-dev/manifests.d
test -s "$S/role.json" && test -s "$S/manifest.json"
test ! -e /var/luna/preferences/vt_devmode

umask 022
cp "$S/role.json" "$R"
cp "$S/manifest.json" "$M"
chmod 644 "$R" "$M"
cmp "$S/role.json" "$R"
cmp "$S/manifest.json" "$M"

echo '===== Trigger LS2 service/policy rescan ====='
ls-control scan-services
echo '===== Repeat registration-only probe ====='
"$BIN" test-ls2-registration --acknowledge-luna-registration
TV
```

`ls-control scan-services` is the documented mechanism for loading
installed LS2 policies, though its behaviour for *manually copied*
files may differ by OEM/build. If the new test still reports `-1027`,
**do not broaden permissions**: inspect the scanner output and the
manifest loading/role-path behaviour next. A role file existing on
disk does not prove the hub loaded it. Use the installed manifest's
absolute `roleFiles` path when auditing. The upstream OSE security
guide has an `opkg` caveat, so re-scan success alone is not proof
of a manually installed role being active.

Rollback applies only to these specific newly created files. On TV,
first verify their contents match the staging copies, then:

```sh
R=/var/luna-service2-dev/roles.d/org.webosbrew.lg-hue-sync.c5-rm-probe.role.json
M=/var/luna-service2-dev/manifests.d/org.webosbrew.lg-hue-sync.c5-rm-probe.manifest.json
S=/var/home/root/c5-ls2-role-stage
cmp "$S/role.json" "$R" &&
cmp "$S/manifest.json" "$M" &&
rm -- "$M" "$R" &&
ls-control scan-services
```

A restart/reboot might be required on some OEM implementations before
removal takes effect; do not assume a file removal instantly revokes
an already loaded hub role. Do not remove any other security file.

## 2026-10-13: post-role A/B result — Resource Manager registration now succeeds

The owner **explicitly installed the two scoped development-policy
files** and invoked `ls-control scan-services` on the rooted C5.
The rescan printed:

```text
telling hub to reload setting and rescan all directories
```

Without replacing the executable, the repeated registration-only
test printed:

```text
LS2_PROBE_ATTEMPT executable=/var/home/root/c5-histogram-probe name=com.webos.rm.client._lghuesyncprobe20872 private_bus=true capture=false
LS2_PROBE_OK name=com.webos.rm.client._lghuesyncprobe20872 registered=true unregistered=true capture=false
```

**Confirmed controlled A/B:** before the newly installed role, the
same executable was rejected with `-1027 Invalid permissions`;
after the role was loaded, the matching Resource Manager name
registered and unregistered successfully. Thus the narrowly scoped
role **sufficed to resolve this registration blocker** on the
tested TV. This is stronger than the earlier static ELF hypothesis.

The exact `/var/luna-service2-dev` role and manifest now **exist
on the owner's TV**. Do not reinstall them with create-only commands
that abort if paths already exist. The step-1 registered name prefix
is `com.webos.rm.client.*` only; the second
`com.webos.service.capture.client*` registration is **not yet
authorised or independently tested**. The result is not evidence that
the native histogram device, capture permission service or protected
HDMI sampling will work. No VT marker was created by the test.

The next experiment should add a *separate registration-only test
mode* to the **same Rust executable** using the native VT client
name family and `LSRegister`, plus an optional, separately reviewed
stage-2 regular role with only that additional registration name.
Do not broaden outbound permissions or invoke `test-histogram`.

## Stage 2: capture-client registration-only diagnostic (code staged, NOT run)

After the successful Resource Manager registration A/B test, the
`lg-hue-sync` CLI gained:

```sh
/var/home/root/c5-histogram-probe test-ls2-registration \
  --acknowledge-luna-registration --capture-client
```

Unlike the default Resource Manager mode (which uses
`LSRegisterPubPriv` with the private-bus argument), this flag
constructs `com.webos.service.capture.client<decimal>` and calls
`LSRegister`, matching the independent registration family
and function found in the C5 native `libvtcapture` ELF.
It immediately unregisters upon success. It does **not**
call `vtCapture_initHistogram`, access HDMI or touch the
`vt_devmode` developer marker.

The native suffix is generated from `/dev/urandom` modulo 10,000;
the diagnostic uses `PID % 10000` as a repeatable numeric test name.
A name collision is unlikely but possible; registration outcomes
must still be interpreted using the returned `LSError`.

**First, test with the currently installed stage-1 Resource Manager
role still active.** Use a fresh, fully passed ARMv7 CI build
containing the new `--capture-client` flag, deploy it to
`/var/home/root/c5-histogram-probe` with a backup, then verify
the previously working Resource Manager test still succeeds and
the capture-client mode's result. The baseline may be
`LS2_PROBE_REJECTED code=-1027`, since the current role does
not allow this second name. Do not guess the outcome.

A potential replacement for the **existing application-specific
role**, but **not installed**, is staged at:

`security/c5-registration-only/stage2-role.json`

It retains the Resource Manager name and
`outbound: ["com.webos.media"]`, adds only
`com.webos.service.capture.client*` to `allowedNames`,
and gives that new registration identity `outbound: []`:
no extra service calling rights or API permission groups.
This is a diagnostic-only **candidate**; whether this OEM hub
accepts an empty outbound list must be empirically established.

Do not create a second role with a duplicate `exeName`.
If a stage-2 policy change is warranted based on the new baseline,
safely back up the existing diagnostic role and **replace only
that exact owned development-policy file**, retain the original
manifest path, invoke `ls-control scan-services`, and repeat
both registration-only subcommands. Confirm that the role
being replaced exactly matches the stage-1 fixture;
otherwise abort. Do not modify LG's `/usr/share` roles
or copy the vendor test suite's `outbound: ["*"]`.

**Important:** even two successful registration tests do not
verify the capture permission whitelist, policy callbacks,
16-region histogram ABI, edge calibration or HDMI secure-video
availability. The histogram probe remains blocked pending
separate engineering review.

## 2026-10-13: stage-two LS2 registration A/B — both PASS

The owner verified the installed stage-one role matched the reviewed
fixture, `vt_devmode` was absent, and a backup was created before
atomically replacing only the owned development role with
`stage2-role.json`. The manifest remained unchanged.
`ls-control scan-services` printed:

```text
telling hub to reload setting and rescan all directories
```

The registration-only output (under the exact executable path
`/var/home/root/c5-histogram-probe`) was:

```text
LS2_PROBE_ATTEMPT kind=resource-manager executable=/var/home/root/c5-histogram-probe name=com.webos.rm.client._lghuesyncprobe29136 private_bus=true capture=false
LS2_PROBE_OK name=com.webos.rm.client._lghuesyncprobe29136 registered=true unregistered=true capture=false
LS2_PROBE_ATTEMPT kind=capture-client executable=/var/home/root/c5-histogram-probe name=com.webos.service.capture.client9141 private_bus=true capture=false
LS2_PROBE_OK name=com.webos.service.capture.client9141 registered=true unregistered=true capture=false
RESOURCE_MANAGER_EXIT=0
CAPTURE_CLIENT_EXIT=0
STAGE2_LS2_REGISTRATION_PASS
```

Confirmed: **both distinct LG native registration families** are
authorised by an executable-specific `regular` LS2 role. The
capture-client identity has `outbound: []`; the Resource Manager
identity retains only `com.webos.media`. No `["all"]` or
capture API permission group was added. The stage-two role and
unchanged manifest are **currently present on the TV**.

This establishes registration/unregistration only, **not** permissions
to call capture APIs, capture-service application whitelisting,
`vtCapture_initHistogram` success, secure/scrambled video access,
calibration, or content from HDMI. Keep the native DRM refusal path
intact.

### Native VT hardware gate before any additional test

The histogram probe implementation's original marker guardian used
`poll(..., 10_000)`, which could remove `vt_devmode` while
slow native initialization remained active. Commit
`b60710570348bd4b7bec730b715f2a067c886d3e` changes this to
wait for pipe closure on normal shutdown or parent death, handling
`EINTR`. **Use a new CI-green ARMv7 binary containing that fix**
before any future histogram run; the on-TV stage-two registration
binary predates it.

If a hardware test is later justified, use only an explicitly
unprotected TV-generated source (not Prime Video or a protected HDMI
session), with no competing VT owner, no pre-existing marker,
two bounded samples, no lighting output, and a post-run marker
check. Respect all `LIB_CAPTURE_REFUSED_*` outcomes and do not widen
capture service permissions to overcome those refusals. Capture
source suitability and device state must be checked separately;
no hardware test is claimed in this document.

## Read-only C5 preflight (no registration)

```bash
ssh root@192.168.8.184 'sh -s' <<'TV'
echo '===== exact probe executable ====='
ls -l /var/home/root/c5-histogram-probe
echo '===== no competing capture processes ====='
ps | grep -Ei 'piccap|hyperdrm|histogramhyperion|lg-hue-sync' | grep -v grep || true
echo '===== native marker must be absent ====='
ls -l /var/luna/preferences/vt_devmode 2>&1 || true
echo '===== do not overwrite an existing C5 role or manifest ====='
for f in \
 /var/luna-service2-dev/roles.d/org.webosbrew.lg-hue-sync.c5-rm-probe.role.json \
 /var/luna-service2-dev/manifests.d/org.webosbrew.lg-hue-sync.c5-rm-probe.manifest.json; do
  ls -l "$f" 2>&1 || true
done
TV
```

**Stop** if the marker is present, if another capture owner is active,
or if either diagnostic security file already exists unexpectedly.

## Owner-controlled two-step smoke test (not executed)

**Only after ARMv7 CI passes**, download the verified `lg-hue-sync`
binary from the `c5-histogram-probe-armv7` GitHub Actions artifact.
Install it at the *exact path* named in the role after confirming no
existing process uses that path. Keep a backup of the earlier binary.
Do not run `test-histogram` during this experiment.

First, run the registration-only subcommand **before installing any
new role**:

```bash
ssh root@192.168.8.184 \
  '/var/home/root/c5-histogram-probe test-ls2-registration --acknowledge-luna-registration'
```

A result like `LS2_PROBE_REJECTED name=... code=... message=...`
provides the missing hub-visible reason. Success at this stage means
the system already permits registration and role installation may not
be necessary. In either case, `LS2_PROBE_OK` only proves an LS2
register/unregister round trip—**not** media authorization or capture.

A *separate owner decision* is required before placing the staged
role and manifest under `/var/luna-service2-dev` and requesting an
`ls-control scan-services`. That action changes the TV's Luna
security configuration and should never happen automatically during
a diagnostic build. Confirm whether the installed webOS 10 hub
supports manual manifest loading, and inspect the exact role and
filename paths first. Do not overwrite other roles. If a proposed
role fails validation, do not weaken it to wildcard outbound or
`type:privileged`; investigate the error.

## Interpreting outputs

| Diagnostic outcome | Meaning |
|---|---|
| `LS2_PROBE_REJECTED` | Direct LS2 registration was refused; code and message are evidence for next step |
| `LS2_PROBE_OK` | Only the private-bus registration/unregister operation succeeded |
| `LS2_PROBE_UNREGISTER_FAILED` | Registration succeeded, but cleanup failed; inspect live service before another probe |
| Library unavailable / symbol mismatch | The TV's Luna C ABI differs or dependency is missing |
| `test-histogram` remains unrun | Correct; this test must not activate the VT capture device |

Any subsequent VT capture work requires independent permission,
whitelist and secure/scrambled-media checks. Never change HDCP,
remove native DRM refusals or assume HDMI capture is authorized.

## References

- Installed C5 `screen-capture-webos` manifest and native
  `vtCaptureTestSuite` role (copied read-only by the owner).
- webOS OSE Luna service security guide:
  https://www.webosose.org/docs/guides/development/configuration-files/security-guide/
- webOS OSE public Luna C registration API:
  https://github.com/openwebos/luna-service2/blob/master/include/public/luna-service2/lunaservice.h
- Supporting ARM disassembly and offsets:
  [LG_C5_HISTOGRAM_PROBE.md](LG_C5_HISTOGRAM_PROBE.md).
