# LG DRM-first histogram capture + vendor-neutral output (development)

Branch: feature/vendor-neutral-wled-drm-first
Base: adeze/lg-hue-sync v0.6.1
Reference: user-supplied HyperDRM-main.zip (HyperDRM 0.13.0)

## Implemented in the feature branch

- Normal frame capture remains based on the existing libvtcapture / DILE_VT code.
- Playback classification polls the read-only Luna getActivePipelines endpoint at a one-second ceiling.
- A resource named SVP_CPB classifies the session as protected immediately.
- Clear requires a foreground media decoder with VDEC and no malformed or secure resource; HDMI, idle and missing replies are unknown. Classification is known only from the G4 resource model.
- Auto DRM-first routes protected sessions to LG histogram capture. Unknown does not clear a previously protected latch; returning to frame capture requires two seconds continuously clear.
- The histogram ABI has a 0x604-byte record per region, with 16 regions and RGB channels of 256 16-bit bins. Each channel uses a 64-bit weighted mean; empty regions are invalid, not black.
- The normal VT owner is released before histogram initialization. After histogram processing is finished, its handle is released before frame capture is reacquired. Pause releases histogram ownership.
- Native histogram initialization briefly creates the LG developer marker as needed and launches a bounded child guardian to remove ONLY the marker this daemon created, preserving existing markers.
- WLED, Hue and Nanoleaf receive vendor-neutral RGB frames via their individual LightOutput adapters. Each device maps its own LED geometry without changing the capture backend.
- Failed protected capture holds real colours for at most 500ms before explicitly sending black. It never uses MockCapture frames as protected-content colours.
- GET /api/status reports capture_source (frame, histogram, histogram-waiting) and playback_class (unknown, clear, protected).
- GET /api/outputs is a read-only versioned output inventory. WLED last_send_ok is local UDP syscall success, not a receiver acknowledgement.

## Configuration: additive fields to existing full config.json

    "capture_mode": "auto-drm-first",
    "histogram": { "profile": "average", "points": [] },
    "wled_sync_enabled": true,
    "wled_devices": [{
      "ip": "192.168.1.50",
      "udp_port": 21324,
      "led_count": 240,
      "start_corner": "bottom-left",
      "reverse_direction": false,
      "perimeter_offset": 0,
      "brightness_trim": 1.0
    }]

capture_mode: auto-drm-first (default), frame-only, histogram-only.
Histogram profile: average (default), g4 (LG G4 measured), custom (2..16
sorted perimeter points with position in [0,4) and region in 0..15).
No other LG TV receives a guessed spatial mapping. Unknown models use
a single average colour until independently calibrated.
WLED is the default type for new installations but requires an explicit
IP address; existing Hue/Nanoleaf configurations remain loadable.

## Not yet established

- This Rust code has not passed cargo fmt, cargo nextest, clippy or ARMv7
  cross-build in this editing environment: Rust tools were unavailable.
- The updated CI workflow includes this feature branch, but GitHub returned
  no workflow runs during inspection. Actions might need enabling on the fork.
- No LG TV deployment or hardware video tests were performed.
- Runtime ABI, exported LG logging symbols (e.g. printLog), rooted marker
  permissions, process diagnostics and VT contention all require target checks.
- Per-TV spatial calibration, capture-to-light latency and HDR colour transfer
  have not been measured.
- WLED dashboard provisioning and a bidirectional vendor-output management
  HTTP API are not yet implemented. LightOutput is the internal vendor API.

## Validation commands (host)

    cargo fmt --all -- --check
    cargo nextest run --locked
    cargo clippy --bin lg-hue-sync -- -D warnings
    make build
    file target/armv7-unknown-linux-gnueabi/release/lg-hue-sync

## Read-only TV evidence, before installation

    readelf -Ws /usr/lib/libvtcapture.so.1 | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)'
    luna-send -n 1 luna://com.webos.media/getActivePipelines '{}'

The real TV may return a different schema. Do not generalize this G4
classifier without examining that evidence. Do not run two independent
VT_CAPTURE owners such as PicCap and this daemon simultaneously.

## Source traceability

HyperDRM ZIP paths used: native/histogram.h, native/playback.h,
native/service.c, native/service_api.h, native/capture_loop.h,
native/handover.h, native/calibration.h, native/layout.h,
native/continuity.h, native/test_histogram.c and native/test_playback.c.
The tests from that original C source passed locally, but those tests do
NOT validate the new Rust FFI or device compatibility.
