# C5 capture service: `execute` lifecycle and permission verification

Status: **protocol pattern discovered from another webOS client; NOT YET VERIFIED as a functioning session on OLED42C5PSA webOS 10**.

## Facts already grounded on this C5

- `/usr/sbin/captureservice` is active and offers `com.webos.service.capture`.
- C5 LS2 file `/usr/share/luna-service2/groups.d/com.webos.service.capture.groups.json` associates `execute`, `executeOneShot`, `createHandle`, `destroyHandle`, `setProperties`, `setOptions`, `setOutput` and related methods with **private** `capture.client`.
- `lightingcontroller` has its **own** capture-client role. It does not confer rights on our Rust executable.
- Our root-shell `luna-send -i -f` test and the Rust `test-blended` subprocess both successfully invoked **`executeOneShot`** at 320×180. They do **not** demonstrate access for an arbitrary registered native LS2 client, nor an `execute` session.
- `getCapability {}` yielded only `{"returnValue":true}`, with no capability data. The installed binary routes its capability handler into a short generic handler; that ACK does not verify formats, rates, or authorized handles.
- No persistent `/dev/video60` owner was seen during monitored BLENDED screenshots; brief opens and other execution modes are not ruled out.

## 2026-10-18 C5 read-only inventory — completed

The owner ran `scripts/c5-execute-lifecycle-inventory.sh` successfully
at 16:48:29 AEST on the real TV; no capture or handle was created by this test.

- `captureservice` ELF SHA256:
  `8af5e7e9971867464ce650239f6176c44437facc5ea03459df9826d7192da684`
  (size 173784 bytes).
- `lightingcontroller` ELF SHA256:
  `34de65f1f49949d9f7b9041b90e4de79539cbd1beedb5272b069ff5153342fd0`
  (size 227556 bytes).
- Native LS2 registry lists `createHandle`, `setProperties`,
  `setOptions`, `setOutput`, `execute`, `destroyHandle`, and
  `executeOneShot` under non-public `capture.client`.
  Availability of API methods **does not grant** a client rights to call them.
- Root service `captureservice` was PID 4795 and held `/dev/fb0`,
  `/dev/fb1`, `/dev/fb3`. No `/dev/video60` FD was observed at the
  instant of sampling; transient opens or other clients aren't ruled out.
- The inventory's role-name/content grep matched
  `com.webos.lunasend.role.json` and `com.webos.lunasendpub.role.json`,
  **not** an explicit role for the experimental Rust binary. This is not
  an exhaustive LS2 permission audit: generic rules may exist.
- `/usr/bin/node` is available but the default Node module resolver
  reports `PALMBUS_MODULE=UNAVAILABLE`; the native
  `/usr/bin/luna-send` binary is available. We must not conflate the
  native command's privilege identity with the Rust binary.
- `lightingcontroller` and `captureservice` roles can register LG-owned
  `com.webos.service.capture.client*` names. Our daemon must **not**
  register those reserved names or borrow those roles.

### Next non-capture permission probe

The existing `test-ls2-registration` diagnostic previously offered reserved
Resource Manager and capture client names. This is unnecessary for the
C5 capture-service investigation and would confuse service-name registration
with method authorization. It was changed to **anonymous LSRegister(NULL)**
followed immediately by LSUnregister, in the same process:

```sh
/tmp/lg-hue-sync-blended-test test-ls2-registration \
  --acknowledge-luna-registration
```

Use the **newly rebuilt binary** (not the prior 7.3 MiB binary, which has
different test semantics). This makes no capture calls and produces no
persistent service. Failure proves this executable could not complete the
anonymous registration in its current environment; success proves only
anonymous LS2 connectivity, **not capture.client access**. It does not
call `createHandle` or request an image.

After an anonymous registration succeeds, the next source-controlled test
should use that **same LS2 handle** to call a read-only capture method,
and report whether LS2 authorizes or denies it. The direct LS2 RPC method,
callback and GLib loop must be verified against installed webOS 10 ABI before
adding this test. If a private method is denied, keep the known-working,
bounded `executeOneShot` subprocess backend rather than editing roles or
using a privileged LG client name.

## 2026-10-18 native Rust LS2 registration result — denied

Owner ran the new safely scoped anonymous LS2 diagnostic, with the exact
unprivileged binary executable identity. Actual output:

```text
LS2_ANONYMOUS_ATTEMPT executable=/tmp/lg-hue-sync-ls2-test private_bus=true capture=false
Error: LS2_ANONYMOUS_REJECTED code=-1027 message=Invalid permissions for (null)
```

**Conclusions:** LS2's hub rejected `LSRegister(NULL)` for this specific
standalone executable path and current runtime security environment. It
did not reach `LSUnregister` because no handle was granted. No
`createHandle`, `execute`, screenshot, VT driver or LS2 privilege mutation
was attempted. Unlike the shell's `luna-send`, the Rust test binary has no
established executable role; lack of a matching role is a strong explanation,
**not proven the unique cause**. This failure does *not* tell us whether an
explicitly installed, correctly permissioned native service could register,
or whether `capture.client` method access would be granted. It is incorrect
to repeat the same test with an LG-owned name or altered role.

