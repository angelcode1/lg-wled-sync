# LG C5 Stage 10 — offline frame freshness, scheduling and latency decisions

**Status:** offline-only mathematical/event simulation and assertions. **No TV access, capture calls, buffers, Luna roles, LED transport, production daemon change or installed configuration modification.** The model is not a benchmark of any LG C5 hardware, webOS capture service, Hue Bridge or WLED device. The original owner-verified Stage 1 baseline is untouched.

## Why Stage 10 is necessary

The owner-verified Stage 9 offline 64×48 YUV420 test passed its SHA-256 manifest check and all synthetic geometry, four-edge colour, orientation, chroma-swap, malformed-buffer and black-frame checks. That establishes the independent test model's behaviour **only**. The vendor's exact YUV layout, colour matrix, video-plane authorization and buffer lease have not been verified.

Stage 7 two-frame JPEG experiments each took approximately 151–161 ms per *request* on the tested C5. These numbers do not give the actual content-frame acquisition time, the source image age, the end-to-end age when Hue/WLED receives a colour, or sustained frame rate. Further, Stage 7 JPEG timing cannot be substituted for the existing Rust BLENDED **PNG** backend timing. A nominal 64×48 processing resolution cannot make a 160 ms capture RPC take 33 ms or repair the known unsafe immediate-requeue C5 VT pointer API.

## Frame freshness contract

A future actual device benchmark must distinguish these monotonically timestamped stages, all from one established clock domain or with a properly bounded clock conversion:

1. `request_start`: application begins the capture request.
2. `source_timestamp`: **independently trustworthy** time when the source content was sampled. It is not the RPC start time, result time, JPEG/PNG timestamp guessed from a filename or the time the file was opened.
3. `capture_complete`: service result received, content provenance and geometry validated.
4. `decoded` / `processed`: colour field computed once, with per-vendor mapping after the shared processing stage.
5. `transport_complete`: colour published/output completed; for Hue/WLED physical display time and transport jitter require a separately verified acknowledgement or optical measurement.

Define:

- **RPC duration:** `capture_complete − request_start`.
- **Nominal downstream time:** `decode + processing + transport` (for this synthetic model only).
- **Source age at output:** `transport_complete − source_timestamp`, measurable only when source timestamp provenance and clock domain are valid.
- **Cadence / sustainable FPS:** number of distinct, fresh successfully published source frames per elapsed wall-clock second across an adequately long series (not inverse of a single fast request). Reused colours, queued stale buffers and duplicate frame content do not count as independent fresh captures.
- **Percentiles:** count and nearest-rank p50/p95/p99, with explicit sample count. A two-frame Stage 7 dataset does **not** support meaningful sustained p95/p99 throughput estimates.

### Synthetic numerical example (not measured on the TV)

An *assumed* request starts at `2000 ms` and completes at `2160 ms`. If an independently timestamped source frame were sampled at `2125 ms`, and downstream costs total `20 ms`, output occurs at `2180 ms`:

- Synthetic request duration: **160 ms**.
- Synthetic source age at output: **55 ms**, not 160 ms.
- If the source were instead sampled at `1950 ms`, age would be **230 ms** at output, exceeding this model's illustrative 125 ms freshness budget.
- If there is no trustworthy source timestamp, the age is **unknown**; the model refuses to label it fresh, even if the request completed successfully.

These examples show why short or long RPC duration cannot alone prove freshness.

## Executable host-only simulator

`scripts/c5_stage10_freshness.py` uses Python 3.9+ standard library only. Its built-in tests execute entirely in memory:

```sh
python3 -m py_compile scripts/c5_stage10_freshness.py
python3 scripts/c5_stage10_freshness.py --selftest
```

Expected prefix: `C5_STAGE10_FRESHNESS_SELFTEST_PASS`.

The model's **illustrative default policy**, not production device tuning, is:

| Policy input | Synthetic default | Purpose |
| --- | ---: | --- |
| Consecutive stable Clear duration | 2,000 ms | Match conservative clear gate behaviour |
| Maximum classification age | 300 ms | No capture/output based on stale media classification |
| Maximum source age at output | 125 ms | Discard stale colour results; example only |
| Concurrent capture requests | 1 | Prevent overlapping lease or RPC assumptions |
| Pending colour output slots | 1 | Replace older not-yet-output colours; no growing queue |

