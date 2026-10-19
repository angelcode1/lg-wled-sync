#!/usr/bin/env python3
"""LG C5 Stage 11: host-only evidence gate for possible capture sources.

No network, TV, capture service, device, native pointer, DRM operation,
permissions mutation or light output. Python >=3.9, standard library only.
The candidate matrix records *evidence as reviewed*, not an API capability.
"""
import argparse
import json
import re
import stat
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Optional

DIMENSIONS = (
    "daemon_identity_permission",
    "verified_clear_source_gate",
    "video_plane_coverage",
    "safe_data_ownership",
    "source_sampling_timestamp",
    "timestamp_clock_domain",
    "reference_color_accuracy",
    "sustainable_fresh_cadence",
)
UNKNOWN = "unknown"
OBSERVED = "observed"
FAILED = "failed"
VALID = frozenset((UNKNOWN, OBSERVED, FAILED))


@dataclass(frozen=True)
class Candidate:
    name: str
    evidence: Dict[str, str]
    reasons: Dict[str, str]
    hard_stop: Optional[str] = None

    def __post_init__(self):
        if set(self.evidence) != set(DIMENSIONS):
            raise ValueError("candidate dimensions must be exact")
        if any(value not in VALID for value in self.evidence.values()):
            raise ValueError("invalid evidence value")
        if set(self.reasons) != set(DIMENSIONS):
            raise ValueError("missing evidence rationale")
        if any(not reason.strip() for reason in self.reasons.values()):
            raise ValueError("empty evidence rationale")


def candidate(name, values, descriptions, hard_stop=None):
    return Candidate(name, dict(zip(DIMENSIONS, values)),
                     dict(zip(DIMENSIONS, descriptions)), hard_stop)


# These are audit findings from the existing repo and operator logs, not
# claimed current TV API entitlements or realtime measurements. The default
# is UNKNOWN unless evidence positively establishes the exact property.
PROFILES = (
    candidate("blended_oneshot_png", [
        UNKNOWN, OBSERVED, UNKNOWN, OBSERVED, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN], [
        "root-shell and temporary Stage 7 LS2 roles do not prove the daemon's deployed identity",
        "existing Rust route requires stable Clear before invoking its snapshot backend",
        "screenshots of moving imagery do not establish guaranteed video-plane coverage",
        "bounded owned private PNG file is validated and decoded; not a borrowed V4L2 pointer",
        "capture reply has dimensions and byte count, but no verified sample-time field",
        "request completion Instant is not a frame-source sampling clock",
        "no independently verified bright-edge reference for real C5 output",
        "no sustained fresh-frame PNG cadence and source-age series"
    ]),
    candidate("stage7_native_jpeg_handle", [
        UNKNOWN, OBSERVED, UNKNOWN, OBSERVED, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN], [
        "the approved temporary client grant was restored; no standing daemon entitlement",
        "two explicit owner-approved captures required independently known Clear classifications",
        "two images do not independently attest all compositor/video source coverage",
        "bounded JPEG artifact decoded and handle destroyed; no VT pointers exposed",
        "JPEG request completion did not disclose independent source-sample time",
        "host request timing is not a verified shared sampling clock",
        "native/oneshot JPEG quantization differs; no source reference measurement",
        "only two requests at ~151-161ms; no streaming/cadence measurement"
    ]),
    candidate("vendor_64x48_yuv", [
        UNKNOWN, UNKNOWN, UNKNOWN, FAILED, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN], [
        "built-in lightingcontroller privileges cannot be borrowed by our daemon",
        "vendor static function inspection is not a per-frame Clear gate",
        "64x48 byte lengths do not establish which on-screen plane was sampled",
        "audited VT library publishes pointer index then immediately requeues; no consumer lease",
        "buffer-size and conversion code expose no proven source timestamp",
        "no verified V4L2 sampling clock associated with a consumer-held frame",
        "NV12/BT.601 Stage 9 synthetic assumptions are not vendor layout proof",
        "offline byte-size advantage cannot establish safe measured throughput"
    ], "exact C5 VT pointer path lacks an independently held buffer lease"),
    candidate("halgal_or_compositor_ui", [
        UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN], [
        "OSE compositor method is privileged; TV daemon authorization not confirmed",
        "no independent clear-source gate has been established for this route",
        "official compositor docs warn that video area may be omitted",
        "only binary exports and screenshot API references, not a verified consumer lease",
        "no source-sampling timestamp observed in this interface",
        "no verified timestamp provenance and clock conversion",
        "no four-edge bright video reference from this route",
        "no sustained capture/provenance/CPU measurements"
    ]),
    candidate("protected_content_histogram", [
        UNKNOWN, FAILED, UNKNOWN, FAILED, UNKNOWN, UNKNOWN, UNKNOWN, UNKNOWN], [
        "no authorised functioning histogram API is proven for our daemon",
        "protected media is outside the Clear-only frame-capture authorization",
        "no validated protected content aggregation method on this exact C5 build",
        "audited VtHistogram::initHistogram is a stub; no usable provider",
        "there are no verified sampling timestamps for histogram frames",
        "no validated timestamp clock domain for histogram source",
        "no real working histogram output to compare against calibrated edge references",
        "no working provider, so sustainable sample cadence unavailable"
    ], "known exact-build histogram stub: Protected must Hold/black"),
)


