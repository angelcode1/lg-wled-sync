# LG C5 histogram colour-profile collection

Use the built-in HyperDRM 0.13.0 calibration wizard from the user-supplied HyperDRM source (app/wizard.js, app/calibration.js).
**The C5 has no verified measured profile in the repository. Do not substitute the LG G4 profile.**

## Confirmed C5 Luna invocation

On this LG OLED42C5PSA.AAUQLJD (SDK 10), `luna-send -n 1` exits
with no text even on valid services. **Use interactive private Luna**:
keep stdin open long enough to receive one JSON reply, then stop the
client. The Rust daemon now implements this without blocking light output.

```bash
ssh root@192.168.8.184 'sleep 6 | luna-send -i -f luna://com.webos.service.tv.systemproperty/getSystemInfo '\''{"keys":["modelName"]}'\'''
ssh root@192.168.8.184 'sleep 6 | luna-send -i -f luna://com.webos.media/getActivePipelines '\''{}'\'''
```

The reported C5 media pipeline while HDMI1 is selected contains
`HDMI_INPUT`, `VHDMIRX`, `AHDMIRX` and `MAIN_SCALER` but not
`SVP_CPB`. This is **Unknown**, not proof of absent HDCP.
A protected internal streaming-app session still needs measurement.

The confirmed exact modelName is `OLED42C5PSA.AAUQLJD`. Use that exact
value when importing a measured C5 histogram profile, rather than an
illustrative model name like OLED65C5PSA. No profile has been measured yet.

## LG C5 kernel versus 32-bit application ABI

On the tested C5, `uname -m` reports **aarch64** because the TV runs a
64-bit **kernel**. That does **not** determine the executable ABI: this C5
runs **32-bit ARM userspace applications**. Continue targeting
`armv7-unknown-linux-gnueabi` for this fork unless an ELF inspection of
the *actual application and native libraries* proves a different userspace
ABI. The C5 reports glibc 2.35; an ARMv7 build limited to GLIBC <= 2.28 is
not inherently incompatible with newer glibc. Inspect dynamic loader,
EABI and native dependencies before deployment.

Do **not** replace the project ARMv7 Docker build with AArch64 based only on
`uname -m`, and do not reject a 32-bit HyperDRM IPK merely because the
kernel is 64-bit.

The initial noninteractive SSH commands yielded blank Luna replies and no native symbol
listing. The old collector swallowed `readelf` errors; an empty result did
**not** establish the absence of histogram symbols. The attempted launch
returned `{"errorCode":-101,"returnValue":false,"errorText":"not exist"}`:
confirm installed app/service paths and Luna availability before running
the region recorder or calibration wizard again.

The updated repository collector records `getconf LONG_BIT`, command
availability and Luna exit codes, and can copy the LG library for offline
ELF inspection on a Mac without modifying the TV:

```bash
git checkout feature/vendor-neutral-wled-drm-first
TV_HOST=192.168.8.184
bash scripts/c5_profile.sh "$TV_HOST" preflight
bash scripts/c5_profile.sh "$TV_HOST" pull-lib
file c5-evidence/libvtcapture.so.1.0.0
# If greadelf is unavailable: brew install binutils
greadelf -h c5-evidence/libvtcapture.so.1.0.0
greadelf -W --dyn-syms c5-evidence/libvtcapture.so.1.0.0 \
  | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)'
```

Check that the capture library is `ELF32` / `ARM`, and distinguish
unavailable `readelf` or `luna-send` from an API actually missing on C5.

## TV preflight (read-only)

    id
    uname -a
    getconf GNU_LIBC_VERSION
    luna-send -n 1 luna://com.webos.service.tv.systemproperty/getSystemInfo '{"keys":["modelName"]}'
    luna-send -n 1 luna://com.webos.media/getActivePipelines '{}'
    readelf -Ws /usr/lib/libvtcapture.so.1 | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)'
    ps | grep -E 'histogram-service|piccap|lg-hue-sync|lg-wled-sync' | grep -v grep
    luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}'

## Mac/Linux commands

    TV_IP=192.168.8.X  # replace with actual C5 IP
    ssh root@$TV_IP 'id; getconf GNU_LIBC_VERSION'
    ssh root@$TV_IP "luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}'"
    ssh root@$TV_IP "luna-send -n 1 luna://com.webos.applicationManager/launch '{\"id\":\"org.webosbrew.histogramhyperion\",\"params\":{\"action\":\"calibrate\"}}'"

Install HyperDRM 0.13.0 and elevate its own service through Homebrew before starting if it is not present.
HyperDRM's Start and calibration wizard also require a reachable Hyperion or HyperHDR JSON TCP receiver (normally port 19444) configured in HyperDRM's Receiver tab. WLED alone does not implement that JSON service; use an existing receiver or run a temporary receiver on your PC for the calibration session. The same receiver is needed for visual LED validation.
Stop any competing lg-wled-sync, lg-hue-sync or PicCap capture before starting.

Use the TV remote to open HyperDRM > Calibration > Start Auto Calibration. The wizard
plays a real 4-second black baseline and 32 perimeter patches for 2 seconds each
(approximately 70 seconds) and samples the 16 native LG histogram regions.
Keep the video full-screen without picture scaling. Save the proposed profile,
then run Validate profile; expected corners are red TL, green TR, blue BR, yellow BL.
Failure to find reliable regions on each edge means retain the default average profile.

## Fetch the resulting saved calibration

    ssh root@$TV_IP 'cat /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json' > lg-c5-calibration.json
    ssh root@$TV_IP "luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}'" > lg-c5-status.txt

