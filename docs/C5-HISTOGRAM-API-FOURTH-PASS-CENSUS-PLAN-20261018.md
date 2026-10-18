# LG C5: fourth-pass histogram API coverage gap and targeted census

**Date:** 2026-10-18. **State:** the prior 3-pass LS2/ELF static audits and
their GitHub CI completed successfully. **No new TV collection has run** for
this fourth pass. The current Stage 1 backup/hash-verified policy and
the DRM-first `Protected → healthy histogram or Hold/black` routing remain
unchanged. Do not describe a histogram backend as operational on this C5.

## Important limitation identified

The owner's earlier `c5-histogram-readonly.qVZTzZ.zip` was generated
using `c5-histogram-readonly-collect-mac.sh`. That script selects
filenames matching *particular* capture, compositor, lightingcontroller,
livepick, VT and related service names plus the capture service's loaded
shared libraries. The earlier inventory intentionally checks four API
definition families:

- `com.webos.service.lightingcontroller`
- `com.webos.service.livepick`
- `com.webos.service.livepickfull`
- `com.webos.service.capture`

The resulting absence of a regional color getter proves only that
**those collected API declarations** do not expose one. It is not
evidence that the C5 system image contains no other relevant
`*.api.json` method names anywhere. The previous audit also excluded
most picture-quality/sensor service descriptions, notably the
`com.webos.service.pqcontroller` family. No alternative protected
histogram capability has been demonstrated.

## Reference distinctions and confidence

LG's official webOS TV supported Luna API introduction enumerates
supported application service categories, without documenting a
screen-region RGB histogram read API:

https://webostv.developer.lge.com/develop/references/luna-service-introduction

The public webOS Open Source Edition reference **explicitly does not
guarantee** API compatibility with webOS TV:

https://www.webosose.org/docs/reference/

A user on a rooted older LG C1 reported the LS2 call
`com.webos.service.pqcontroller/getEyeqSensorData` returning
`sensorData.luminance` and `visibleLuminance`.
This is a **room-light sensor** report, not screen-region colors.
Neither availability nor permissions on this C5 are established:

https://www.reddit.com/r/webos/comments/122yu1r/

This distinction matters: even if a future API definition includes
`getAveragePictureLevel`, its name alone does not prove a regional
histogram, the content of its return payload, access to protected
material, or permission for our app to call it. A factory `private`
label is not an API entitlement.

## Prepared narrowly scoped fourth-pass collection

The separate, Mac-runnable `c5-api-census-kit.zip` packages:

- `c5_histogram_api_census_mac.sh`: one bounded read-only SSH
  session, no LS2 call and no device execution. Copies only selected
  `/usr/share/luna-service2/api-permissions.d/*.api.json`
  declaration files, with a maximum of 100 files, 64 KiB each
  and 2 MiB overall. Targets filenames containing lighting, LED,
  sensor, ambient, color, PQ controller, HDR, display, capture,
  screen, picture, video and related terms. No roles, credentials,
  content, vendor ELF, screenshots, DRM keys or device files.
- `c5_histogram_api_census.py`: offline tar safety checking;
  emits method names, public/private group labels, and SHA-256s in
  JSON plus a compact text table. It labels possible **region**,
  **global picture statistics**, **room-light**, or general
  lighting leads separately. No selected name establishes
  a callable or protected-content-safe API.
- `README.txt`: command and privacy review guidance.

The parser synthetic self-test and a fully generated test tar were
run locally and passed; the shell script passed `sh -n`.
**The TV-side collection has not been run**, so any new method
discovery is pending actual owner output.

### How to run

Unpack `c5-api-census-kit.zip` in `~/Downloads`, then:

```sh
cd ~/Downloads/c5-api-census-kit
python3 c5_histogram_api_census.py --selftest
sh c5_histogram_api_census_mac.sh
```

It uses `root@192.168.8.184` as in previous approved collectors.
The script saves `~/Downloads/c5-api-census.XXXXXX.zip`; review
before uploading. The earlier collected archive remains private.
Never execute methods merely because they appear in this list.

## Next decision after receiving a census

First compare the method names and their declared groups against
the four already inspected services, and prioritize newly
discovered *read-only* global/region statistics signatures.
Perform public documentation and offline caller analysis first.
If a method is private, undocumented or resembles a debug/write
control, leave it uncalled. Do not grant factory identities,
modify the LG authorization database, initialize protected
frame capture, patch vendor stubs or relax the protected-media gate.

If no new suitable API declarations are found, the outcome is
"none found among selected API declarations", **not** proof
that the hardware has no potential histogram.
