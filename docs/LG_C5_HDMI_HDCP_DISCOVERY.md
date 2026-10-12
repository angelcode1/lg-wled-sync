# C5 HDMI / HDCP state discovery (read-only)

Evidence observed on an LG OLED42C5PSA.AAUQLJD, webOS SDK 10.0.0,
with Prime Video running on Fire TV connected to HDMI 1.
These findings do **not** establish that HDCP is disabled or enabled.

## Confirmed behaviour

- Private Luna works via `sleep 6 | luna-send -i -f`; `luna-send -n 1`
  on this C5 exits 0 with no reply.
- `com.webos.media/getActivePipelines` returned unchanged foreground
  HDMI1 resources when Prime Video started: `HDMI_INPUT`, `VHDMIRX`,
  `AHDMIRX`, `ADEC`, `ADEC_BANDWIDTH`, plus `MAIN_SCALER`
  and `MAIN_SOUND` in adjacent `avconnector` pipelines.
- No `SVP_CPB` resource was present. On HDMI this is **not** an HDCP
  detector; routing correctly remains `Unknown`, not `Clear`.
- Root `ls-monitor -i com.webos.service.extinputs.hdmi` showed
  `getHDMIInfo`, `getHDMIDiagnosticsInfo`, `getSignalState`,
  `getProperties`, `status`, `getVideoInfo` and other methods.
  It did **not** advertise a method explicitly named getHDCPStatus.
- Root `ls-monitor -i com.webos.service.eim` showed
  `getHDMISignalInfo`, `getCurrentInput`, `getInputStatus` and
  `getConnectionStatus`.
- `/sys/class/lg/hdmi200` exists, linked to
  `/sys/devices/virtual/lg/hdmi200`. Its exposed regular sysfs files
  are `dev`, `uevent` and generic power management attributes.
  **No HDCP state attribute** was exposed by this node.

These interface names are observed registrations. **Return schemas and
required request arguments remain unknown until actually queried.**

## Safe, read-only follow-up

Keep Prime Video playing through HDMI 1 and use the private bus. Start
with `{}` because service-specific argument schemas are unknown; an
error about a missing field is evidence, not proof of an absent API.

```sh
ssh root@192.168.8.184 'sh -s' <<'TV'
probe() {
  printf '\n===== %s =====\n' "$1"
  sleep 6 | luna-send -i -f "$2" '{}' 2>&1
}
probe 'HDMI info' 'luna://com.webos.service.extinputs.hdmi/getHDMIInfo'
probe 'HDMI diagnostics' 'luna://com.webos.service.extinputs.hdmi/getHDMIDiagnosticsInfo'
probe 'HDMI signal state' 'luna://com.webos.service.extinputs.hdmi/getSignalState'
probe 'EIM HDMI signal info' 'luna://com.webos.service.eim/getHDMISignalInfo'
probe 'EIM current input' 'luna://com.webos.service.eim/getCurrentInput'
TV
```

Do **not** invoke `getHDMISelfDiagnostics`, setters, `open`, `close`,
`resetHDMIErrorHistory`, or sysfs writes as part of discovery.
Inspect any output for device identifiers before sharing. HDCP capability,
HDCP version support, active authentication state, and actual protection
of a specific title are distinct concepts.

## Follow-up: actual service replies with empty payload (2026-10-12)

The owner queried the LG methods using private interactive Luna and `{}`
while a Fire TV played Prime Video on HDMI1:

- `extinputs.hdmi/getHDMIInfo`: `{"returnValue":false,"pipelineId":""}`
- `extinputs.hdmi/getHDMIDiagnosticsInfo`: same failure result.
- `extinputs.hdmi/getSignalState`: `errorCode:3`, explicitly
  `Failed to validate against schema: Missing mandatory field: 'pipelineId'`.
- `eim/getHDMISignalInfo`: `returnValue:true`;
  `HDMI1.5v="TRUE"` and `HDMI1.signal="TRUE"`; other input signals
  `FALSE` (note an observed `HDMI2.5v="FASLE"` typo).
- `eim/getCurrentInput`: `mainInputSourceId:"HDMI_1"`.

Therefore **the first three methods have not yet been meaningfully tested**:
send the ID from a **fresh** `com.webos.media/getActivePipelines`
response, not a port number and not a stale identifier from a previous
SSH session. The active foreground TV pipeline with `HDMI_INPUT` is the
first reasonable candidate; `type:avconnector` pipelines are distinct,
so investigate separately if that candidate is rejected.

Use a local Mac Python 3 parser to select exactly one foreground
`type:tv` pipeline with `HDMI_INPUT`, then validate its identifier
before issuing read-only method requests:

```bash
bash <<'MAC'
set -euo pipefail
PIPELINES="$(ssh -n root@192.168.8.184 "sleep 6 | luna-send -i -f luna://com.webos.media/getActivePipelines '{}'")"
PIPELINE_ID="$(
  printf '%s\n' "$PIPELINES" | python3 -c '
import json, re, sys
pipelines = json.load(sys.stdin)
matches = [
    x.get("id", "") for x in pipelines
    if x.get("type") == "tv" and x.get("is_foreground") is True
    and any(r.get("resource") == "HDMI_INPUT" for r in x.get("resource", []))
]
if len(matches) != 1 or not re.fullmatch(r"_[A-Za-z0-9_-]+", matches[0]):
    raise SystemExit("No unique, valid foreground HDMI TV pipeline")
print(matches[0])
'
)"
printf 'Foreground HDMI pipeline: %s\n' "$PIPELINE_ID"
for method in getSignalState getHDMIInfo getHDMIDiagnosticsInfo; do
  echo "===== $method ====="
  ssh -n root@192.168.8.184 \
    "sleep 6 | luna-send -i -f luna://com.webos.service.extinputs.hdmi/$method '{\"pipelineId\":\"$PIPELINE_ID\"}'"
done
MAC
```

**Do not infer HDCP status** from EIM's 5V/signal booleans. The missing
`pipelineId` error establishes part of the request schema, not which
HDMI protection status is available in any response.

## Current engineering policy

Do not infer an HDMI session is clear solely because `SVP_CPB` is absent.
Keep HDMI in `Unknown` pending a validated active protection signal;
test native histogram sampling independently. Preserve the 2s stability
policy and protected-session latch when later integrating an HDMI HDCP
detector. **No guessed DRM/HDCP state should be used for routing.**
