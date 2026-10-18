# C5 histogram follow-up: lightingcontroller frame-colour path and KADP APR

**Date:** 2026-10-18. **Method:** offline ARMv7 ELF dynamic symbol and
Thumb disassembly of the user-provided
`c5-histogram-readonly.qVZTzZ.zip`; no TV connection, service invocation,
capture initialization, modification or policy change.

**Evidence hashes (full binary SHA-256):**
- `/usr/sbin/lightingcontroller`:
  `34de65f1f49949d9f7b9041b90e4de79539cbd1beedb5272b069ff5153342fd0`
- `/usr/lib/libkadaptor.so.2.0.1`:
  `9d817747df49847991740286f58ea28d92129304f12c78601be7c36dbfd11dd3`

No proprietary binaries, pixels, device IDs or sensitive TV data were
committed to this repository.

## 1. LG lightingcontroller's actual screen-colour calculation

The built-in service is a **frame-capture consumer**, not a client of
`vtCapture_getHistogram`. It imports
`vtCapture_create`, `vtCapture_initEx`,
`vtCapture_preprocess`, `vtCapture_process`,
`vtCapture_currentCaptureBuffInfo`,
`vtCapture_planeInfo`, cleanup functions, and related API entry
points. The four C `vtCapture_*Histogram` interfaces are not among
its dynamic imports.

The service's exported, local wrapper
`_vtCaptureCurrentCaptureBuffInfo(unsigned char*)` at ELF text
address `0x15e44` provides stronger, **direct** evidence:

```text
0x15e5e: call vtCapture_currentCaptureBuffInfo (PLT)
         verify returned buffer metadata/pointers and dimensions
0x15e76: compare metadata value against 0xc00 (3072)
0x15ec2: compare another value against 0x600 (1536)
0x15eca: r0 = 0x40 (64)
0x15ed0: r1 = 0x30 (48)
0x15ed8: call csc_YUV420toRGB (0x152fc)
0x15ee0: r0 = 0x0d (13)
0x15ee2: call kMeansClustering (0x15704)
```

That function only reaches the conversion/clustering calls after
successful VT buffer retrieval and its dimensional checks.
64 × 48 is 3,072 samples; the second dimension check is 1,536,
consistent with a YUV420 image plane arrangement. The precise
pointer/plane-layout ABI is **not** independently established and
should not be reconstructed as a safe zero-copy contract from these
instructions. The function's use of direct VT frame-buffer
pointers means it is **not** proof of a safe, authorized aggregate
colour API for protected third-party content.

`csc_YUV420toRGB(unsigned, unsigned, unsigned, unsigned char*,
unsigned char*, unsigned char*)` is an actual exported function.
`kMeansClustering(int, unsigned char*, unsigned char*)` is also
exported, and a direct call in this wrapper sets its first argument
to **13**. This supports *13 clusters/centroids*, **not** the claim
that 13 individually addressable screen regions or a 13-value LS2
result are available. Several internal video-sync call sites invoke
these VT wrappers, but the symbol table is stripped of many local
function boundaries, so avoid assigning unlabelled callers to
unproven named methods.

The image-to-light implementation is associated with global
`centroids`, `captureMapTable`, `isVideoSyncMode`,
`videoSyncModeCondition`, `ledController::sendLedData`
and `ledController::sendLedArrayData`. It appears to be a
device-internal path from screen-frame analysis to LG's own LED
microcontroller, not a data-return service for a WLED client.

Most significantly, the binary's own customer-facing text says:

> Video Sync Mode is only supported with HDMI inputs and cloud games,
> and most apps provide indirect lighting effects.

That is firmware-supplied text, **not** a test establishing which
content actually works on this particular TV. It should not be used
to infer that DRM-protected streaming apps yield accessible pixels.