def assess(item: Candidate):
    """Never authorizes deployment or capture: best outcome is manual review."""
    failed = [key for key in DIMENSIONS if item.evidence[key] == FAILED]
    unknown = [key for key in DIMENSIONS if item.evidence[key] == UNKNOWN]
    if item.hard_stop or failed:
        decision = "BLOCKED"
    elif unknown:
        decision = "INSUFFICIENT_EVIDENCE"
    else:
        decision = "ELIGIBLE_FOR_HUMAN_REVIEW_ONLY"
    return {
        "candidate": item.name,
        "decision": decision,
        "hard_stop": item.hard_stop,
        "failed": failed,
        "unknown": unknown,
        "observed": [key for key in DIMENSIONS if item.evidence[key] == OBSERVED],
        "evidence": {key: {"state": item.evidence[key], "basis": item.reasons[key]}
                     for key in DIMENSIONS},
        "authorizes_capture_or_deployment": False,
    }


def bounded_text(root: Path, relative: str) -> str:
    """Only exact named regular files in a locally supplied checkout."""
    path = root
    for segment in Path(relative).parts:
        path = path / segment
        if path.is_symlink():
            raise ValueError("refuse symlink in source path: " + relative)
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 200000:
        raise ValueError("unexpected or oversized source file: " + relative)
    return path.read_text(encoding="utf-8")


def audit_repo(root: Path):
    """Detect changes to assumptions; string scanning is NOT runtime proof."""
    if not root.is_dir() or root.is_symlink():
        raise ValueError("expected non-symlink repository directory")
    blended = bounded_text(root, "src/capture/blended.rs")
    luna = bounded_text(root, "src/capture/luna_client.rs")
    routing = bounded_text(root, "src/capture/drm_routing.rs")
    capture_mod = bounded_text(root, "src/capture/mod.rs")
    stage9 = bounded_text(root, "docs/C5-STAGE9-LOW-RES-COLOUR-AND-CAPTURE-DECISION.md")
    match = re.search(r"\bpub\s+struct\s+BlendedFrame\s*\{([^}]*)\}", blended, re.S)
    fields = dict(re.findall(r"\bpub\s+(\w+)\s*:\s*([\w<>]+)", match.group(1))) if match else {}
    required = {
        "finished": "Instant", "request_ms": "u128", "decode_ms": "u128",
        "pixels": "Vec<u8>", "width": "u32", "height": "u32"
    }
    checks = {
        "blended_completion_fields_known": all(fields.get(k) == v for k, v in required.items()),
        "no_claimed_frame_sampling_timestamp": not any(
            any(word in k.lower() for word in ("source_timestamp", "sample_timestamp", "exposure_timestamp", "capture_timestamp"))
            for k in fields
        ),
        "reply_contains_dimensions_and_byte_count": all(
            f'reply["{k}"]' in blended for k in ("capturedWidth", "capturedHeight", "writtenBytes")
        ),
        "uses_oneshot_png": all(term in blended for term in
                                   ('"method": "BLENDED"', '"format": "png"', 'executeOneShot')),
        "luna_is_separate_interactive_subprocess": all(term in luna for term in
                    ('Command::new("luna-send")', '"-i"', '"-f"', 'kill_on_drop(true)')),
        "known_c5_vt_fingerprint_guard_present": "histogram::is_known_c5_vendor_elf()" in capture_mod,
        "drm_router_requires_stable_clear": all(term in routing for term in
                    ('PlaybackClass::Protected', 'PlaybackClass::Unknown', 'Duration::from_secs(2)')),
        "stage9_disclaims_vendor_layout_confirmation": (
            "do not prove NV12 vs NV21" in stage9),
    }
    return {
        "status": "STATIC_SOURCE_CONTRACT_MATCH" if all(checks.values()) else "SOURCE_CHANGED_MANUAL_REVIEW_REQUIRED",
        "checks": checks,
        "proof_limits": "Static presence/absence of these code patterns is not runtime entitlement, source timestamp, safe lease, or colour proof",
        "tv_access": False,
        "capture": False,
    }


