# LG C5 BLENDED screenshot backend (experimental, opt-in)

The verified C5 uses LG's own
`luna://com.webos.service.capture/executeOneShot` with
`{"method":"BLENDED","width":320,"height":180,"format":"png","path":"..."}`.
Root-shell tests produced real 320×180 PNGs containing moving screen imagery
without a persistent /dev/video60 owner. First-file latency was 170–220ms in
five samples, **not** sustained FPS.

This branch implements an **optional** snapshot backend using the same LG
service with our existing Rust colour processing and WLED/Hue/Nanoleaf outputs.
It is **not** a raw video streaming implementation, 30fps engine, DRM bypass,
or a replacement for the existing guarded VT path on all platforms.

## Selection

In an existing `/var/home/root/lg-hue-sync/config.json`, the experimental
option is:

```json
{
  "blended_capture_enabled": true,
  "capture_width": 320,
  "capture_height": 180
}
```

These are the **individual keys to add** to the existing JSON, **not** a
replacement for the full populated configuration. Existing Hue/Nanoleaf/WLED
credentials must remain unchanged. The default is `false`: old installations
retain prior behaviour.

This feature must only be evaluated on a user-owned TV with legitimately
unprotected media. The media classifier must return **Clear** continuously for
two seconds before the daemon requests an image. Unknown, protected, stale
media, loss of TV sync or user stop cancels the capture task and suppresses
further screenshots. LED output goes black if frames stop arriving.

Important: the existing classifier intentionally returns **Unknown** for the
HDMI_INPUT family and many UI-only states, even when LG's screenshot service
works. Do **not** broaden the media classifier based on screenshot success.
This means the backend can remain black on the launcher or HDMI, by design,
until those source types receive an independently verified clear policy.

## Backend behaviour

- One in-flight `executeOneShot` request maximum.
- Uses the existing bounded `luna-send -i -f` subprocess adapter with stdin
  held open; **one Luna client process per frame**, not a persistent session.
- Max screenshot geometry 640×360; cap compressed PNG at 2 MiB.
- Strict validation of returnValue, response dimensions, writtenBytes, PNG
  CRC, bit depth/colour mode, filters and decompressed size.
- Only non-interlaced RGB8/RGBA8 PNGs accepted; unsupported formats return a
  capture failure, never a guessed RGB conversion.
- Private 0700 temporary directory with one unpredictable image filename
  per capture, removed on completion/abort; no image persistence.
- RGB/RGBA decoded once and reused by existing vendor-neutral ColorPipeline.
- No `/dev/video60`, V4L2 ioctls, `libvtcapture` CPU pointer access,
  permissions/role mutation, or native capture buffer ownership.
- A successful `luna-send` subprocess call may rely on the subprocess's own
  LS2 identity. It does **not** establish permission for a future direct LS2
  client registered under the Rust daemon's executable identity.

## Safest first runtime check

**Do not deploy this branch until the ARMv7 build and tests pass.**

Once built, on a known-clear local media playback source only:

```sh
/var/home/root/lg-hue-sync/lg-hue-sync test-blended \
  --acknowledge-clear-capture --width 320 --height 180
```

This command requires three fresh Clear media-resource classifications and
tests one BLENDED capture under the running executable's environment. It
prints timing/dimensions and discards all pixels. It does not drive lights.

Only after that test succeeds, enable the JSON key in the existing config and
deploy/restart following `docs/operations.md`, with a backup of the prior
binary and config. To disable: set `blended_capture_enabled` to `false`
and restart. The original VT source remains available for other platforms.

## Next performance improvement

Our first iteration proves the correctness of the API and capture-data path,
not efficiency. Measure sustained sequential throughput before considering
more than ~5fps. Profile Luna startup, native capture, PNG encode and disk
write separately if possible. Explore `execute` and native LS2 client
lifecycle **without** assuming permission or buffer lease from the method
name. Don't overlap capture requests or run a second `/dev/video60` owner.
