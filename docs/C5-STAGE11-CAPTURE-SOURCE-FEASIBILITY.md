# LG C5 Stage 11 — capture-source feasibility and provenance audit

**Status:** reviewed source contracts plus an executable **offline-only** evidence gate. Owner's Stage 9/10 logs passed the bundled SHA-256 and synthetic test checks; Stage 10 GitHub CI completed successfully. **No TV connection, capture request, privilege update, vendor binary execution, device buffer read, HDR/DRM experiment, production code change, or light output was performed for Stage 11.** The Stage 1 baseline remains unchanged.

## Actual findings, not inferred capabilities

1. **BLENDED PNG, currently opt-in in Rust.** `src/capture/blended.rs::BlendedFrame` contains `pixels`, `width`, `height`, `finished: Instant`, `request_ms`, `decode_ms`, `png_bytes`. Its `executeOneShot` reply validation accepts `returnValue`, `capturedWidth`, `capturedHeight` and `writtenBytes`, then validates and decodes a private PNG file. `finished` records the **end of the local processing operation**, not the TV content's source-sampling timestamp. The code has **no verified source-sampling timestamp field**, clock origin, or age bound. The bounded private PNG file is an independently owned artifact and **does not have the VT ring-buffer pointer hazard**, but image availability does not prove source freshness, full-video coverage or deployed daemon LS2 permissions. The Rust helper launches an interactive `luna-send -i -f` subprocess per request; its underlying identity must not be silently equated with either the Stage 7 temporary `capture.client` role or a future persistent direct LS2 client.
2. **Stage 7 native-handle and one-shot JPEG.** The owner approved two known-clear pairs and observed native request times **151/161 ms** and one-shot **160/161 ms**. No captured source timestamp, sustained fresh cadence, independent bright reference or equal JPEG quantization was established. Native `destroyHandle` succeeded, Stage 1 was hash-verified and restored, and the host heredoc PID fix was later validated offline; its end-to-end exit-status path was not rerun. The Stage 7 temporary role is not proof of *production* client entitlement. **Do not substitute the JPEG request times for production PNG throughput.**
3. **64×48 vendor lightingcontroller YUV.** The owner-supplied native-code review found a 3,072-byte Y and 1,536-byte UV working layout and YUV/RGB + clustering logic. Stage 9 reproduced an *assumed* NV12/BT.601 model only. This does not prove actual NV12/NV21 ordering, matrix/range, plane origin or pixel lease. The audited **exact C5** `libvtcapture` publishes a frame index and immediately requeues the associated buffer before consumer processing; **no safe client-held lease exists in that examined ABI**. Do not copy those live pointers, bypass the exact-hash guard, or start a competing `/dev/video60` queue.
4. **HAL_GAL/compositor/UI.** Vendor binary exports and an LG compositor first-party screenshot path establish a research lead, not trusted video coverage. The public webOS OSE `captureCompositorOutput` documentation identifies privileged `surfacemanager.devutility` access and warns that content from the video area can be omitted. webOS OSE APIs are not guaranteed available to ordinary TV applications. No frame lease, sampling time or permission for our daemon is established.
5. **Protected-content histogram.** The exact audited C5 VT histogram initialization is a non-functional stub. There is no demonstrated usable, permissioned protected-video colour aggregate service. Preserve `Protected -> verified histogram only if genuinely healthy; otherwise Hold/black`, and **never** fall back to a screenshot or frame-pixel path on protected/unknown input.

### Source/research references

- `src/capture/blended.rs`, `src/capture/luna_client.rs`, `src/capture/mod.rs`, `src/capture/drm_routing.rs` in this feature branch.
- `docs/LG_C5_VTCAPTURE_BUFFER_OWNERSHIP.md` (audited exact library hash and native worker sequence).
- `docs/C5-IMAGE-STACK-OFFLINE-ANALYSIS-20261017.md` (LG capture-service roles, binaries, public vs private API distinction).
- `docs/C5-STAGE9-LOW-RES-COLOUR-AND-CAPTURE-DECISION.md` and `docs/C5-STAGE10-FRAME-FRESHNESS-AND-LATENCY.md` (geometry and freshness experiments).
- Linux kernel V4L2 reference: https://cdn.kernel.org/doc/html/latest/userspace-api/media/v4l/vidioc-qbuf.html and https://cdn.kernel.org/doc/html/latest/userspace-api/media/v4l/buffer.html . For a dequeued buffer, timestamp acquisition point and clock are driver-defined; queued buffers are returned to the driver and not implicitly leased to independent consumers.
- webOS OSE compositor API: https://www.webosose.org/docs/reference/ls2-api/com-webos-surfacemanager/ . Its stated UI-video limitation is a warning, not evidence about exact LG C5 implementation behavior.

