# Owned-name LS2 registration diagnostic

> **Research disclaimer:** This diagnostic and its policy templates are intended for research and educational purposes only.

`test-ls2-registration` tests registration and unregister only. It does not
request images, call capture methods, create a capture handle, open a capture
device, change permissions or send light output. Running it is an active bus
operation and requires `--acknowledge-luna-registration`.

Current modes:

- Default: `LSRegister(NULL)` followed by `LSUnregister` on success.
- `--named-probe`: the exact application name
  `org.webosbrew.lg-hue-sync.c5-probe`, followed by unregister on success.

The old `--capture-client` flag is rejected by argument parsing. Neither mode
generates LG Resource Manager or capture-client service names.

## Candidate policy files

The three JSON files here are source templates, not an installer. They bind
only `/var/home/root/c5-ls2-probe` to the exact owned name, with a `regular`/`dev`
role, empty inbound and outbound arrays, and no access-control groups. There
are no wildcard grants, service activation files or capture permissions.

Policy installation or rescan requires explicit owner direction and review of
the actual target's existing files, loaded policy directories and rollback.
Do not overwrite an existing policy or substitute an LG identity. The
repository's older `security/c5-registration-only` files are historical
experiment inputs; they are not the policy for this command's owned name.

After an independently approved installation and verified ARMv7 build at the
exact executable path, the diagnostic invocation is:

```sh
/var/home/root/c5-ls2-probe test-ls2-registration \
  --acknowledge-luna-registration --named-probe
```

Success is `LS2_NAMED_OK registered=true unregistered=true capture=false`.
It establishes only that this invocation registered and unregistered. It does
not establish `capture.client` membership, capture method access, production
daemon authorization, pixel ownership, source timestamps or capture-to-light
behavior. The diagnostic reports registration and cleanup failures separately.

The daemon and other capture commands are unchanged by this diagnostic update.
