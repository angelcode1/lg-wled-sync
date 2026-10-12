# Vendor-neutral output API / DRM-first WLED fork

Development branch based on upstream lg-hue-sync v0.6.1.

## Working code introduced
- \`src/output/mod.rs\`: \`LightFrame\` with \`ColorSource\`, \`LightOutput\` trait, Hue and Nanoleaf adapters.
- \`src/output/wled.rs\`: WLED UDP DRGB/DNRGB frame sender; port 21324 by default, chunking for >490 LEDs.
- \`src/output/perimeter.rs\`: configurable screen-edge layout and frame-derived RGB LED sampling. Never assume WLED's 40 LEDs are Nanoleaf panel IDs.
- \`src/config.rs\`: optional \`wled_devices\` and \`wled_sync_enabled\`. Fresh configurations disable Hue by default; no WLED address is invented. Existing configurations deserialize without the new fields.
- \`src/web/server.rs\`: read-only GET \`/api/outputs\` returns vendor status; WLED \`last_send_ok\` means local UDP syscall success only.
- \`src/capture/drm_routing.rs\`: independent DRM-first selection policy and tests, **not yet wired into the capture loop**.

## Not implemented (do not claim operational)
- HyperDRM native histogram FFI, ABI validation, calibrated region mapping, playback DRM detection and capture ownership/switching.
- Runtime use of \`DrmCaptureRouter\`; the current daemon still runs its existing frame capture on TV.
- Fully vendor-neutral runtime for all transports. Existing Hue and Nanoleaf management retains current logic, only their streaming paths use adapters.
- Web UI controls for WLED configuration, device enumeration/discovery or live enable/disable.
- Bidirectional programmable output HTTP API: \`/api/outputs\` is read-only status. \`LightOutput\` is the vendor-agnostic internal API.

## Future histogram integration
DRM -> select histogram immediately; clear -> frame; unknown while protected -> keep histogram latch; require 2s stable clear to return. If histogram unavailable or calibration invalid, hold at most 500ms then cease output/blackout. Do not fabricate RGB frame from histogram bins. Never import HyperDRM ARM binaries with GLIBC 2.34 into this older webOS target.

## Proposed WLED-only configuration (full config retains other existing required fields)
\`\`\`json
{
  "hue_enabled": false,
  "hue_sync_enabled": false,
  "wled_sync_enabled": true,
  "wled_devices": [
    {
      "ip": "192.168.1.50",
      "led_count": 240,
      "start_corner": "bottom-left",
      "reverse_direction": false,
      "perimeter_offset": 0,
      "brightness_trim": 1.0
    }
  ]
}
\`\`\`

## Tests / deployment gates
\`\`\`sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --bin lg-hue-sync -- -D warnings
make build
\`\`\`
No Rust compiler or ARM builder was available in the editing environment. Run CI, then confirm binary and glibc compatibility before installing on TV. Do not test lights or modify TV system settings automatically.