## Executable evidence gate

`scripts/c5_stage11_capture_feasibility.py` uses only Python 3.9+ stdlib. It keeps eight requirements separate rather than treating a generic screenshot or a process completion clock as a valid frame source: daemon permission, independent Clear gate, video plane, safe ownership, sample timestamp, clock domain, reference RGB, sustainable fresh cadence.

- `--selftest`: validates unknown-vs-failed distinctions, exact C5 hard stop, no automatic authorization, complete *synthetic hypothetical* record still requiring human review, source-contract drift detection and symlink refusal.
- `--report`: prints the reviewed candidate evidence matrix. `observed` means existing **limited and specific** evidence, not verified deployment rights or complete capture feasibility. It cannot contact the TV or approve deployment.
- `--audit-repo PATH`: read-only, bounded, exact-file static check against the current repository source. It detects changes to the assumed BLENDED response/struct field contract, interactive client, C5 exact-fingerprint guard, DRM classifier and Stage 9 unverified format declaration. A match **only** means the previously reviewed code patterns still exist, **not** that an API can provide frames. A drift result fails CI for human review; it never automatically adapts or enables a new path.

Run from this kit:

```sh
python3 -m py_compile scripts/c5_stage11_capture_feasibility.py
python3 scripts/c5_stage11_capture_feasibility.py --selftest
python3 scripts/c5_stage11_capture_feasibility.py --report
```

On an actual **local checkout** of `angelcode1/lg-wled-sync` with the feature branch selected:

```sh
python3 scripts/c5_stage11_capture_feasibility.py --audit-repo .
```

The source audit should output `"status": "STATIC_SOURCE_CONTRACT_MATCH"`, but remember this is an assumption check rather than an end-to-end test. The CI workflow runs both `--selftest` and `--audit-repo .`; no proprietary TV images, binaries or credentials are committed.

## Feasibility decision

| Candidate | Report state | Why not promote |
| --- | --- | --- |
| Opt-in BLENDED one-shot PNG | **INSUFFICIENT_EVIDENCE** | No validated source-sampling timestamp, sample clock, full-video scope, deployed daemon authorization or sustained fresh cadence |
| Stage 7 native-handle JPEG | **INSUFFICIENT_EVIDENCE** | Only two approved bounded samples; JPEG quality not equal, timestamps/steady cadence absent, temporary roles removed |
| Vendor lightingcontroller 64×48 YUV via exact C5 VT | **BLOCKED** | Verified index-publication/immediate-requeue hazard; no consumer-owned data lease; do not dereference |
| HAL_GAL/compositor | **INSUFFICIENT_EVIDENCE** | Could omit video and may be privileged; timestamp, coverage, lease and entitlement unverified |
| Existing protected-content histogram route | **BLOCKED** | Exact vendor histogram initializer stub; no functional authorized provider |

**None is ready for promotion to the production capture path.** This is **not** a claim that no hardware solution is possible: it is the narrower conclusion that current owner-provided C5 evidence does not satisfy the required contracts. The existing default-off BLENDED feature and DRM-first behavior remain as they were.

## Next evidence required — no new capture authorized here

1. **Timestamp contract:** identify an approved service returning an *independent* frame-sampling timestamp tied to a documented monotonic/clock domain, or explicitly mark freshness unknown. Do not synthesize it from `finished`, `stat` mtime or API response time.
2. **Buffer ownership contract:** for any new native path, identify an actual consumer-held dequeue/acquire and release/queue protocol with the applicable exact C5 build and no other queue owner. Pointer size, readable VMAs, a buffer index or frame-size match do not prove this.
3. **Permission/source coverage:** independently verify the daemon's own LS2 client identity, captured plane, and stable authorized DRM-free content; do not impersonate factory roles, alter ACG or broaden `Unknown`/HDMI into Clear.
4. **Performance:** only after a legitimate path and operator approval, a bounded measurement with actual source-frame timestamps, distinct fresh output cadence, p50/p95/p99 source age, CPU costs and clean teardown would be informative. Do not repeat Stage 7 merely to chase average milliseconds.