def selftest():
    summaries = {c.name: assess(c) for c in PROFILES}
    assert summaries["blended_oneshot_png"]["decision"] == "INSUFFICIENT_EVIDENCE"
    assert "source_sampling_timestamp" in summaries["blended_oneshot_png"]["unknown"]
    assert summaries["vendor_64x48_yuv"]["decision"] == "BLOCKED"
    assert "safe_data_ownership" in summaries["vendor_64x48_yuv"]["failed"]
    assert summaries["protected_content_histogram"]["decision"] == "BLOCKED"
    assert summaries["halgal_or_compositor_ui"]["decision"] == "INSUFFICIENT_EVIDENCE"
    assert all(not result["authorizes_capture_or_deployment"] for result in summaries.values())

    first = PROFILES[0]
    try:
        Candidate("bad", {"foo": OBSERVED}, {"foo": "made up"})
    except ValueError:
        pass
    else:
        raise AssertionError("unexpected dimension allowed")

    # Even a hypothetical complete future record is a review candidate,
    # never authority for device writes, DRM routing or capture.
    hypothetical = Candidate("hypothetical", {k: OBSERVED for k in DIMENSIONS},
                             {k: "independently verified hypothetical test" for k in DIMENSIONS})
    complete = assess(hypothetical)
    assert complete["decision"] == "ELIGIBLE_FOR_HUMAN_REVIEW_ONLY"
    assert not complete["authorizes_capture_or_deployment"]
    veto = Candidate("veto", hypothetical.evidence, hypothetical.reasons,
                     "unsafe ownership despite apparent fields")
    assert assess(veto)["decision"] == "BLOCKED"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for sub in ("src/capture", "docs"):
            (root / sub).mkdir(parents=True, exist_ok=True)
        code = ('pub struct BlendedFrame {\n'
                ' pub pixels: Vec<u8>, pub width: u32, pub height: u32,\n'
                ' pub finished: Instant, pub request_ms: u128, pub decode_ms: u128\n}\n'
                'reply["capturedWidth"] reply["capturedHeight"] reply["writtenBytes"]\n'
                '"method": "BLENDED" "format": "png" executeOneShot\n')
        (root / "src/capture/blended.rs").write_text(code)
        (root / "src/capture/luna_client.rs").write_text(
            'Command::new("luna-send") "-i" "-f" kill_on_drop(true)')
        (root / "src/capture/mod.rs").write_text("histogram::is_known_c5_vendor_elf()")
        (root / "src/capture/drm_routing.rs").write_text(
            'PlaybackClass::Protected PlaybackClass::Unknown Duration::from_secs(2)')
        (root / "docs/C5-STAGE9-LOW-RES-COLOUR-AND-CAPTURE-DECISION.md").write_text(
            'do not prove NV12 vs NV21')
        result = audit_repo(root)
        assert result["status"] == "STATIC_SOURCE_CONTRACT_MATCH", result
        assert all(result["checks"].values())
        (root / "src/capture/blended.rs").write_text(code.replace(
            'pub finished: Instant', 'pub source_timestamp: Instant, pub finished: Instant'))
        assert audit_repo(root)["status"] == "SOURCE_CHANGED_MANUAL_REVIEW_REQUIRED"
        (root / "src/capture/blended.rs").unlink()
        (root / "src/capture/blended.rs").symlink_to(root / "src/capture/mod.rs")
        try:
            audit_repo(root)
        except ValueError:
            pass
        else:
            raise AssertionError("symlink accepted")

    print("C5_STAGE11_CAPTURE_CONTRACT_SELFTEST_PASS "
          "explicit_unknown=true known_c5_vt_block=true no_auto_permission=true "
          "synthetic_complete_needs_review=true source_drift_detected=true "
          "symlink_refused=true no_tv_access=true capture=false lights_output=false")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--selftest", action="store_true")
    modes.add_argument("--report", action="store_true", help="print reviewed C5 evidence, offline")
    modes.add_argument("--audit-repo", type=Path, metavar="PATH",
                       help="check assumptions against a local source checkout (read-only)")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return 0
    if args.report:
        print(json.dumps({"schema": "c5-stage11-evidence-v1", "device_access": False,
                          "deploy_authorized": False,
                          "candidates": [assess(c) for c in PROFILES]}, indent=2, sort_keys=True))
        return 0
    try:
        report = audit_repo(args.audit_repo)
    except (OSError, UnicodeError, ValueError) as exc:
        print("C5_STAGE11_SOURCE_AUDIT_REFUSE:", exc, file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["status"] == "STATIC_SOURCE_CONTRACT_MATCH" else 1


if __name__ == "__main__":
    raise SystemExit(main())
