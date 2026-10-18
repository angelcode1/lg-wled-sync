# LG C5 colorInfoMiner — exact executable discovered (2026-10-18)

**Scope:** offline inspection of owner-uploaded
`c5-colorinfominer-readonly.zip`; no LG executable run or TV connection
was used for this review. The ZIP SHA-256 is
`4f8462f6f7691e810e92f1b8a9c3dafdbc00127600d711a10916a2ea9950773e`.
All **four** embedded files independently match the submitted manifest
SHA-256 and sizes (total 1,616 bytes).

## What the upload actually contains

The preceding filename-based collector matched only these static LS2 files:

| System file | Size | SHA-256 |
| --- | ---: | --- |
| `usr/share/luna-service2/api-permissions.d/com.webos.service.colorInfoMiner.api.json` | 924 | `2d4d03cf78ab4b05fbf6c81579b6370979fcd4f0dad9828b155943d3a9a064bf` |
| `usr/share/luna-service2/client-permissions.d/com.webos.service.colorInfoMiner.perm.json` | 118 | `5e3d8627084aa95cc87bdb0b7cce04993c06dac98c59547b9fe5e0901cc48ac3` |
| `usr/share/luna-service2/roles.d/com.webos.service.colorInfoMiner.role.json` | 438 | `7c3363f8a51e3a5c57d38559ccc77b10dc6a11f1b4f371f5784e219b35810d11` |
| `usr/share/luna-service2/services.d/com.webos.service.colorInfoMiner.service` | 136 | `e12ce064e44ac2db5f7a63e42fde5b3abb721c3c1688335c5339317d26fb7b67` |

**There are no ELF executables or libraries in this ZIP.**
Previous descriptions suggesting that this upload is sufficient to
trace implementation imports are premature.

### Critical new clue: executable path and a secondary service

The original LS2 service declaration, quoted exactly:

```ini
[D-BUS Service]
Name=com.webos.service.colorInfoMiner;com.webos.service.colorInfoMinerSync
Exec=/usr/sbin/color-info-miner
Type=dynamic
```

The corresponding role identifies the exact
`exeName` as `/usr/sbin/color-info-miner` and lists both
`com.webos.service.colorInfoMiner` and
`com.webos.service.colorInfoMinerSync` in `allowedNames`.
The role's `inbound: ["*"]` and `outbound: ["*"]` permissions
are attached to **LG's executable identity**, **not** a generic
grant to a third-party ambient-lighting application.

The client permissions declaration similarly provides `all`
to the two specific LG service identities. It must not be
copied or impersonated in our application's LS2 role.

The API file repeats the four previously observed methods:
`startUp`, `hueSync/getStatus`,
`hueSync/setAutoEnable`, `hueSync/setManualEnable`.
**No per-region RGB/histogram getter is declared** in this
particular API file, but `colorInfoMinerSync` could have
separate internal APIs; its own implementation and contracts
are not in the upload. Do not infer what it does from its name.

## Collector defect and corrected bounded follow-up

The earlier path filter matched
`colorInfoMiner|colorMiner|hue[-_]?sync`, omitting the
hyphenated **`color-info-miner`** basename. Thus it found
LS2 metadata but missed the exact executable named inside it.

The corrected, independently self-tested
`c5-colorinfominer-binary-kit.zip` explicitly reads:

- `/usr/sbin/color-info-miner` — must be a regular, at-most-16-MiB
  ELF file; refuse if missing, symlinked or non-ELF.
- Exact `colorInfoMiner` and `colorInfoMinerSync` LS2
  service, role, API and client-permission paths under
  `/usr/share/luna-service2` when present.

It caps the entire file collection at nine files / 20 MiB.
A separate Mac Python verifier rejects non-regular tar entries,
unexpected paths, duplicate paths and excess sizes, and generates
SHA-256 receipts. Its synthetic self-test and `sh -n` passed.

No LS2 method or diagnostic/test suite is invoked. No change to TV
files, factory permissions, DRM policy, services or firmware is made.

## Next after receiving the exact binary

Inspect ARM dynamic imports, `NEEDED` libraries,
read-only strings and call sites to answer whether the program:

1. Calls the known-stubbed `vtCapture_*Histogram` ABI,
   normal frame capture, `captureservice` or some independent
   authorized aggregate-color producer.
2. Publishes a color-result LS2 method or only operates
   internal Hue-sync controls.
3. Has a documented, permission-compatible way for our
   application to consume **aggregate** results.

Do not execute any HueSync startup, setters or unknown
`colorInfoMinerSync` methods. The proven production
`Protected → healthy histogram; otherwise Hold/black`
routing remains unchanged.
