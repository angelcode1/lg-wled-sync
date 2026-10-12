# LG C5 histogram colour-profile collection

Use the built-in HyperDRM 0.13.0 calibration wizard from the user-supplied HyperDRM source (app/wizard.js, app/calibration.js).
**The C5 has no verified measured profile in the repository. Do not substitute the LG G4 profile.**

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
to other TVs. The Rust runtime does not yet enforce model identity.

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
