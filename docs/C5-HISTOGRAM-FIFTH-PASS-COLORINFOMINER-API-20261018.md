# C5 fifth-pass LS2 API census: colorInfoMiner / hueSync discovery

**Evidence:** user-uploaded `c5-api-census.zip`, with `api-declarations.tar`,
`api-census.json`, `api-census.txt` and `README.txt`, inspected
offline on 2026-10-18. All **12** embedded raw `*.api.json` files
individually matched the SHA-256 receipts in the JSON report.
The census covered **12 service declarations** and **202
distinct method names summed within services** (not an exhaustive
device-wide total).

**No TV methods were invoked** by the collector. A method declaration,
even in a `public` group, is **not** proof that the method is
callable from our own app or that it returns screen colors.

## Primary new finding: com.webos.service.colorInfoMiner

The original API declaration SHA-256 is
`2d4d03cf78ab4b05fbf6c81579b6370979fcd4f0dad9828b155943d3a9a064bf`.

The exact method set:

```text
com.webos.service.colorInfoMiner/startUp
com.webos.service.colorInfoMiner/hueSync/getStatus
com.webos.service.colorInfoMiner/hueSync/setAutoEnable
com.webos.service.colorInfoMiner/hueSync/setManualEnable
```

These four methods occur in both `public` and `private`
declarations and in the **`colorInfoMiner`** ACG group.
The service `startUp` method is additionally in
`activities.callbacks` and `configurator.callbacks`.

This is an independently verified **Hue-sync control-family
declaration** on the C5, not the generic
`com.webos.service.lightingcontroller` LED-microcontroller
firmware/version service from the previous audit.

**Crucial limitation:** the LS2 declaration does *not* include
`getColors`, `getHistogram`, per-region RGB samples,
a subscribe-to-color endpoint or even a payload schema for
`hueSync/getStatus`. The term `hueSync` suggests an
LG integration with hue-style lighting, **but what implementation,
external devices, permissions, picture-sampling source or runtime
support this build provides remains entirely unknown**.

Do **not** invoke `startUp` or either `set*Enable`
method as an exploratory action: the method names imply state
changes and could start lighting activity. Do not claim that
`hueSync/getStatus` contains pixel colors; it may just be
enabled/disabled state. Only a read-only *implementation-file*
collection and offline symbol/string analysis is justified next.

## Secondary finding: ambient-light sensor method confirmed in C5 declarations

`com.webos.service.pqcontroller/getEyeqSensorData` is
present under both `public` and `private` groups in the
C5's `pqcontroller` declaration (SHA-256
`c1cd14dd9347f2a40b372140525124c8007bc2a1bd7d875ecae3738acd925418`).
This is a **confirmed API method name**, not a confirmed
working sensor or granted app permission. It is an **ambient
room-light** lead, not regional colors from the displayed video.

Other PQ control methods include `getExternalPqData`,
`getInfo`, and `getProperties`; the declaration
provides neither response schemas nor evidence that any
method contains safe aggregate screen color statistics.

## Other newly visible services

| Service | Declared distinct method names | Relevance |
| --- | ---: | --- |
| `colorInfoMiner` | 4 | **High-priority control-family lead**; no color getter declared |
| `ledbarcontroller` | 3 across callback groups | `setAplusLedBarControl` is a setter, not color output |
| `pqcontroller` | 23 | `getEyeqSensorData` could support brightness adaptation |
| `oledepl` | 23 | OLED health/protection and panel status; no RGB histogram getter declared |
| `videooutput` | 26 | Window/scaler/display controls and status, not declared regional RGB |
| `rollingscreen` | 26 | Rolling display motor/view control, not relevant |
| `secondscreen.gateway` | 22 | Secondary-screen management; no declared histogram getter |
| `com.webos.tas.screensaver` | 2 | Screensaver methods, not histogram |
| `lightingcontroller` | 4 | Only ambient/gaming LED controller firmware/version methods |
| `livepick` / `livepickfull` | 33 / 19 | Scene/information services; no histogram getter declared |
| `capture` | 17 | Video screenshot/handle API, not histogram |

**Method tally accounting:** the `ledbarcontroller` 3-method
count includes two callback-service method names from
`com.webos.service.config` and `com.webos.service.tvpower`;
the declaration exposes only one
`com.webos.service.ledbarcontroller/` method.
Likewise `capture` groups include two
`com.webos.service.capturepermission/` methods. Totals
are literal per-file method-name sums, **not counts of
service-owned methods only**.

No `screen_region_leads` or `screen_global_leads` were reported
by the narrowly scoped classifier. This keyword classifier does
not prove that an obliquely named API or a native
non-LS2 producer cannot provide aggregate color data.

## Next analysis (source-only, no new TV API calls)

The user can separately collect only the implementation binaries
and LS2 role/manifest metadata for
`colorInfoMiner` / `hueSync`, from system directories under
`/usr` and `/etc/palm`; do not collect pairing tokens,
user settings, bridge credentials, internal media frames or
protected DRM payloads. Inspect file hashes, ELF imports,
symbols, strings and any implementation references to
`libvtcapture`, `captureservice`, JPEG, Hue
Entertainment or socket output.

**Do not equate Hue-sync with protected-content support.**
The vendor's `VtHistogram` initializer remains a confirmed
stub on this C5. The deployed DRM-first router continues
`Protected → verified working histogram, else Hold/black`;
no production capture backend or TV file was changed.