Convert the measured 4-16 sorted rim points (positions 0..4, region indexes 0..15)
into the existing Rust config's histogram.profile=custom and histogram.points fields.
Keep the exact modelName stored alongside the measured profile; do not apply it
to other TVs. The Rust runtime now enforces model identity for custom spatial mappings.

## Optional read-only raw-region recording during the wizard

    ssh root@$TV_IP 'i=0; while [ "$i" -lt 360 ]; do echo "SAMPLE $(date +%s)"; luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status "{}"; echo END_SAMPLE; i=$((i+1)); sleep 0.25; done' > c5-regions.log

The service status includes sampleSequence, sampleAgeMs, colorValid and 16 RGB region arrays.
Inspect local endpoint metadata in these logs before sharing them publicly.

## Verification scope

A measured spatial histogram profile is NOT an HDR/SDR colour LUT, a white-point
measurement, or proof that DRM histograms work on C5. The C5 native ABI and protected
playback path still require on-device validation. Avoid running multiple VT owners.
Exact tested helper scripts are distributed in lg-c5-histogram-profile.zip in the conversation.
## Source-level test evidence (not a C5 TV test)

On a Linux build host, the provided HyperDRM 0.13.0 source passed:

- node tests/calibration.test.js (patch fitting, missing/global regions)
- gcc -Wall -Wextra -Werror -O2 native/test_histogram.c -o test-histogram && ./test-histogram
- gcc -Wall -Wextra -Werror -O2 native/test_playback.c -ldl -o test-playback && ./test-playback

These do not validate our Rust fork or the C5's native device behaviour.

## Model-bound import (2026 development branch)

The fork now enforces a model guard before using spatial histogram mappings:
- The saved HyperDRM calibration must contain the exact `model` returned by the C5 system property endpoint.
- A custom profile needs at least 4 measured points covering all four screen edges.
- The runtime reads the TV model via `getSystemInfo` and refuses mismatched/unknown custom or G4 spatial mappings rather than borrowing the G4 profile.
- The default averaged-colour histogram profile remains usable if the C5 has not been calibrated.

The repo contains a preflight/measurement helper and an importer:

```bash
git checkout feature/vendor-neutral-wled-drm-first
export TV_HOST=192.168.8.X    # replace with actual LG C5 IP
bash scripts/c5_profile.sh "$TV_HOST" preflight
bash scripts/c5_profile.sh "$TV_HOST" record  # Terminal A; samples status
bash scripts/c5_profile.sh "$TV_HOST" launch  # Terminal B; explicitly opens HyperDRM wizard
# Save the profile with the remote on the TV.
bash scripts/c5_profile.sh "$TV_HOST" fetch
python3 scripts/c5_profile_import.py c5-evidence/saved-calibration.json \
  --model 'OLED65C5PSA' --output c5-histogram-config.json
python3 -m unittest discover -s scripts -p test_c5_profile_import.py -v
```

Replace the example model with **the exact modelName in preflight.txt**, not a guessed model/size. Merge the output fragment with the existing daemon config; it is not a complete config.json. All evidence files remain local and should not be committed to GitHub because system status may contain private network details. Do not start the development daemon while the HyperDRM calibration wizard owns VT_CAPTURE.

The helper's `preflight`, `record`, and `fetch` modes only read from the TV. `launch` explicitly opens the installed HyperDRM calibration app, and the TV's own wizard performs measurement. The helper does not root the TV, install a service, change configuration or enable autostart.



## C5 observation on 2026-10-12: missing service and blank Luna replies

The owner confirmed through root SSH that neither
\`/media/developer/apps/usr/palm/applications/org.webosbrew.histogramhyperion\`
nor
\`/media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service\`
exists. The earlier launch returned \`{"errorCode":-101,"returnValue":false,"errorText":"not exist"}\`.
No calibration observations can be collected until the service is deliberately installed.

\`luna-send -n 1\` returned an empty stdout with exit status 0 for both
\`getSystemInfo\` and \`getActivePipelines\`. That DOES NOT prove those
services are healthy or expose usable replies. The updated diagnostics use
\`-f\` for formatted JSON and \`-w 2000\` for a bounded response timeout,
as documented in the official webOS luna-send usage guide. Try known-good
endpoints, and \`luna-send-pub\` if installed, to distinguish bus/endpoint
failure from an unavailable method. Playback remains \`Unknown\` if no
parseable JSON arrives; measured spatial profiles cannot be applied without
an exact model identity.

\`\`\`sh
ssh root@192.168.8.184 'sh -s' <<'TV'
for tool in luna-send luna-send-pub; do
  command -v "$tool" || true
done
echo '=== Test known system endpoint ==='
luna-send -n 1 -f -w 2000 \
  luna://com.webos.settingsservice/getSystemSettings \
  '{"keys":["localeInfo"]}'
echo "exit=$?"
echo '=== C5 model ==='
luna-send -n 1 -f -w 2000 \
  luna://com.webos.service.tv.systemproperty/getSystemInfo \
  '{"keys":["modelName"]}'
echo "exit=$?"
echo '=== C5 media (try with a video playing) ==='
luna-send -n 1 -f -w 2000 \
  luna://com.webos.media/getActivePipelines '{}'
echo "exit=$?"
TV
\`\`\`

No install or capture initialization is included in that diagnostic block.
The collector's \`record\`, \`launch\` and \`fetch\` modes now reject the
missing service, app and calibration file, respectively, rather than
generating empty "success" logs.

