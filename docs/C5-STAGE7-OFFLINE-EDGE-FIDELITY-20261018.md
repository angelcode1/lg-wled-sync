# LG C5 Stage 7: offline perimeter-colour fidelity gate (2026-10-18)

**Status:** The offline-only synthetic fidelity audit is implemented,
passing locally, and added to CI. It has **not** tested a TV frame,
changed the production capture backend, installed new permissions, or
initiated capture. The C5's verified Stage 1 baseline remains the
operational device state. Protected and Unknown sources must never
be used for these experiments.

## What this fixes in the evidence chain

In Stage 5, 5/5 native and 5/5 one-shot 320×180 JPEGs passed
SOI/EOI/SOF marker validation. However, the raw JPEGs were deleted
after validation and their decoded content cannot be assessed
retroactively. The native request averaged 151.6 ms, one-shot
162.0 ms; CPU was 97.23 versus 145.71 ms in a five-frame
sample. JPEG sizes differed substantially, and matched quality
or identical scene contents were not proven.

Stage 6 added `scripts/c5_jpeg_audit.py` using the
Mac-compatible `Pillow==11.3.0` for full JPEG decoding,
quantization tables, sampling, coarse edge RGB means, and
two-image comparison. Its `--selftest` passed on the
owner's Mac and in CI.

Stage 7 adds `scripts/c5_stage7_edge_fidelity.py`,
which imports the existing bounded, full-decoding Stage 6
validator rather than duplicating its JPEG parser.

### Synthetic reference and validation

The Stage 7 test generates **only in memory** a 320×180
picture containing four different perimeter swatches:
top (235,20,25), right (20,205,60), bottom (30,50,220),
left (220,160,25), with a dark interior.

Two synthetic JPEG representations of that same source
are encoded at quality 75 and 95, with 4:4:4 sampling.
After **full JPEG decode**, the audit measures the
center of each perimeter edge, avoiding corner overlap:

| Edge | Center-swatch rectangle (left, top, right, bottom) |
| --- | --- |
| Top | (80, 0, 240, 18) |
| Right | (288, 45, 320, 135) |
| Bottom | (80, 162, 240, 180) |
| Left | (0, 45, 32, 135) |

Its assertions prove, for this synthetic example:

1. Each edge's decoded mean RGB agrees with the
   original palette to within 8 digital channel values.
2. Both encodings' edge means differ by at most 8
   channel values despite different quantization tables.
3. Rotating one decoded image 180 degrees produces an
   edge discrepancy of at least 100 channel values,
   so orientation mistakes are detected.
4. A genuinely black image is valid: its edge means
   remain at most 1. No heuristic automatically rejects
   black frames as DRM or encoding failures.

Expected result:

```text
C5_STAGE7_OFFLINE_EDGE_FIDELITY_PASS decoded=true four_edges=true palette_accuracy=true orientation_mismatch_detected=true black_valid=true capture=false no_images_written=true
```

For user-supplied, **already existing**, locally-held
320×180 JPEGs of the same stationary *known-clear* source:

```sh
python3 scripts/c5_jpeg_audit.py --selftest
python3 scripts/c5_stage7_edge_fidelity.py --selftest
python3 scripts/c5_stage7_edge_fidelity.py \
  --native ./native-static.jpg --oneshot ./oneshot-static.jpg
```

The two-file audit prints only derived dimensions,
JPEG byte lengths, per-edge mean RGB, quantization
and chroma-subsampling comparability, and differences.
It does **not** save decoded frames or transmit pixels.

**Limits:** the source may be a different scene,
a legitimate plain black frame, a DRM-protected source,
or an unexpected display plane; decoded edge statistics
alone cannot prove provenance, authorization,
perfect RGB colorimetry, or acceptable sustained FPS.
Do not apply synthetic pass/fail thresholds to real
scenes without a verified original color reference.
Do not interpret *same* quantization tables as proof
of equivalent encoder quality settings.

## Capture service quality hypothesis remains unproven

The Stage 5 reusable handle explicitly called
`setOptions` with `options.jpegQuality=75`,
while its experimental `executeOneShot` request
sent `jpegQuality=75` at the request root.
The returned one-shot JPEGs were not retained, nor
was their quantization checked. Therefore it is
**not known** whether LG honors that one-shot field,
and no conclusion about relative encoder efficiency
or input fidelity is justified.

The production `src/capture/blended.rs` route
still uses `executeOneShot` with PNG and has a
separate PNG decoder. The Stage 7 JPEG analysis
is not by itself a production PNG/JPEG comparison.

## Next decisions

- If the owner authorizes a new, bounded TV test,
  use one stationary, personally controlled, local,
  independently verified DRM-free video or still.
  Revalidate `Clear` before every capture, use only
  temporary app-owned LS2 permissions, tear down
  the native handle and restore Stage 1 with
  independently verified SHA-256 receipts.
- Assess full decode and actual edge-color usefulness
  before any higher-frame-rate experiment. Keep
  the source static and compare the same image.
- Do not repurpose the C5's known-stub
  `VtHistogram` for protected frames, install the
  unconfirmed optional Hue Sync service, copy LG
  roles, or call APR/debug diagnostics. The
  current protected policy remains
  `Protected → healthy verified histogram, else Hold/black`.
- The `pqcontroller/getEyeqSensorData` method observed
  in the C5 API census is a **room-light sensor**
  lead for optional LED brightness, not a
  regional picture histogram. Runtime access and
  return schema are not proven.

**No Stage 7 TV run is authorized by this document.**
