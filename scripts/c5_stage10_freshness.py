#!/usr/bin/env python3
"""Offline-only C5 Stage 10 synthetic frame-freshness and fail-closed simulator.

No TV/network/native capture operations. All timestamps are *synthetic* monotonic
milliseconds; a source timestamp is mandatory to establish frame age.
Python 3.9+, standard library only.
"""
import argparse
from collections import Counter
from dataclasses import dataclass
import json
import math
from pathlib import Path
import re
import stat

SCHEMA = "c5-stage10-synthetic-v1"
MAX_TRACE_BYTES = 65536
MAX_EVENTS = 512
MAX_MS = 10_000_000
EVENT_KEYS = {
    "classification": {"t_ms", "type", "class"},
    "start": {"t_ms", "type", "id"},
    "complete": {"t_ms", "type", "id", "source_timestamp_ms",
                 "decode_ms", "processing_ms", "transport_ms"},
    "tick": {"t_ms", "type"},
}
PRIORITY = {"classification": 0, "complete": 1, "start": 2, "tick": 3}


class BadTrace(ValueError):
    pass


def bounded_int(value, name, limit=MAX_MS):
    if type(value) is not int or not 0 <= value <= limit:
        raise BadTrace("invalid bounded integer: " + name)
    return value


def percentile_nearest_rank(numbers, p):
    if not numbers:
        return None
    sorted_data = sorted(numbers)
    return sorted_data[max(0, math.ceil(len(sorted_data) * p / 100) - 1)]


def percentiles(numbers):
    return {"count": len(numbers), "p50_ms": percentile_nearest_rank(numbers, 50),
            "p95_ms": percentile_nearest_rank(numbers, 95),
            "p99_ms": percentile_nearest_rank(numbers, 99)}


def validate_trace(obj):
    if type(obj) is not dict or set(obj) != {"schema", "synthetic", "events"}:
        raise BadTrace("trace top-level fields are invalid")
    if obj["schema"] != SCHEMA or obj["synthetic"] is not True:
        raise BadTrace("only explicitly synthetic trace schema is allowed")
    events = obj["events"]
    if type(events) is not list or not 1 <= len(events) <= MAX_EVENTS:
        raise BadTrace("event count outside bound")
    last_time = -1
    gate_times = set()
    seen_starts = set()
    for event in events:
        if type(event) is not dict:
            raise BadTrace("invalid event object")
        kind = event.get("type")
        if kind not in EVENT_KEYS or set(event) != EVENT_KEYS[kind]:
            raise BadTrace("event fields/type mismatch")
        t = bounded_int(event["t_ms"], "t_ms")
        if t < last_time:
            raise BadTrace("events must be timestamp-ordered")
        last_time = t
        if kind == "classification":
            if event["class"] not in ("Clear", "Protected", "Unknown"):
                raise BadTrace("invalid classification")
            if t in gate_times:
                raise BadTrace("multiple classifications at the same timestamp")
            gate_times.add(t)
        if kind in ("start", "complete"):
            if type(event["id"]) is not str or not re.fullmatch(r"[A-Za-z0-9_-]{1,24}", event["id"]):
                raise BadTrace("invalid synthetic request id")
        if kind == "start":
            if event["id"] in seen_starts:
                raise BadTrace("synthetic request identifiers must be unique")
            seen_starts.add(event["id"])
        if kind == "complete":
            source_ts = event["source_timestamp_ms"]
            if source_ts is not None:
                bounded_int(source_ts, "source_timestamp_ms")
            for key in ("decode_ms", "processing_ms", "transport_ms"):
                bounded_int(event[key], key, 10_000)
    return events


@dataclass
class Pending:
    ready_at: int
    source_at: int
    generation: int