The collected
`com.webos.service.lightingcontroller.api.json` lists only four
methods relating to gaming/ambient LED microcontroller firmware and
version information. It does **not** publish screen-region colours
or a histogram read endpoint. The service role permits
`com.webos.service.capture.client*` identities for its own
executable; it is not a license to register that identity from a
third-party program.

## 2. KADP HST strings are APR debug menus, not public histogram data

The earlier first-pass hypothesis was that
`libkadaptor.so.2.0.1` contained a hardware histogram lead,
based on strings `HST [0:Off,1:On,2:Fmt,3:HISTOGRAM,...]`
and `dump raw histogram`.

A closer strings-and-symbol cross-reference shows those strings
in a **KADP_APR automatic pattern recognition** debug-menu block:

```text
KADP_APR_SetControl / KADP_APR_GetInfo / KADP_APR_SetCommand
HST [0:Off,1:On,2:Fmt,3:HISTOGRAM,...]
FD3 [0:Off,1:On,2:Fmt,3:log,...]
2: dump data for script simulation
3: dump data for excel simulation
4: dump fd3
5: dump raw histogram
```

The ELF actually exports
`KADP_APR_Open`, `KADP_APR_Close`,
`KADP_APR_Initialize`, `KADP_APR_SetControl`,
`KADP_APR_GetInfo`, `KADP_APR_SetCommand` and
`KADP_APR_DEBUG_Test`. The `GetInfo` implementation
at address `0xe8c68` forwards to an internal device-control
dispatch; it is not a known public LS2 endpoint or
a specification for per-screen-edge RGB values.
**Do not call the APR control/debug functions** or try enabling
histogram diagnostics to bypass the stubbed VT API. The diagnostic
menu may operate on picture/pattern recognition internals and its
protection policy is unknown.

Two tempting exported KADP getters were checked at instruction
level and are also **stubs in this exact library build**:

```text
0x0c8800 KADP_BE_TOP_Get_OSDAlphaAPL: movs r0,#0; bx lr
0x0c8868 KADP_BE_LED_GetAplMinMax:   movs r0,#0; bx lr
```

They immediately return zero and do not write requested output
fields; they are not functional colour/brightness readers.

## 3. Final classification after this pass

| Candidate | Result | Implication for our C5 |
| --- | --- | --- |
| `VtHistogram` `initHistogram` | Previously confirmed unconditional `return 7` stub | No working regional histogram via this library |
| `lightingcontroller` video-sync pipeline | **Confirmed YUV frame-to-RGB, k-means(13) path** | Firmware reference only, not an aggregate histogram or safe DRM read API |
| `lightingcontroller` LS2 APIs | Only firmware/version methods exported | No authorized 13-cluster or region-colour data query |
| `KADP_APR_* / HST` debug | APR debug/device interface, not documented colour service | Do not invoke; no third-party contract identified |
| `KADP_BE_LED_GetAplMinMax` | Four-byte stub returning zero | No working getter in this build |
| `KADP_BE_TOP_Get_OSDAlphaAPL` | Four-byte stub returning zero | No working getter in this build |
| `livepick GetColorHistogram` | ImageMagick software histogram (first-pass audit) | Not a protected display aggregate provider |

**Conclusion:** the additional evidence narrows, rather than
expands, our protected-content options. We have found no
vendor-supported, callable **aggregate histogram result API**
for this C5. The built-in lighting path still uses its internal
VT frame capture. A producer on a factory-privileged capture path
must not be presumed available to third-party apps or safe to
reuse for DRM content.

**Recommended continuation:** keep the existing
`Protected → healthy histogram only; otherwise Hold/black`
policy. Pursue public/vendor-supported LED colour data or
authorized aggregate statistics contracts from LG documentation
or future firmware, without calling APR/debug devices,
reusing privileged identities, patching stubbed functions or
accessing protected display buffers. Clear-content Stage 7
JPEG analysis remains a separately bounded task.

This is a static ABI analysis; runtime support, permissions,
scene identity, and DRM behavior are unverified unless
explicitly documented above.
