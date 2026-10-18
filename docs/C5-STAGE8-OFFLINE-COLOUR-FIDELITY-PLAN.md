# LG C5 Stage 8 — offline colour fidelity and capture architecture

Status: offline-only design and executable synthetic verification. No capture, device mutation, new ACG permission, light output, dashboard changes, or DRM route changes. Stage 1 remains the owner-verified device baseline.

## What the real C5 experiments establish

| Quantity | Stage 7 first clear pair | Stage 7 second clear pair |
| --- | ---: | ---: |
| Native JPEG / native request | 7,799 B / 161 ms | 5,550 B / 151 ms |
| One-shot JPEG / request | 14,441 B / 160 ms | 9,032 B / 161 ms |
| Decoded mean RGB difference | 10.405 | 2.213 |
| PSNR | 19.42 dB | 29.04 dB |
| Worst central edge channel disagreement | 12.27 | 0.36 |
| Quantization tables match | No | No |
| Independently measured reference palette | No | No |

Both pairs fully decoded. Native handles were destroyed. The owner separately verified exact Stage 1 restoration and absence of remaining `/tmp/c5-ls2-stage7-*` directories. The later host-side shell heredoc/PID bug was fixed on the Mac and passed static/syntax checks; its fixed `wait` has not been verified in a further live run, and *no further capture is needed to troubleshoot this host-only issue*.

The second pair's edges were nearly black: matching dark values is not the same as reproducing independently known chromatic edges. The first run also had no independent reference or proof of identical source plane/frame. Neither establishes physical LED colour accuracy, matched JPEG encoder quality, source provenance or sustained throughput. Protected, Unknown and HDMI remain outside the approved clear-only test.

## Stage 8 colour gate (offline)

`scripts/c5_stage8_reference_gate.py` implements a synthetic-only selftest and an optional three-file mode using **existing locally held** 320×180 images. It does not capture or contact a device. It reads an *independently authored* PNG reference and two JPEGs that the operator independently knows depict that same static, unprotected frame. It fully decodes into memory; checks the centre of the four edges; compares each image to the reference, not just to the other image; and reports only numeric differences and criteria. It refuses malformed, oversized, wrong-format, wrong-dimension, symlink and special-file input.

Its **strict four-chromatic-edge acceptance** is only meaningful for a deliberately designed reference stimulus, not arbitrary film frames. Each reference edge must have an encoded RGB channel reaching 64, channel spread of 40 and pairwise separation of at least 50 from every other edge. Otherwise outcome is **INCONCLUSIVE**, even if both JPEGs match perfectly. An informative reference must also have at most 12 nominal RGB channel-value mean error at every centre edge in **both** decoded JPEGs, or the verdict is **REFERENCE_MISMATCH**. These are provisional laboratory thresholds, not calibrated physical tolerances. It intentionally does not label a matching all-black scene as successful RGB fidelity.

A `SYNTHETIC_OR_REFERENCE_RGB_MATCH` verdict is **not** confirmation that a live screen really displayed the reference, that the two capture paths saw the same video plane, that JPEG quality settings matched, that HDCP/DRM access is possible, or that LED chromaticity is accurate. The synthetic fixture includes four distinct colour edges and deliberately different JPEG encodings. Offline regressions ensure rotated output, two identical wrong captures, a dark reference, and corrupted JPEG data never receive a false match.

### Commands (Mac, no TV interaction)

```sh
cd <checkout-of-lg-wled-sync>
python3 -m venv .venv-c5-stage8
.venv-c5-stage8/bin/python -m pip install 'Pillow==11.3.0'
.venv-c5-stage8/bin/python scripts/c5_stage8_reference_gate.py --selftest

# Only if these files already exist and you independently verified the
# exact same controlled, stationary DRM-free reference image was used:
.venv-c5-stage8/bin/python scripts/c5_stage8_reference_gate.py \
  --reference ./reference-320x180.png \
  --native ./native-320x180.jpg \
  --oneshot ./oneshot-320x180.jpg
```

**Do not use the Stage 7 terminal metrics as the reference image.** Both prior experiments deliberately removed source image bytes. A future calibrated device capture would be a separate request requiring explicit approval. This stage neither needs nor authorizes it.

## Performance architecture and decision gates

The installed production clear-content BLENDED fallback uses `executeOneShot` **PNG** and a custom PNG decoder (`src/capture/blended.rs`). Stage 7 measured the experimental **JPEG** paths, not the production PNG end-to-end path. The shared `ColorPipeline` normalizes frames into a 32×18 field, does colour processing once upstream, then maps independently to Hue/Nanoleaf/other output vendors. Do **not** duplicate saturation/temporal processing in protocol adapters.

- **Current verified clear-only LS2 JPEG requests:** ~151–161 ms each. Even ignoring media queries, decoding and output, a strictly sequential request loop would have an optimistic ceiling of ~6.2–6.6 captures/s. This is not a measured sustained frame rate or a promise of smooth output.
- **Potential session reuse:** removes client/session setup overhead but did not reduce the observed single request below 151 ms in Stage 7. No evidence yet supports 30–60 fps.
- **Production one-shot PNG:** must be independently benchmarked with frame decode, filesystem cost, time-to-first-frame, wall-time distribution, timeouts and stale-frame behaviour. Do not transfer JPEG conclusions to PNG.
- **Native VT/HAL/OSD route:** existing C5 fingerprint guards mean the probed `VtHistogram` implementation is unusable; alleged raw-buffer or protected paths remain unverified and unsafe to treat as available. Any future faster authorized clear-content compositor path needs proven ownership, source correctness, bounded access, DRM-first fail-closed policy and explicit device authorization.
- **Authorized non-pixel aggregate path for protected content:** investigate vendor-supported aggregate colour data only. The current C5 histogram stub is not a solution. Protected/Unknown must hold/black rather than silently capture protected pixels.

A useful *future* capture architecture should specify a **freshness budget** (capture age and end-to-end latency separately), frame-drop policy, bounded memory/CPU on ARMv7, source provenance, device capture privileges and protected/unknown fail-closed tests. At 30 fps a complete frame budget is 33.3 ms; at 60 fps it is 16.7 ms, before accounting for light/network latency. This is an architectural target, not a Stage 7 measurement.

## Exit criteria before any production switch

1. Bright, independent four-edge reference agreement with colour-orientation and black-scene negative controls; do not mislabel near-black agreement as accurate RGB reproduction.
2. Media-source identity and independently confirmed clear-content provenance; no HDMI or protected fallback.
3. Fair format/quantization comparison and p50/p95/p99 wall/request/decoding measurements from **separately approved** bounded tests, not one synthetic sample; stable thermal/CPU behaviour.
4. Freshness/timeouts, process cleanup and rollback correctness, including host SSH/PID lifecycle; image output remains private and ephemeral.
5. Default production and Stage 1 untouched until all gates pass and the owner explicitly approves promotion.