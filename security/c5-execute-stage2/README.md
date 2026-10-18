# C5 Stage 2 (PROPOSED ONLY) — first native read-only capture-method request

**Status: NOT INSTALLED, NOT APPROVED.** Stage 1 successfully registered
and unregistered the dedicated native Rust name on 2026-10-18:

```text
LS2_NAMED_ATTEMPT executable=/var/home/root/c5-ls2-probe name=org.webosbrew.lg-hue-sync.c5-probe private_bus=true capture=false
LS2_NAMED_OK registered=true unregistered=true capture=false
C5_STAGE1_REGISTRATION_PASS policy_installed=true capture=false
```

Thus registration and exact executable role lookup **work** with the
regular/dev no-ACG Stage 1 policy. The prior anonymous
`-1027 Invalid permissions for (null)` does not apply to this named role.

This folder describes a **replacement for only our three Stage 1 policy
files**, after separate explicit user approval and successful new CI.
It is not an installer and must not be installed automatically.

## Least-privilege intended change

The existing role stays at
`/var/home/root/c5-ls2-probe`, with exactly the same allowed name,
`regular` role and `dev` trustLevel. No vendor identity is borrowed.

- Role inbound remains `[]`; outbound changes from `[]` to the
  **one destination** `com.webos.service.capture`.
- Client ACG changes from `[]` to exactly `["capture.client"]`.
- No `all`, `capturepermission.security`, wildcard service name,
  new service file, persistent daemon or media/device permissions.
- Files `manifest.json`, `role.json` and `client-permissions.json`
  represent exact replacements of our Stage 1 files. Do not create
  duplicates or edit system files under `/usr/share/luna-service2`.

Important: `capture.client` grants access to *all methods* in the
vendor's private capture ACG, including mutating/session methods.
This is the **smallest existing LG ACG that includes getCapability**,
but is not method-level least privilege. Restricting the Rust diagnostic
to `getCapability` is a code/runtime guard, not an LS2 method-level ACL.

**The LG TV may reject this group for a dev/regular client**; that is
a meaningful denial, not reason to escalate to an LG privileged role.

## Planned Stage 2 test (not run)

With the new **verified** ARMv7 binary deployed only to the same
standalone executable path, and separately approved Stage 2 policy:

```sh
/var/home/root/c5-ls2-probe test-ls2-capture-permission \
  --acknowledge-capture-permission-check
```

This uses native `LSRegister` under our *own name*,
`LSGmainContextAttach`, `LSCallOneReply` to
`luna://com.webos.service.capture/getCapability` with `{}`,
then `LSUnregister` and GLib context cleanup. It has a 2-second
reply window, capped output text, no pixels, no handle creation and
no capture buffers.

Expected possibilities:

- Native `LSCallOneReply` says permissions denied: STOP. Retain the
  safe `executeOneShot` subprocess path.
- Response is `{"returnValue":true}`: the method is authorized,
  but the vendor implementation may be an empty/degenerate handler.
  It does NOT prove video capture or continuous `execute`.
- Any crash, hang, missing symbol or unregister failure: STOP and do
  not proceed to session create/destroy.

## Approval and rollback preconditions

Before any TV modification:

1. CI must pass Rust format, 149+ tests, Clippy and ARMv7.
2. Take backups of **all three existing Stage 1 files** with hashes,
   stage all three Stage 2 files, verify exact JSON fields, and
   check their filenames/expected manifest pointers.
3. Verify the live Stage 1 installed files match their immutable,
   expected checksums; refuse on any mismatch or absent file.
4. Replace *only* the three owned policy files transactionally, with
   original backups retained; use `ls-control scan-services` only
   with explicit approval. Never edit factory roles or whitelists.
5. Do one bounded read-only method call, log exact response/denial,
   no repeated retries.
6. Restore **exact** Stage 1 files on any failure and rescan.
   Confirm named registration still succeeds after rollback.

Further lifecycle testing (`createHandle`, `destroyHandle`,
`execute`) is a separate stage, not permitted by this Stage 2 test
even though the capture ACG is broader than the chosen method.

The difference between LS2 registration, outbound target permission,
and API group membership is documented at:
https://www.webosose.org/docs/guides/development/configuration-files/security-guide/
