# LG C5: Hue-sync service discovery v2 — no installed target found

**Date:** 2026-10-18. **Source:** owner's
`c5-huesync-discovery-v2.txt` (26 lines), read-only output
from the C5 through SSH. No calls to any Hue-sync LS2 method, screen
capture, activation, permission changes or TV file modifications
occurred as part of this discovery command.

## Directly observed in the uploaded log

```text
=== SSH CONNECTION OK ===
LGwebOSTV
=== COLOR INFO MINER ===
-rwxr-xr-x    1 root     root         67156 Apr  6  2011 /usr/sbin/color-info-miner
=== HUE SYNC SERVICE SEARCH ===
SEARCHING=/usr/share/luna-service2
FILENAME_MATCH=/usr/share/luna-service2/api-permissions.d/com.webos.service.colorInfoMiner.api.json
FILENAME_MATCH=/usr/share/luna-service2/client-permissions.d/com.webos.service.colorInfoMiner.perm.json
CONTENT_MATCH=/usr/share/luna-service2/client-permissions.d/com.webos.service.colorInfoMiner.perm.json
FILENAME_MATCH=/usr/share/luna-service2/manifests.d/color-info-miner.manifest.json
FILENAME_MATCH=/usr/share/luna-service2/roles.d/com.webos.service.colorInfoMiner.role.json
CONTENT_MATCH=/usr/share/luna-service2/roles.d/com.webos.service.colorInfoMiner.role.json
FILENAME_MATCH=/usr/share/luna-service2/services.d/com.webos.service.colorInfoMiner.service
CONTENT_MATCH=/usr/share/luna-service2/services.d/com.webos.service.colorInfoMiner.service
SEARCHING=/var/luna-service2
FILENAME_MATCH=/var/luna-service2/roles.d/org.webosbrew.lg-hue-sync.c5-probe.role.json
FILENAME_MATCH=/var/luna-service2/client-permissions.d/org.webosbrew.lg-hue-sync.c5-probe.perm.json
FILENAME_MATCH=/var/luna-service2/manifests.d/org.webosbrew.lg-hue-sync.c5-probe.manifest.json
SEARCHING=/usr/palm/services
SEARCHING=/usr/palm/applications
SEARCHING=/media/developer/apps/usr/palm/services
SEARCHING=/media/developer/apps/usr/palm/applications
SEARCHING=/media/cryptofs/apps/usr/palm/services
SEARCHING=/media/cryptofs/apps/usr/palm/applications
=== DISCOVERY COMPLETE ===
```

The script searched candidate service and application directory entries
to depth 4, matching relevant filenames and
`*.service`/`*.json` content. It checked both factory and
developer/cryptofs application paths where those directories exist.
Only the `colorInfoMiner` registration family and the existing
`org.webosbrew.lg-hue-sync.c5-probe` paths were reported.

**No `com.lighting.huesync.service` registration,
`com.lighting.huesync` application or direct implementation
path was found in the search scope.** This is not proof that
these components cannot exist under a different path, symlink
target, package identity or dormant firmware partition. It
does mean the original exact target referenced by
`/usr/sbin/color-info-miner` is **not established as installed or
available** on this C5.

## Effect on the prior static-binary interpretation

The previously verified `color-info-miner` ELF contains
`com.lighting.huesync.service`, `startSync`, and `stopSync`
references and reports only readiness/autoEnable state in
`hueSync/getStatus`. The presence of these strings is therefore
best treated as **optional/integration-control scaffolding**,
not evidence of an operating Hue color-data producer.

`color-info-miner` itself remains installed (67,156 bytes),
and the registration under
`com.webos.service.colorInfoMiner` is still present.
That fact alone does not make `com.lighting.huesync.service`
available or expose aggregate RGB values.

The log does list the existing Stage 1 probe's LS2 filenames,
but does **not** run `sha256sum -c` or show the
`STAGE1_HASHES_PASS` receipt; thus this evidence should
not be described as confirming their byte-for-byte integrity.

## Disposition

Treat the `com.lighting.huesync.service` implementation lead
as **unavailable/unconfirmed on this installed C5 image**, and
avoid repetitive broad directory scans or attempts to invoke
`startSync`/`stopSync`. Do not copy pairing or app data,
grant LG factory identities, alter LS2 privileges or make a
protected-content frame-access path.

Next productive investigations:
1. Continue the already demonstrated, DRM-free
   `execute`/BLENDED JPEG quality and latency testing
   under a separately approved bounded device experiment.
2. Separately evaluate the documented
   `pqcontroller/getEyeqSensorData` **room-light sensor**
   contract for optional WLED brightness, but do not
   mistake ambient brightness for a screen histogram.
3. Revisit protected-content regional colors only if
   future vendor documentation or a legitimately
   supported, verifiable aggregate-statistics API emerges.

The firmware's `VtHistogram` remains stubbed in the
fingerprinted C5 build and the repository's DRM-first
`Protected → verified healthy histogram; else Hold/black`
policy must remain unchanged.
