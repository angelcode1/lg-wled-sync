# C5 vendor/public lighting API inventory — offline third pass

**Scope:** static, reproducible inspection of the owner's
`c5-histogram-readonly.qVZTzZ.zip`, SHA-256
`dedcdb098d114dd6ddeb82178a6cb8d2e372953b9b8c1be41133f8053153c33a`.
No TV call, capture, API execution, role change, DRM test, or image retrieval.
The analyzed bundle contains the firmware's `api-permissions.d` snapshots
and four ELF images. Missing declarations or symbols elsewhere cannot
be excluded by this targeted analysis.

## Reproducible static scan

`scripts/c5_lighting_interface_inventory.py` accepts the original ZIP
and runs `readelf --dyn-syms --wide` **only on temporary copies of
selected ELF files**. It never executes those ELF files. It validates
candidate file sizes and only reads four API declarations and four
ELF binaries.

```sh
python3 scripts/c5_lighting_interface_inventory.py --selftest
python3 scripts/c5_lighting_interface_inventory.py \
  --archive ~/Downloads/c5-histogram-readonly.qVZTzZ.zip > c5-interface-inventory.json
```

The synthetic parser self-test passed locally with
`C5_LIGHTING_STATIC_INVENTORY_SELFTEST_PASS execute=false capture=false`.
The checked-in GitHub CI runs the self-test without shipping proprietary
binaries or needing a TV. The ZIP inventory was verified locally.
The JSON names and SHA-256 values are suitable for comparing owner-collected
snapshots from future firmware releases. They are **not** runtime permission
proof.

## Collected LS2 API declarations

| Service | Groups in its `api.json` | Distinct methods | Histogram/region-color outputs declared |
| --- | --- | ---: | --- |
| `com.webos.service.lightingcontroller` | `public`:4, `private`:4 | **4** | **None** |
| `com.webos.service.livepick` | `private`:33 | **33** | **None** |
| `com.webos.service.livepickfull` | `private`:19 | **19** | **None** |
| `com.webos.service.capture` (and capturepermission) | `capture.client`:15, `private`:17, `capturepermission.security`:2 | **17 total** | **None** |

The `lightingcontroller` methods are
`getAmbientLedMicomVersion`, `getGamingLedMicomVersion`,
`updateAmbientLedMicomFirmware`, and
`updateGamingLedMicomFirmware`. They control or query the
built-in LED microcontroller firmware—not screen color statistics.
Their listing in both `public` and `private` does **not** authorize
arbitrary callers; deployed application permissions and product
policies still apply.

`livepick` has recognition and scene-information endpoints,
including `getSceneInfo` and `getTotalInfo`, but neither is
declared to return per-edge RGB histogram data. Do not construe
`getSceneInfo` as a color-result contract without a verified
response schema. The same limitation applies to its `isCapturable`
and `isPickable` status queries.

The 15 `capture.client` methods include
`createHandle`, `setProperties`, `setOptions`, `setOutput`,
`execute` and `executeOneShot`. They do not expose a
`getHistogram` or regional color result. The separate
`capturepermission.security` control group must not be modified.

## Static ELF imports/exports

- `lightingcontroller` SHA-256
  `34de65f1f49949d9f7b9041b90e4de79539cbd1beedb5272b069ff5153342fd0`
  dynamically imports **11** `vtCapture_*` functions,
  including `vtCapture_currentCaptureBuffInfo`, `process`,
  `preprocess`, `planeInfo`, `initEx` and cleanup. It
  imports **no** `vtCapture_*Histogram` function in its
  collected dynamic symbol table. This corroborates the previous
  YUV420-to-RGB and `kMeansClustering(13)` **frame** path.
- `libvtcapture.so.1.0.0` SHA-256
  `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`
  exports native histogram symbols, but offline Thumb
  disassembly already confirmed `VtHistogram::initHistogram()`
  unconditionally returns `7` in this exact build. **Exports
  are not working functionality.**
- `livepick` SHA-256
  `e62df8230676cf27c72edad81d047681d1f2846b5fcbe094c118b634438953f4`
  exports ImageMagick `GetColorHistogram` and
  `RegisterHISTOGRAMImage`; these describe software image
  analysis rather than a protected-screen aggregate interface.
- `libkadaptor.so.2.0.1` SHA-256
  `9d817747df49847991740286f58ea28d92129304f12c78601be7c36dbfd11dd3`
  exports `KADP_APR_*` debug and related hardware control
  APIs as described in the previous second-pass audit. Its
  `GetAplMinMax` and `Get_OSDAlphaAPL` exports are
  immediate-return stubs on this build. Nothing in the
  declared app-visible LS2 methods provides these raw debug
  functions as an authorized screen-color API.

These are **targeted static observations**. An unlisted private
symbol or different vendor build could behave differently; we
make no blanket claim that the hardware cannot generate histograms.

## Public SDK research: separate webOS TV from webOS OSE

LG's webOS TV Developer Luna Service API list does not document an
app-facing histogram, per-edge color, or lightingcontroller color-output
service. LG webOS TV community staff have stated that screen-capture
APIs are not provided for normal web apps. These developer statements
are consistent with (but do not replace) inspection of this particular
rooted C5 firmware.

The webOS OSE reference **explicitly warns** that OSE API availability
does not guarantee compatibility with webOS TV. Do not infer that a
working OSE service can be registered or called under the C5's TV ACG.

A community report for an older LG C1 identified
`com.webos.service.pqcontroller/getEyeqSensorData` with
`sensorData.luminance` and `visibleLuminance`. That data concerns
**ambient room brightness**, not regional colors from displayed
content. Its existence and permission contract on this C5 remain
unverified. A future read-only check, separately authorized, might
allow optional WLED auto-brightness—not DRM-protected color capture.

References:
- https://webostv.developer.lge.com/develop/references/luna-service-introduction
- https://forum.webostv.developer.lge.com/t/how-to-capture-screen-on-webos-tv/7880
- https://www.webosose.org/docs/reference/
- https://www.lgwebos.com/topic/9481-lgtv-ambient-sensor-reading/

## Decision

**No supported source of per-zone or per-edge screen colors from
protected material was established.** Preserve `Protected → verified
healthy histogram only; otherwise Hold/black`; keep the native
screenshot path clear-only. Do not invoke KADP APR debug commands,
change factory roles, patch vendor stubs, or activate protected
frame reads.

A worthwhile next task is a read-only *difference* of another
legitimately obtained firmware's API/ELF inventory, not probing DRM
paths until an authorized histogram provider has been identified.