It asserts: stable and fresh `Clear` classification before initiating a request; `Protected`/`Unknown` revoke outstanding captures/output; same-timestamp revocations win; no captured frame without trustworthy synthetic source timestamp is counted as fresh; future timestamps, overlapping requests and orphan results are refused; stale outputs are dropped; gaps in classification invalidate the accumulated clear-stability timer. Counters and nearest-rank percentiles are reported separately for synthetic request duration and synthetic source age.

No simulated protection decision authorizes real capture, and no simulated frame touches WLED, Hue, the TV, or an on-device buffer. The script does not import or invoke vendor libraries.

### Optional existing local synthetic trace

For independent deterministic replay, pass an existing **regular** (not symlink) local JSON file of at most 65,536 bytes containing **1–512 events**, explicitly marked `"synthetic": true` and schema `c5-stage10-synthetic-v1`. Invalid types, fields, timestamps and out-of-order records are refused; only counts and aggregate synthetic timing results are printed. Events support `classification`, `start`, `complete` and `tick`. For example:

```json
{
  "schema": "c5-stage10-synthetic-v1",
  "synthetic": true,
  "events": [
    {"t_ms": 0, "type": "classification", "class": "Clear"},
    {"t_ms": 200, "type": "classification", "class": "Clear"},
    {"t_ms": 400, "type": "classification", "class": "Clear"},
    {"t_ms": 600, "type": "classification", "class": "Clear"},
    {"t_ms": 800, "type": "classification", "class": "Clear"},
    {"t_ms": 1000, "type": "classification", "class": "Clear"},
    {"t_ms": 1200, "type": "classification", "class": "Clear"},
    {"t_ms": 1400, "type": "classification", "class": "Clear"},
    {"t_ms": 1600, "type": "classification", "class": "Clear"},
    {"t_ms": 1800, "type": "classification", "class": "Clear"},
    {"t_ms": 2000, "type": "classification", "class": "Clear"},
    {"t_ms": 2000, "type": "start", "id": "synthetic-a"},
    {"t_ms": 2160, "type": "complete", "id": "synthetic-a", "source_timestamp_ms": 2125, "decode_ms": 8, "processing_ms": 7, "transport_ms": 5},
    {"t_ms": 2200, "type": "tick"}
  ]
}
```

Run: `python3 scripts/c5_stage10_freshness.py --trace synthetic-trace.json`. The simulator uses the explicit trace's timestamps; it neither creates timing data from C5 logs nor infers any real device capability. If a benchmark lacks source sampling provenance, report **INCONCLUSIVE**, not an assumed capture timestamp.

## Capture-path promotion decisions

| Candidate | Permission/coverage gate | Ownership/freshness gate | Decision |
| --- | --- | --- | --- |
| LG `executeOneShot` clear PNG | Existing opt-in code, only verified Clear state; no production timing series | Source sampling timestamp and frame age absent from current evidence | Preserve, **do not claim 30 fps** |
| Stage 7 native handle JPEG | Temporary own-name client, explicit bounded owner-approved test; original Stage 1 restored | Two 151/161 ms native requests, no sustained response/freshness data | Research-only, no automatic reuse |
| 64×48 LG lightingcontroller YUV route | Privileged vendor code imports C5 VT; not our executable identity | Same exact C5 `libvtcapture` immediate requeue; no safe lease | **No live pointer reads** |
| Compositor/HAL screenshot | API appears privileged and can be UI-only/missing video | No proven whole-video coverage, safe rights or source age | Static analysis only |
| Histogram/protected aggregate | No functional permitted C5 histogram provider established | Existing exact vendor histogram initializer is stubbed | **Hold/black**, no bypass |

## Future benchmark gates (not authorized by this stage)

1. A legitimate per-service executable identity with only the permissions needed, independently confirmed `Clear` media source, and reliable source-plane provenance; no HDMI/unknown/protected shortcuts.
2. Independent, bounded source-acquisition timestamp (or output must report source age as **unknown**), geometry, format, colour matrix, encoder setting and a safe data-buffer lease.
3. A long enough measured sequential capture series to compute request p50/p95/p99, distinct fresh frame cadence, source age at output, CPU utilisation, memory and service teardown safety. Measure PNG and JPEG independently.
4. Stale/unknown/protected transitions that revoke capture and pending output without unbounded queue build-up; no new V4L2 queue owner or unsafe native pointer use.
5. Independent bright-edge colour reference (Stage 8), orientation and valid-black tests (Stages 8/9), separate per-Hue-channel/WLED spatial mapping, and user approval before any device deployment.

**Stage 10 does not promote any experimental path into production.** No TV call or upload is made by the code; repository CI runs only offline self-tests.
