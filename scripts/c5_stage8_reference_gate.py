#!/usr/bin/env python3
"""C5 Stage 8: offline reference-based perimeter fidelity gate.

In-memory only; no device access, screenshots, raw output, network, or files
written. An independently authored 320x180 PNG reference is required when
inspecting already-existing JPEGs. The synthetic selftest needs Pillow.

A close match between two captures is NOT evidence of true RGB fidelity.
Reference-matched and colorful are independent requirements. Nominal RGB is
not a measurement of display primaries, EOTF or real LED colorimetry.
"""
import argparse
from io import BytesIO
import json
from pathlib import Path
import stat
import sys

from PIL import Image, ImageDraw, ImageStat, UnidentifiedImageError

W, H = 320, 180
MAX_BYTES = 2 * 1024 * 1024
BOXES = {
    "top": (80, 0, 240, 18),
    "right": (288, 45, 320, 135),
    "bottom": (80, 162, 240, 180),
    "left": (0, 45, 32, 135),
}
MAX_CHANNEL_ERROR = 12.0
MIN_BRIGHT_CHANNEL = 64.0
MIN_CHROMA_SPREAD = 40.0
MIN_EDGE_SEPARATION = 50.0


class InvalidImage(ValueError):
    pass


def load_image(data: bytes, expected_format: str) -> Image.Image:
    if not 16 <= len(data) <= MAX_BYTES:
        raise InvalidImage("image size out of bounds")
    try:
        with Image.open(BytesIO(data)) as probe:
            if probe.format != expected_format or probe.size != (W, H):
                raise InvalidImage("unexpected format or dimensions")
            probe.verify()
        with Image.open(BytesIO(data)) as decoded:
            if decoded.format != expected_format or decoded.size != (W, H):
                raise InvalidImage("format/geometry changed while decoding")
            if expected_format == "JPEG" and (
                not data.startswith(b"\xff\xd8") or not data.endswith(b"\xff\xd9")
            ):
                raise InvalidImage("missing JPEG start/end marker")
            if expected_format == "JPEG" and not decoded.quantization:
                raise InvalidImage("JPEG quantization information missing")
            if expected_format == "PNG" and decoded.mode not in ("RGB", "RGBA"):
                raise InvalidImage("reference PNG must be RGB/RGBA")
            decoded.load()
            return decoded.convert("RGB")
    except (OSError, SyntaxError, UnidentifiedImageError, ValueError) as exc:
        if isinstance(exc, InvalidImage):
            raise
        raise InvalidImage("invalid or truncated image") from exc


def bounded_file(path: Path) -> bytes:
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or not 16 <= info.st_size <= MAX_BYTES:
        raise InvalidImage("not a bounded regular image")
    with path.open("rb") as file:
        data = file.read(MAX_BYTES + 1)
    if len(data) != info.st_size:
        raise InvalidImage("image size changed during read")
    return data


def edges(image: Image.Image):
    if image.mode != "RGB" or image.size != (W, H):
        raise InvalidImage("wrong decoded image properties")
    return {
        side: [float(x) for x in ImageStat.Stat(image.crop(rect)).mean]
        for side, rect in BOXES.items()
    }


def is_informative(reference):
    values = list(reference.values())
    colorful = [
        v for v in values
        if max(v) >= MIN_BRIGHT_CHANNEL and max(v) - min(v) >= MIN_CHROMA_SPREAD
    ]
    separated = all(
        max(abs(a - b) for a, b in zip(first, second)) >= MIN_EDGE_SEPARATION
        for i, first in enumerate(values) for second in values[i + 1:]
    )
    return len(colorful) == 4 and separated


def assess(reference_rgb: Image.Image, native_rgb: Image.Image, oneshot_rgb: Image.Image):
    """Diagnostics only; never asserts provenance, source time, or DRM status."""
    source = edges(reference_rgb)
    native, one = edges(native_rgb), edges(oneshot_rgb)
    informative = is_informative(source)

    def deviations(observed):
        return {
            side: round(max(abs(a - b) for a, b in zip(observed[side], source[side])), 3)
            for side in BOXES
        }

    native_error, oneshot_error = deviations(native), deviations(one)
    pair_error = {
        side: round(max(abs(a - b) for a, b in zip(native[side], one[side])), 3)
        for side in BOXES
    }
    max_reference_error = max(*native_error.values(), *oneshot_error.values())
    if not informative:
        verdict = "INCONCLUSIVE_REFERENCE_TOO_DARK_OR_NOT_FOUR_DISTINCT_COLORS"
    elif max_reference_error > MAX_CHANNEL_ERROR:
        verdict = "REFERENCE_MISMATCH"
    else:
        verdict = "SYNTHETIC_OR_REFERENCE_RGB_MATCH"
    return {
        "stage": 8,
        "verdict": verdict,
        "independent_reference_informative": informative,
        "thresholds_nominal_rgb": {
            "max_channel_error": MAX_CHANNEL_ERROR,
            "min_bright_channel": MIN_BRIGHT_CHANNEL,
            "min_chroma_spread": MIN_CHROMA_SPREAD,
            "min_pairwise_edge_separation": MIN_EDGE_SEPARATION,
        },
        "max_native_reference_error": native_error,
        "max_oneshot_reference_error": oneshot_error,
        "max_between_capture_error": pair_error,
        "maximum_reference_channel_error": round(max_reference_error, 3),
        "raw_images_written": False,
        "source_identity_confirmed": False,
        "quality_equivalence_confirmed": False,
        "protected_content_access_claimed": False,
        "physical_led_color_accuracy_claimed": False,
        "note": (
            "Nominal, independently authored sRGB reference and known identical static "
            "clear source must be established outside this offline tool. Different "
            "frame planes, colour transfer, and JPEG quality can affect comparison. "
            "A passing edge gate does not establish live calibration or sustained FPS."
        ),
    }