If the required source timestamp is fundamentally unavailable, it may still be possible to perform a separate owner-approved optical/source-clock calibration for latency, but that cannot be equated with a guaranteed per-frame device timestamp. Such a future experiment requires a new scope and explicit permission.

**Stop rule:** no source-pointer reads, no protected-frame probes, no device/LS2 mutations, no firmware updates and no light output as part of Stage 11.


## Owner Stage 11 verification and closure (2026-10-19)

The operator ran the distributed Stage 11 host-only kit and supplied its
exact machine-readable `--report` output. The self-test marker reported:

```text
C5_STAGE11_CAPTURE_CONTRACT_SELFTEST_PASS explicit_unknown=true known_c5_vt_block=true no_auto_permission=true synthetic_complete_needs_review=true source_drift_detected=true symlink_refused=true no_tv_access=true capture=false lights_output=false
```

The reported `schema` was `c5-stage11-evidence-v1`;
`deploy_authorized=false`, `device_access=false`. Every one of
the five candidate entries had `authorizes_capture_or_deployment=false`.
This is an **operator-supplied offline test result**, not a new C5
runtime observation.

The five reported decisions were:

| Candidate | Decision | Observed specific gates | Unresolved / failed |
| --- | --- | --- | --- |
| `blended_oneshot_png` | `INSUFFICIENT_EVIDENCE` | stable Clear gate, bounded private PNG ownership | 6 unknown, 0 failed |
| `stage7_native_jpeg_handle` | `INSUFFICIENT_EVIDENCE` | known Clear gate, bounded JPEG artifact ownership | 6 unknown, 0 failed |
| `vendor_64x48_yuv` | `BLOCKED` | none | 7 unknown, **unsafe VT buffer ownership failed** |
| `halgal_or_compositor_ui` | `INSUFFICIENT_EVIDENCE` | none | 8 unknown, 0 failed |
| `protected_content_histogram` | `BLOCKED` | none | 6 unknown, 2 failed (no clear capture gate, unavailable provider) |

The repository CI workflow for commit
[`997443d89a4ed88c1c0365375450aab34dfae6d6`](https://github.com/angelcode1/lg-wled-sync/commit/997443d89a4ed88c1c0365375450aab34dfae6d6)
also completed with **success**:
[GitHub Actions run 39188559965](https://github.com/angelcode1/lg-wled-sync/actions/runs/39188559965).
This verifies the host-side evidence contract and repository checks;
it does not prove capture authorization, successful runtime use, or
hardware throughput.

### Next substantive evidence, without another synthetic phase

1. **First determine the source-time contract of LG screenshot output.**
   The current `executeOneShot` reply and Rust `BlendedFrame` store
   request completion only. Search previously owner-supplied, lawfully
   obtained API/ELF metadata for any *documented actual source-sampling*
   timestamp and clock origin. Do not interpret `finished`, file mtime
   or request completion as source time. If no independent source clock
   exists, report exact frame age as unmeasurable on this path.
2. **Separate private PNG ownership from native V4L2 ring ownership.**
   The captured private file is bounded and locally owned; the exact
   audited C5 VT pointer snapshot still is **not** safely leased. An
   independent acquire/release/fence ABI and exact-build evidence would
   be required before revisiting native frame reads.
3. **Verify permissions and source coverage separately.** A root-shell
   response or temporary Stage 7 role cannot establish authorisation
   for the deployed Rust service identity. Compositor/UI screenshots
   cannot be assumed to include all video planes.
4. **Only after owner approval and a verified authorised clear source,**
   define an explicitly bounded real-device timing test with true frame
   age/provenance (or clearly labelled optical proxy), fresh cadence,
   CPU budget and teardown. No such capture is authorised by this
   report. Preserve `Unknown`/`Protected` Hold/black and the exact
   C5 VT guard.

**Stage 11 disposition:** offline verification **PASS**; all
capture-path promotion decisions remain **unchanged**. No further
simulated throughput stage can substitute for missing device/API
evidence. Production behaviour and the restored Stage 1 installation
remain untouched.
