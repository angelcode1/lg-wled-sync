# LG C5: registration-only Luna diagnostic (no capture)

Status: **code and role fixture staged in Git; NOT installed or run on the TV**.
The native histogram has not produced a sample. HDMI HDCP is still `Unknown`.

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