def encode_jpeg(image, quality):
    buf = BytesIO()
    image.save(buf, "JPEG", quality=quality, subsampling=0)
    return buf.getvalue()


def synthetic_reference():
    rgb = Image.new("RGB", (W, H), (10, 10, 10))
    palette = {"top": (235, 20, 25), "right": (20, 205, 60),
               "bottom": (30, 50, 220), "left": (220, 160, 25)}
    draw = ImageDraw.Draw(rgb)
    draw.rectangle((0, 0, W - 1, 17), fill=palette["top"])
    draw.rectangle((0, 162, W - 1, H - 1), fill=palette["bottom"])
    draw.rectangle((0, 18, 31, 161), fill=palette["left"])
    draw.rectangle((288, 18, W - 1, 161), fill=palette["right"])
    return rgb


def selftest():
    reference = synthetic_reference()
    native = load_image(encode_jpeg(reference, 75), "JPEG")
    shot = load_image(encode_jpeg(reference, 95), "JPEG")
    valid = assess(reference, native, shot)
    assert valid["verdict"] == "SYNTHETIC_OR_REFERENCE_RGB_MATCH", valid
    assert valid["independent_reference_informative"]
    assert valid["maximum_reference_channel_error"] <= MAX_CHANNEL_ERROR

    rotated = load_image(encode_jpeg(reference.transpose(Image.Transpose.ROTATE_180), 75), "JPEG")
    assert assess(reference, rotated, shot)["verdict"] == "REFERENCE_MISMATCH"
    wrong = load_image(encode_jpeg(Image.new("RGB", (W, H), (0, 0, 0)), 75), "JPEG")
    assert assess(reference, wrong, wrong)["verdict"] == "REFERENCE_MISMATCH"
    black_reference = Image.new("RGB", (W, H), (0, 0, 0))
    assert assess(black_reference, wrong, wrong)["verdict"].startswith("INCONCLUSIVE_")
    assert assess(black_reference, native, shot)["verdict"].startswith("INCONCLUSIVE_")
    for broken in (encode_jpeg(reference, 75)[:-3], b"not a jpeg"):
        try:
            load_image(broken, "JPEG")
        except InvalidImage:
            pass
        else:
            raise AssertionError("invalid JPEG was accepted")

    print("C5_STAGE8_REFERENCE_FIDELITY_SELFTEST_PASS "
          "independent_bright_reference=true orientation_guard=true "
          "near_black_not_pass=true pair_agreement_not_fidelity=true "
          "malformed_refused=true capture=false no_images_written=true")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true", help="synthetic, in-memory fixtures only")
    parser.add_argument("--reference", type=Path, help="independently authored RGB PNG at 320x180")
    parser.add_argument("--native", type=Path, help="existing known-clear JPEG at 320x180")
    parser.add_argument("--oneshot", type=Path, help="existing known-clear JPEG at 320x180")
    args = parser.parse_args()
    if args.selftest:
        if any((args.reference, args.native, args.oneshot)):
            parser.error("--selftest cannot be combined with file arguments")
        selftest()
        return 0
    if not all((args.reference, args.native, args.oneshot)):
        parser.error("all of --reference, --native and --oneshot are required")
    try:
        reference = load_image(bounded_file(args.reference), "PNG")
        native = load_image(bounded_file(args.native), "JPEG")
        oneshot = load_image(bounded_file(args.oneshot), "JPEG")
        report = assess(reference, native, oneshot)
    except (InvalidImage, OSError) as exc:
        print(f"C5_STAGE8_REFUSE: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True, indent=2))
    return 0 if report["verdict"] == "SYNTHETIC_OR_REFERENCE_RGB_MATCH" else 1


if __name__ == "__main__":
    raise SystemExit(main())