class Simulator:
    """Single in-flight request and at most one pending output, no queue."""

    def __init__(self, *, clear_stability_ms=2000, classification_ttl_ms=300,
                 max_age_ms=125):
        self.stability = bounded_int(clear_stability_ms, "clear_stability_ms")
        self.ttl = bounded_int(classification_ttl_ms, "classification_ttl_ms")
        self.max_age = bounded_int(max_age_ms, "max_age_ms")
        if not all((self.stability, self.ttl, self.max_age)):
            raise BadTrace("policy thresholds must be positive")
        self.klass = "Unknown"
        self.last_gate = None
        self.clear_since = None
        self.inflight = None
        self.pending = None
        self.generation = 0
        self.counts = Counter()
        self.request_ms = []
        self.frame_age_ms = []

    def authorized(self, t):
        return (self.klass == "Clear" and self.clear_since is not None
                and self.last_gate is not None and t - self.last_gate <= self.ttl
                and t - self.clear_since >= self.stability)

    def _emit_if_due(self, t, *, inclusive):
        pending = self.pending
        if pending is None or (pending.ready_at > t if inclusive else pending.ready_at >= t):
            return
        self.pending = None
        age = pending.ready_at - pending.source_at
        if not self.authorized(pending.ready_at) or pending.generation != self.generation:
            self.counts["blocked_emissions"] += 1
        elif age > self.max_age:
            self.counts["stale_frames_dropped"] += 1
        else:
            self.counts["emitted_fresh_frames"] += 1
            self.frame_age_ms.append(age)

    def _gate(self, e):
        t, cls = e["t_ms"], e["class"]
        previous_valid = (self.klass == "Clear" and self.last_gate is not None
                          and t - self.last_gate <= self.ttl)
        self.last_gate = t
        self.klass = cls
        if cls != "Clear":
            self.clear_since = None
            self.generation += 1
            if self.inflight is not None:
                self.counts["revoked_inflight"] += 1
                self.inflight = None
            if self.pending is not None:
                self.counts["revoked_pending"] += 1
                self.pending = None
        elif not previous_valid:
            self.clear_since = t

    def _start(self, e):
        if self.inflight is not None:
            self.counts["overlapping_requests_refused"] += 1
        elif not self.authorized(e["t_ms"]):
            self.counts["unauthorized_starts_refused"] += 1
        else:
            self.inflight = (e["id"], e["t_ms"], self.generation)
            self.counts["requests_started"] += 1

    def _complete(self, e):
        t = e["t_ms"]
        if self.inflight is None or self.inflight[0] != e["id"]:
            self.counts["orphan_results_refused"] += 1
            return
        request_id, started, gen = self.inflight
        self.inflight = None
        self.request_ms.append(t - started)
        self.counts["requests_completed"] += 1
        if gen != self.generation or not self.authorized(t):
            self.counts["blocked_results"] += 1
            return
        source_at = e["source_timestamp_ms"]
        if source_at is None:
            self.counts["missing_source_timestamps"] += 1
            return
        if source_at > t:
            self.counts["future_source_timestamps_refused"] += 1
            return
        ready_at = t + e["decode_ms"] + e["processing_ms"] + e["transport_ms"]
        if ready_at > MAX_MS:
            self.counts["invalid_downstream_times"] += 1
            return
        # Bound memory: never accumulate output frames in an unbounded queue.
        if self.pending is not None:
            self.counts["superseded_pending_frames"] += 1
        self.pending = Pending(ready_at, source_at, gen)

    def run(self, obj):
        events = validate_trace(obj)
        group = []
        at = None
        def flush():
            if not group:
                return
            now = group[0]["t_ms"]
            self._emit_if_due(now, inclusive=False)
            for e in sorted(group, key=lambda v: PRIORITY[v["type"]]):
                kind = e["type"]
                if kind == "classification":
                    self._gate(e)
                elif kind == "start":
                    self._start(e)
                elif kind == "complete":
                    self._complete(e)
            # Same-timestamp revocations take precedence over output emission.
            self._emit_if_due(now, inclusive=True)
        for e in events:
            if at is not None and e["t_ms"] != at:
                flush()
                group = []
            group.append(e)
            at = e["t_ms"]
        flush()
        return {
            "schema": SCHEMA,
            "provenance": "SYNTHETIC_ONLY_NOT_C5_PERFORMANCE",
            "capture_performed": False,
            "pixels_written": False,
            "policy": {"clear_stability_ms": self.stability,
                       "classification_ttl_ms": self.ttl,
                       "max_source_age_at_output_ms": self.max_age,
                       "single_inflight": True, "max_pending_outputs": 1},
            "counts": dict(sorted(self.counts.items())),
            "request_duration_synthetic": percentiles(self.request_ms),
            "source_age_at_output_synthetic": percentiles(self.frame_age_ms),
            "pending_output_at_trace_end": self.pending is not None,
            "inflight_at_trace_end": self.inflight is not None,
            "note": "Synthetic event replay, not sustained FPS; real source timestamp requires trustworthy provenance."
        }