WebOS's documented LS2 configuration model binds a native `role.json`'s
`exeName` to an absolute executable path and its `allowedNames` /
outbound permissions. Its access control groups are separately specified
through `client-permissions.d`. Root does not automatically grant our Rust
executable `capture.client` rights. Reference:
https://www.webosose.org/docs/guides/development/configuration-files/security-guide/

**Decision:** retain the known-working `executeOneShot` subprocess
fallback, which runs under the existing `luna-send` identity and
conditionally requires a freshly verified `Clear` media state. The
persistent `execute` path is **blocked for the standalone Rust client**
until a legitimate least-privilege application-specific LS2 role is
designed, reviewed, installed separately with owner approval, and verified
through a non-capture access check. No system LG role edits, wildcard
`all` grants, or identity impersonation.

Next safe inspection: read the TV's actual
`/usr/share/luna-service2/roles.d/com.webos.lunasend.role.json`
metadata, and check installed third-party LS2 role directories
`/var/luna-service2` / `/var/luna-service2-dev` by **name/metadata only**
to map policy lookup (no policy changes). The legacy webOS dashboard app previously declared
`requiredPermissions: ["all"]` even though its single HTML entry point
only checks the local HTTP daemon and navigates to it. The development
branch now **omits that unnecessary request**. This changes the
front-end app manifest only; it does **not** grant any Luna permissions
to our native Rust executable or prove the private capture APIs work.
The dashboard change needs validation on real webOS after packaging.

## Evidence from other webOS implementations (not C5 observations)

[webos-iptv-player capture client](https://github.com/lennylxx/webos-iptv-player/blob/7d8fd30e927ee9d09ba7979b053f9a25ac932420/scripts/tv-capt-remote.js) contains a concrete reusable capture-handle lifecycle:

1. Open a persistent `palmbus.Handle` (Luna client connection).
2. `createHandle {}` -> save returned `handle`.
3. `setProperties {handle, properties:{method,width,height,format:"JPEG"}}`.
4. `setOptions {handle, options:{jpegQuality}}`.
5. `setOutput {handle, file:{path}}`.
6. For each screenshot, call `execute {handle}`, wait for the reply, and read the output file.
7. `destroyHandle {handle}` before releasing the client.

Importantly **`execute` is one request per frame in this example, not documented automatic 30 FPS streaming**. Frame cadence is software-driven. File delivery is shown; zero-copy shared-memory delivery is not established by this implementation. Reusing handle/client/encoder state might be lighter than repeated `executeOneShot` subprocesses, but that performance gain must be measured on the C5.

[Older API inventory](https://github.com/rhinoswirl/webostv-research/blob/cbbd676bb572be2b6eecd3bb5e0ee655da9ef120/apis/com.webos.service.tv.capture.md) lists the same family under a legacy service name. It is not a current webOS 10 guarantee.

## Safe staged verification (no WLED or permanent service needed)

**Phase A: zero capture and zero role changes.** Collect the read-only LS2 inventory:

```sh
ssh root@192.168.8.184 'sh -s' < scripts/c5-execute-lifecycle-inventory.sh
```

The script prints only relevant policy excerpts, hashes of existing executables, video/fb/dri FD names and whether a local `node` / `palmbus` client is available. It does not invoke `execute`, `executeOneShot`, `createHandle`, or any capture permission mutator. Review the resulting report before sharing.

**Phase B: single native Luna connection, no pixels.** Only if Phase A shows a legitimate usable client path, create one handle and then immediately `destroyHandle` within the **same registered connection**, logging its return codes and confirming the service remains healthy. Do not fake `lightingcontroller`'s identity, register under privileged LG wildcard names, or edit LS2 roles/whitelists. Test explicit cleanup and transport-disconnect behaviour separately; the implementation's orphan-handle cleanup is not yet proven. **Not implemented or authorized by Phase A.**

**Phase C: one known-clear frame.** With the same permitted client and an independently classified unprotected local video, set 320×180 `BLENDED` / JPEG output to a private temporary path. Call `execute` once, validate dimensions and output bytes, `destroyHandle` in a finally-like cleanup path. Delete the temporary file and do not retain image data. Do not call `execute` on protected/unknown content.

**Phase D: bounded performance comparison.** Only after safe cleanup is verified, compare five sequential `execute` calls at 320×180 against `executeOneShot`; report capture interval, PNG/JPEG decode time, host Rust process CPU, capture-service CPU sampled separately, error/timeout rates, and private temporary-file lifetime. Do not equate requested FPS or light-output loop FPS with actual new capture frames. No parallel handles.

## Architecture decision

- Keep `executeOneShot` as **known-working** guarded fallback. It may be slower, but runs without direct VT memory pointer access.
- Prefer the `execute` session only once actual C5 authorization, handle cleanup and measured benefit are verified. Do not assume native access merely because the method exists.
- Keep native `libvtcapture` behind its no-dereference guard on this fingerprint. A source-level `release_frame()` no-op in [webosbrew/hyperion-webos](https://github.com/webosbrew/hyperion-webos/blob/master/unicapture/backends/libvtcapture.cpp) does not establish that the returned V4L2 buffer belongs to our Rust process.
- Avoid user-managed codec streams, firmware modifications and all broad Luna permission grants.

Next input: review the read-only client-role metadata; anonymous LS2 registration from the temporary Rust executable is already known to be rejected and should not be retried.
