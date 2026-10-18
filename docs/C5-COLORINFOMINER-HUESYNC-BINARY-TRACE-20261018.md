# C5 ColorInfoMiner binary audit — exact ARM ELF, Hue service handoff

**Date:** 2026-10-18. **Source:** owner-uploaded
`c5-colorinfominer-binary.zip`, processed *offline only*.
Outer ZIP SHA-256:
`932260a2e79be61ac5c84a0dbea5109d28244b8021a724b803607efe2846b276`.
The `manifest.json` validates **5/5** embedded regular files,
**68,772 bytes** total; the collector's `tv_method_calls=0`.
No executable was run on the TV, no LS2 method called, and no
video/key/device material was collected for this review.
LG binary bytes are **not** committed into this repository.

## ELF provenance

`/usr/sbin/color-info-miner`:
- Size: 67,156 bytes.
- SHA-256:
  `ed8f68dfb6deadceb21e9dd59730f8c87f5f843ecc0945ad36911370b0575955`.
- ELF: 32-bit ARM EABI5, PIE, dynamically linked, soft-float,
  interpreter `/lib/ld-linux.so.3`, stripped.
- Build ID: `230027705382c051d487deb2880af032df0e55c9`.
- Dynamic `DT_NEEDED`: `libglib-2.0.so.0`,
  `libluna-service2.so.3`, `libPmLogLib.so.3`,
  `libpbnjson_cpp.so.2`, `libstdc++.so.6`,
  `libgcc_s.so.1`, `libc.so.6`.
- **No direct dependency** on `libvtcapture`,
  `libdile_vt`, `libhalgal`, LG `captureservice`, or a
  vendor histogram library. This does not exclude calls to
  another service that *itself* uses capture hardware.

Static inspection used `readelf --dyn-syms`, ELF section
disassembly via `llvm-objdump --triple=thumbv7-unknown-linux-gnueabi`,
and read-only string inspection.

## Confirmed coordination path

The executable dynamically links LS2 functions including
`LSRegister`, `LSRegisterCategory`,
`LSRegisterServerStatusEx`, `LSCall`,
`LSCallOneReply`, `LSSubscriptionAdd`,
`LSSubscriptionReply`, and `LSMessageRespond`.
Its exported C++ symbols and their strings support
the following static control flow:

```text
com.webos.service.colorInfoMiner
    |-- startUp
    |-- /hueSync/getStatus
    |-- /hueSync/setAutoEnable
    |-- /hueSync/setManualEnable
    |
    |-- subscriptions to
    |     com.webos.service.settings (EULA, hueSync auto/manual, WOL)
    |     com.webos.service.tvpower (power state)
    |     com.webos.service.config (profile list)
    |     com.webos.applicationManager (com.lighting.huesync app status)
    |     com.palm.connectionmanager (network status)
    |
    |-- connection-status monitoring:
    |     com.lighting.huesync.service
    |
    '-- LunaHandler::colorInfoMinerSync
           '-- LS2 requests to separate Hue service:
                 startSync / stopSync
```

`LunaHandler::colorInfoMinerSync` is an internal C++ function
at text address `0x751c`; it gates requests on
`CGlobalData::IsConnectedHueService` and sends a
constructed service request through
`LunaHandler::callSync` (text address `0xc758`).
Firmware strings contain
`com.lighting.huesync.service`, `startSync`, and
`stopSync`. Those methods appear to initiate/stop
Hue synchronization, **not read a histogram**.

`LunaHandler::checkStartSyncCondition` (at `0x799c`)
and callbacks tracking settings, power/network, app
availability and service status provide another strong
indication that this program manages readiness and
autostart. It does not present a decoded frame loop,
YUV/RGB conversion routine, histogram sampling
routine, or LED-region data endpoint.

## Hue-sync getStatus is state, not pixels

The exported function
`LunaHandler::hueSyncGetStatus` at `0xb89c`
calls `CGlobalData::UpdateHueSyncGetStatus` at
`0x5884`. Disassembly and immutable JSON key strings
show the latter populating a reply with:
- `isReady` — state flag,
- `autoEnable` — state flag,
- `returnValue` — success flag.

The registration `com.webos.service.colorInfoMiner.api.json`
declares the same four methods already seen in the
fifth-pass census. **No RGB/histogram/zone-color
response is present in this audited getStatus
implementation.** The exact full possible error/subscribe
payloads need further proof before documenting every field.
The text string `HUE_STATE` is present but cannot be taken
to imply a screen histogram or RGB result.

## Important interpretation limits

- The service names and `startSync/stopSync` strings do not
  establish whether Hue sync supports DRM-protected content.
- This executable's role has unrestricted outbound LS2 permission
  *for LG's registered identity*, not our app. Do not copy,
  impersonate or modify its role.
- There is no proof that the separate Hue service is installed,
  running, callable or granted useful aggregate outputs on
  this particular TV. It is simply the **next concrete
  implementation target**.
- The existing vendor `VtHistogram` stub result remains valid:
  `initHistogram` returns `7` in the fingerprinted C5 ELF.
- The protected-content router still selects healthy histogram
  only, otherwise Hold/black. No BLENDED screenshot or frame
  fallback is appropriate for DRM content.

## Next strictly read-only collection

Resolve `com.lighting.huesync.service` from its **factory
registration files** and collect only the referenced, bounded
executable or installed code bundle plus static API
declarations and dependencies. An LG app package may have
license or pairing materials in its data directory:
**never collect user preferences, pairings, tokens, certificates,
keys, private logs, screenshots or DRM payloads**. First
inventory registration metadata and executable paths.
Use explicit file-size bounds and path allowlists before
any binary copy. Do not activate `startSync` or
`stopSync`, enable Hue sync, or grant any new LS2
permissions merely to see whether it works.

A subsequent ELF/JS static scan should determine:
1. Does the *actual* Hue service link `libvtcapture`,
   `libhalgal`, `captureservice` or other hardware
   color analysis providers?
2. Does it return per-zone color data over any
   documented, authorized read-only API, or only
   transmit lighting control output internally?
3. Can an independent, vendor-supported aggregate
   interface be used without protected frame access,
   privileged identity impersonation or policy changes?

No new TV test is authorized by this document.