def make_trace(events):
    return {"schema": SCHEMA, "synthetic": True, "events": events}


def event(t, kind, **props):
    return {"t_ms": t, "type": kind, **props}


def clear_gates(last=2000):
    return [event(t, "classification", **{"class": "Clear"})
            for t in range(0, last + 1, 200)]


def completion(t, ident, source_at, decode=8, processing=7, transport=5):
    return event(t, "complete", id=ident, source_timestamp_ms=source_at,
                 decode_ms=decode, processing_ms=processing, transport_ms=transport)


def selftest():
    # 160ms simulated RPC does not mean source age is 160ms or display FPS ~6.25.
    good = make_trace(clear_gates(2000) + [event(2000, "start", id="good"),
                    completion(2160, "good", 2125), event(2200, "classification", **{"class": "Clear"})])
    valid = Simulator().run(good)
    assert valid["counts"]["emitted_fresh_frames"] == 1
    assert valid["request_duration_synthetic"]["p50_ms"] == 160
    assert valid["source_age_at_output_synthetic"]["p50_ms"] == 55

    stale = make_trace(clear_gates(2000) + [event(2000, "start", id="old"),
                    completion(2160, "old", 1950), event(2200, "classification", **{"class": "Clear"})])
    assert Simulator().run(stale)["counts"]["stale_frames_dropped"] == 1

    no_stamp = make_trace(clear_gates(2000) + [event(2000, "start", id="missing"),
                        completion(2160, "missing", None), event(2200, "tick")])
    assert Simulator().run(no_stamp)["counts"]["missing_source_timestamps"] == 1
    assert Simulator().run(no_stamp)["source_age_at_output_synthetic"]["count"] == 0

    protected = make_trace(clear_gates(2000) + [event(2000, "start", id="x"),
                          completion(2160, "x", 2150, decode=60, processing=0, transport=0),
                          event(2200, "classification", **{"class": "Protected"}),
                          event(2220, "tick"), event(2220, "start", id="forbidden")])
    p = Simulator().run(protected)
    assert p["counts"]["revoked_pending"] == 1
    assert p["counts"]["unauthorized_starts_refused"] == 1
    assert p["source_age_at_output_synthetic"]["count"] == 0

    revoked_inflight = make_trace(clear_gates(2000) + [event(2000, "start", id="inflight"),
                       event(2120, "classification", **{"class": "Unknown"}),
                       completion(2160, "inflight", 2130), event(2200, "tick")])
    r = Simulator().run(revoked_inflight)
    assert r["counts"]["revoked_inflight"] == 1
    assert r["counts"]["orphan_results_refused"] == 1

    overlap = make_trace(clear_gates(2000) + [event(2000, "start", id="a"),
                        event(2000, "start", id="b"), completion(2160, "a", 2150),
                        event(2200, "classification", **{"class": "Clear"})])
    assert Simulator().run(overlap)["counts"]["overlapping_requests_refused"] == 1

    future = make_trace(clear_gates(2000) + [event(2000, "start", id="a"),
                       completion(2160, "a", 2161), event(2200, "tick")])
    assert Simulator().run(future)["counts"]["future_source_timestamps_refused"] == 1

    # Gap in classification freshness must restart two-second Clear stability.
    gap = make_trace(clear_gates(2000) + [event(2400, "classification", **{"class": "Clear"}),
                        event(2400, "start", id="gap")])
    assert Simulator().run(gap)["counts"]["unauthorized_starts_refused"] == 1

    # Classification revocation at exact emission time wins over queued output.
    equal = make_trace(clear_gates(2000) + [event(2000, "start", id="same"),
                       completion(2160, "same", 2150, decode=40, processing=0, transport=0),
                       event(2200, "classification", **{"class": "Unknown"})])
    assert Simulator().run(equal)["counts"]["revoked_pending"] == 1

    # Multiple accepted starts while only one pending output is maintained.
    replaced = make_trace(clear_gates(2200) + [event(2000, "start", id="r1"),
                       completion(2010, "r1", 2005, decode=250, processing=0, transport=0),
                       event(2020, "start", id="r2"),
                       completion(2050, "r2", 2040, decode=20, processing=0, transport=0),
                       event(2200, "tick")])
    # Pre-existing clear gates interleaved to preserve chronological order.
    replaced["events"] = sorted(replaced["events"], key=lambda x: x["t_ms"])
    # The first pending would have been replaced; second emission due at 2070.
    rep = Simulator().run(replaced)
    assert rep["counts"]["superseded_pending_frames"] == 1, rep
    assert rep["counts"]["emitted_fresh_frames"] == 1, rep

    for bad in [make_trace([event(0, "classification", **{"class": "Invalid"})]),
                make_trace([event(0, "tick", hidden="x")]),
                {"schema": SCHEMA, "synthetic": False, "events": [event(0, "tick")]},
                make_trace([event(3, "tick"), event(2, "tick")]),
                make_trace([event(0, "start", id="a", suspicious="x")]),
                make_trace([event(0, "start", id="a"), event(1, "start", id="a")]),
                make_trace([event(0, "classification", **{"class": "Clear"}),
                            event(0, "classification", **{"class": "Protected"})])]:
        try:
            Simulator().run(bad)
        except BadTrace:
            pass
        else:
            raise AssertionError("invalid synthetic trace accepted")
    assert percentile_nearest_rank([1, 2, 3, 4, 5, 6], 95) == 6
    print("C5_STAGE10_FRESHNESS_SELFTEST_PASS "
          "source_age_separate_from_request=true missing_timestamps_refused=true "
          "stale_dropped=true overlap_refused=true pending_max_1=true "
          "clear_stability_and_ttl=true protected_unknown_fail_closed=true "
          "same_timestamp_revocation_wins=true malformed_refused=true "
          "capture=false tv_access=false lights_output=false")


def load_trace(path):
    meta = path.lstat()
    if not stat.S_ISREG(meta.st_mode) or not 1 <= meta.st_size <= MAX_TRACE_BYTES:
        raise BadTrace("requires a bounded regular JSON file (no symlinks)")
    with path.open("rb") as fh:
        data = fh.read(MAX_TRACE_BYTES + 1)
    if len(data) != meta.st_size:
        raise BadTrace("file changed while reading")
    try:
        return json.loads(data, parse_constant=lambda x: (_ for _ in ()).throw(BadTrace("invalid JSON number")))
    except (ValueError, UnicodeDecodeError) as e:
        raise BadTrace("invalid JSON") from e


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    opts = parser.add_mutually_exclusive_group(required=True)
    opts.add_argument("--selftest", action="store_true", help="Synthetic internal assertions")
    opts.add_argument("--trace", type=Path, help="Existing local JSON with synthetic=true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return 0
    try:
        report = Simulator().run(load_trace(args.trace))
    except (BadTrace, OSError) as exc:
        parser.exit(2, "C5_STAGE10_TRACE_REFUSED: " + str(exc) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
