# C5 LS2 Stage 1 — executable-specific registration (NOT installed)

## Evidence and purpose

On 2026-10-18 the user's real TV reported:

```text
/usr/bin/luna-send: privileged; allowedNames ["*"]; outbound ["*"]
/usr/bin/luna-send-pub: devmode; allowedNames ["*"]; outbound ["*"]
Existing matching custom lg-hue-sync roles: none
Rust anonymous LSRegister(NULL): -1027 Invalid permissions for (null)
```

Therefore the existing BLENDED screenshot subprocess succeeds under the
separate, privileged `luna-send` identity. It does not establish permission
for a persistent Rust-native capture session. The failure to register an
anonymous Rust client with no role is expected under LS2 security but can
have additional causes. We should test an independent **owned application
name** before considering even a read-only service RPC.

The three JSON files here are an **uninstalled** least-privilege candidate for
that first registration-only test, and must not be confused with LG's roles.
The TV's webOS 10 implementation still needs validation.

- Executable pinned to `/var/home/root/c5-ls2-probe`. No other path or
  symlink is automatically authorized.
- Own name: `org.webosbrew.lg-hue-sync.c5-probe`.
- Role type `regular`, trustLevel `dev`.
- Exact allowed name only, `inbound: []`, `outbound: []`.
- The independent client-permission file maps only this own name to an **empty ACG array** `[]`.
- **No `capture.client`, `capturepermission.security`, `all`,
  `com.webos.service.capture.client*`, VT permissions, or wildcard calls**.
- No registered service file, installed daemon, startup hook, or WLED config.

The corresponding Rust command is:

```sh
/var/home/root/c5-ls2-probe test-ls2-registration \
  --acknowledge-luna-registration --named-probe
```

That one-shot command uses `LSRegister("org.webosbrew.lg-hue-sync.c5-probe")`
then `LSUnregister`; no screenshot, service `createHandle`,
`execute`, or media query. It does **not** prove API access even if it
succeeds. The older `test-ls2-registration` command without
`--named-probe` remains the anonymous control, which has already failed
as `-1027` on the C5.

## Prerequisites before any installation (owner approval required)

1. Confirm the rebuilt ARMv7 artifact CI passed, and stage a copy of that
   binary at the exact path above. Do not modify the current known-working
   test binary or install a background service.
2. Inspect all three staged JSON files on a Mac. Confirm exact exeName, role
   name, `inbound: []`, `outbound: []`, and no wildcard permission.
3. Read-only check that neither candidate exists already under
   `/var/luna-service2` or `/var/luna-service2-dev`.
4. Decide which policy directory webOS 10 actually loads for
   this root-invoked, manually managed binary. Do not assume webOS OSE
   developer-mode instructions apply unchanged to the C5.
5. Only after explicit owner authorization, install **new files only**,
   separately from factory LG policies, and request normal policy rescan
   if the platform requires it. Do not overwrite existing files or modify
   `/usr/share/luna-service2`, the vendor roles, or whitelist files.
6. Run one registration-only invocation, collect exact LS2 return code,
   unregister result and service health. On failure stop; do not change
   `type` to privileged or add wildcards to make the error disappear.

Rollback: remove **only these specifically created files**, and only if
they still match the staged content, then rescan if required. Do not
delete broad policy directories or existing roles. Use no test runner
that automatically provisions an LS2 policy.

## Next phase (NOT included in Stage 1)

For a future approved **read-only** method access test, an app-specific
client permission file for the exact own name may be needed, granting only
the vendor `capture.client` ACG and outbound access only to
`com.webos.service.capture`; whether a `regular`/`dev` client can obtain
that non-public ACG is *unknown*. The C5's
`groups.d/com.webos.service.capture.groups.json` must be inspected for
trust-level constraints first. Do not create that permission file yet.

Even a successfully authorized method lookup does not prove that
`createHandle` is non-mutating, that `execute` allows captures from the
source, or that `destroyHandle` runs on client disconnect. Those must be
tested in order under clear-content gating and bounded cleanup.

Official reference: https://www.webosose.org/docs/guides/development/configuration-files/security-guide/
