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

## Current engineering policy

Do not infer an HDMI session is clear solely because `SVP_CPB` is absent.
Keep HDMI in `Unknown` pending a validated active protection signal;
test native histogram sampling independently. Preserve the 2s stability
policy and protected-session latch when later integrating an HDMI HDCP
detector. **No guessed DRM/HDCP state should be used for routing.**
