# LG C5 histogram-only hardware probe (opt-in)

This test targets the owner's LG OLED42C5PSA.AAUQLJD (webOS SDK 10),
which reports a 32-bit ARM EABI5 soft-float application ABI, aarch64
kernel and all four `vtCapture_*Histogram` symbols.

## What the probe does

- Calls the existing native Rust `HistogramCapture::try_new()` and
  `sample()` functions. No HyperDRM, HyperHDR or WLED is required.
- **May temporarily create** `/var/luna/preferences/vt_devmode` if absent,
  then automatically remove **only its own file** via the marker guardian.
- Acquires LG's exclusive VT histogram device for a few seconds.
  Stop competing PicCap, HyperDRM, or other capture clients first.
- Prints weighted RGB means for all 16 physical histogram regions,
  for 8 samples at a 600ms interval by default. No raw image/frame/pixel
  data is retained, no config is loaded, and **no LED/network output** occurs.
- Always requests native finalization when exiting normally.
  A native driver crash or hang cannot be ruled out until tested on hardware.
- Requires an explicit `--acknowledge-native-capture` switch. It does **not**
  automatically run, install a daemon, start on boot, or change HDCP policy.

This is a diagnostic of LG-provided **aggregate** capture measurements,
not a means of accessing DRM-decrypted frames.

## Build and download

GitHub Actions on the `feature/vendor-neutral-wled-drm-first` branch runs
full Rust unit tests/Clippy and cross-compiles for
`armv7-unknown-linux-gnueabi` with GLIBC <= 2.28. After a **green**
workflow run, its **ARMv7** job attaches a short-lived
`c5-histogram-probe-armv7` artifact containing `lg-hue-sync`
(7-day retention). Download it from the workflow's Artifacts panel.

Optional GitHub CLI on Mac:

```bash
gh run download RUN_ID --repo angelcode1/lg-wled-sync \
  -n c5-histogram-probe-armv7 -D ~/Downloads/c5-histogram-bin
file ~/Downloads/c5-histogram-bin/lg-hue-sync
```

## TV preflight and explicit hardware test

First confirm that no other capture owner is running; do not kill
TV media pipelines, `ls-hubd`, or `extinput-integration`.

```bash
ssh root@192.168.8.184 \
  'ps | grep -Ei "piccap|hyperdrm|histogramhyperion|lg-hue-sync" | grep -v grep || true; \
   ls -l /var/luna/preferences/vt_devmode 2>&1 || true'

scp ~/Downloads/c5-histogram-bin/lg-hue-sync \
  root@192.168.8.184:/var/home/root/c5-histogram-probe
ssh root@192.168.8.184 'chmod 700 /var/home/root/c5-histogram-probe'

# Start a changing scene on the Fire TV's Prime Video app through HDMI1.
ssh -t root@192.168.8.184 \
  '/var/home/root/c5-histogram-probe test-histogram \
   --acknowledge-native-capture --samples 8 --interval-ms 600' \
  | tee ~/Downloads/c5-histogram-test.log

# Inspect the marker after the test (if present before, it is left alone).
ssh root@192.168.8.184 'ls -l /var/luna/preferences/vt_devmode 2>&1 || true'
```

Expected line format:
`HISTOGRAM_SAMPLE 1/8 changed_regions=0 00=RRR,GGG,BBB ... 15=RRR,GGG,BBB`.
`HISTOGRAM_RESULT` reports the count of valid and failed samples and
how many consecutive successful pairs included at least one meaningful
(4/255 RGB channel) region change.

**Interpretation**:
- Init fails: investigate error without altering TV HDMI or DRM settings.
- Init succeeds but zero channels / samples invalid: hardware not yet
  validated; avoid publishing a calibration.
- Two or more valid samples, some changes: aggregate histogram API appears
  functional on this specific HDMI session. This does **not** identify
  screen-edge geometry, prove HDCP authentication state, or establish
  DRM-secure coverage.
- Valid but no changes: inconclusive if the scene is static; compare
  changing scenes before claiming success or failure.

Do not add a G4 spatial mapping to C5 based on the 16 region IDs.
Spatial mapping requires a separate measured perimeter calibration.

## Current DRM routing

The Fire TV Prime Video HDMI1 resource envelope is still `Unknown`:
`SVP_CPB` is absent, but that is not evidence of an unprotected HDMI
link. This hardware histogram test bypasses **only the Rust router
selection**, not any content or HDCP protection system.
