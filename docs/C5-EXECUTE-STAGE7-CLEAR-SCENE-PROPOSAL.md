# Stage 7 — stationary clear-scene comparison; histogram DRM policy preserved

**Status: source/design only. No Stage 7 TV capture or LS2 policy
change authorized.** The owner previously approved and completed
Stage 5. Stage 6 is an offline JPEG decoder and quality audit
that passed on the owner's Mac with Pillow 11.3.0 and in GitHub CI.

## Non-negotiable DRM routing preserved

The live router (`src/capture/drm_routing.rs`) is DRM-first:
- Only stable `Clear` permits normal frame capture.
- `Protected` selects `Histogram` only when the health signal is
  `Valid` and capture mode permits it. Otherwise it selects `Hold`.
- `Unknown` never authorizes frame capture. A protection latch
  survives Unknown until stable Clear.
- The daemon (`src/main.rs`) cancels any BLENDED request when the
  route is not `Frame`, drops VT frame ownership, and never supplies
  synthetic MockCapture pixels as actual scene colors. With no
  current histogram field, the lights hold for at most 500 ms and
  then receive black.

**LG C5 exception:** `src/capture/histogram.rs` fingerprints this
TV's `libvtcapture.so.1` by SHA-256. In the known C5 build,
`VtHistogram::initHistogram` returns 7 unconditionally and the
`getHistogram` API returns statuses without writing histogram bins.
The module refuses this exact build before native initialization.
This means **histogram remains the preferred architectural path
for protected-content ambient lighting but is not operational
on this particular C5**. Do not describe the Stage 4B/5 BLENDED
capture as functional DRM capture or use it as a substitute for
protected video. The installed TV still has its original hash-
verified Stage 1 executable/policies; none of the post-Stage-1
capture prototypes runs as a persistent production service.

## Grounded motivation

Stage 5's known-clear test:
- Native `execute`: 5/5 JPEGs, mean RPC 151.6 ms, mean 4,641.6 B,
  total measured CPU 97.23 ms and phase wall 969 ms.
- `executeOneShot`: 5/5 JPEGs, mean RPC 162.0 ms, mean 8,877.2 B,
  total measured CPU 145.71 ms and phase wall 892 ms.
- Different scene frames and unknown one-shot JPEG quality
  prevent a valid quality-matched performance conclusion.
- The production BLENDED fallback currently uses PNG, not JPEG.
- Stage 6's Pillow audit fully decodes JPEGs offline but prior
  test frames were intentionally deleted.

## Offline work allowed without TV authorization

1. Use `scripts/c5_jpeg_audit.py --selftest` with synthetic frames
   and pinned, Mac-compatible Pillow 11.3.0.
2. Review which `setOptions` and one-shot parameters are actually
   documented and confirmed. Never infer quality settings from
   compressed byte lengths alone.
3. Maintain tests ensuring Protected/Unknown routes cannot invoke
   `execute` or BLENDED, and that a known-stub C5 histogram backend
   cannot be considered healthy.
4. Keep production PNG and app permissions unchanged.

## Proposed next real-device experiment — requires its own approval

Capture at most one 320×180 JPEG by each already proven clear-only
path while a **stationary, personally controlled DRM-free local
video/still** is playing and independently classified `Clear`.
The scene must not change between requests; unknown, HDMI or
protected classifications abort without any capture. Use the
existing dedicated app-owned LS2 name, a temporary `capture.client`
ACG, and exact automatic/manual Stage 1 restoration with SHA receipts.
Destroy any capture handle even if validation fails; never retry
automatically on lost acknowledgement.

Perform full JPEG decode and image-detail/edge-color comparison
only in memory in a private ephemeral directory on the TV if a
safe decoder implementation is available. Otherwise use a separately
authorized transfer of just these owner-controlled clear stills to
the Mac; never upload frame pixels, log them or retain them by
default. Quantization table fingerprints and sampling factors are
useful but do not constitute authoritative encoder-quality
verification. Verify decode and usefulness of both images first;
repeat quality-matched performance measurements only after those
conditions are met.

**No DRM bypass, factory-role impersonation, unbounded screen
capture, or access to protected pixels is part of this stage.**

## Separate future DRM functionality research

If functional ambient colors from protected content are essential,
investigate *authorized, vendor-supported* aggregate histogram or
lighting-data APIs, or optional content-provider metadata, for
this exact C5 model. Record firmware-dependent evidence and
permission contracts before any device experiment. Do not assume
the G4 histogram ABI can be enabled on C5 by altering flags or
granting extra LS2 privileges; the currently fingerprinted C5
vendor implementation is stubbed.
