#!/usr/bin/env python3
"""Offline-only LG C5 JPEG audit: full decode, quantization and zone statistics.

This program never connects to the TV, captures a frame, writes an image
or prints source image bytes. It needs Pillow 12.3.0 for full JPEG decoding.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import stat
import struct
import sys
from io import BytesIO

try:
    from PIL import Image, ImageChops, ImageStat, JpegImagePlugin, UnidentifiedImageError
except ImportError as exc:
    raise SystemExit("C5_JPEG_AUDIT_DEPENDENCY_REQUIRED: Pillow 12.3.0 (offline only)") from exc

WIDTH, HEIGHT = 320, 180
MAX_BYTES = 2 * 1024 * 1024


class InvalidJpeg(ValueError):
    pass


def _audit_bytes(data: bytes, *, source: str = "synthetic"):
    if len(data) < 16 or len(data) > MAX_BYTES:
        raise InvalidJpeg("size outside 16..2097152 bytes")
    if not data.startswith(b"\xff\xd8") or not data.endswith(b"\xff\xd9"):
        raise InvalidJpeg("missing JPEG start/end markers")
    try:
        with Image.open(BytesIO(data)) as probe:
            if probe.format != "JPEG" or probe.size != (WIDTH, HEIGHT):
                raise InvalidJpeg("not a 320x180 JPEG")
            probe.verify()
        with Image.open(BytesIO(data)) as decoded:
            decoded.load()  # Full entropy scan, not the Stage 5 header-only check
            if decoded.format != "JPEG" or decoded.size != (WIDTH, HEIGHT):
                raise InvalidJpeg("JPEG changed across verification")
            sampling = JpegImagePlugin.get_sampling(decoded)
            tables = decoded.quantization or {}
            if not tables:
                raise InvalidJpeg("JPEG has no quantization tables")
            fingerprint = hashlib.sha256()
            for key, table in sorted(tables.items()):
                fingerprint.update(struct.pack(">H", key))
                if len(table) != 64:
                    raise InvalidJpeg("malformed JPEG quantization table")
                for value in table:
                    if not 0 <= value <= 65535:
                        raise InvalidJpeg("out-of-range quantization coefficient")
                    fingerprint.update(struct.pack(">H", value))
            rgb = decoded.convert("RGB")
            grayscale = rgb.convert("L")
            intensity = ImageStat.Stat(grayscale)
            edge_boxes = {
                "top": (0, 0, WIDTH, 18),
                "right": (WIDTH - 32, 0, WIDTH, HEIGHT),
                "bottom": (0, HEIGHT - 18, WIDTH, HEIGHT),
                "left": (0, 0, 32, HEIGHT),
            }
            edges = {
                key: [round(v, 1) for v in ImageStat.Stat(rgb.crop(box)).mean]
                for key, box in edge_boxes.items()
            }
            return {
                "source": source,
                "jpeg_bytes": len(data),
                "fully_decoded": True,
                "dimensions": [WIDTH, HEIGHT],
                "sampling_code": sampling,
                "quantization_tables": len(tables),
                "quantization_fingerprint": fingerprint.hexdigest()[:16],
                "mean_luminance": round(intensity.mean[0], 2),
                "luminance_stddev": round(intensity.stddev[0], 2),
                "low_detail_warning": intensity.stddev[0] < 2,
                "edge_rgb_means": edges,
            }, rgb
    except (OSError, UnidentifiedImageError, SyntaxError, struct.error) as exc:
        raise InvalidJpeg("JPEG failed complete decoding or metadata validation") from exc


def audit_file(path: Path, source: str):
    # Refuse links/special files; cap file size; never print source paths.
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or not 16 <= metadata.st_size <= MAX_BYTES:
        raise InvalidJpeg("not an expected regular bounded JPEG file")
    with path.open("rb") as fh:
        data = fh.read(MAX_BYTES + 1)
    if len(data) != metadata.st_size:
        raise InvalidJpeg("JPEG changed while reading")
    return _audit_bytes(data, source=source)


def compare(native, oneshot):
    d1, im1 = native
    d2, im2 = oneshot
    diff = ImageStat.Stat(ImageChops.difference(im1, im2))
    mae = sum(diff.mean) / 3.0
    rms = math.sqrt(sum(x * x for x in diff.rms) / 3.0)
    return {
        "native": d1,
        "oneshot": d2,
        "quantization_tables_match":
            d1["quantization_fingerprint"] == d2["quantization_fingerprint"],
        "sampling_matches": d1["sampling_code"] == d2["sampling_code"],
        "mean_absolute_rgb_difference": round(mae, 3),
        "psnr_db": None if rms == 0 else round(20 * math.log10(255.0 / rms), 2),
        "pixels_identical": rms == 0,
        "interpretation": (
            "Pixel comparison requires the same static, known-clear scene. "
            "Identical quantization tables do not prove an encoder's quality setting; "
            "different tables rule out a quality-equivalent comparison."
        ),
    }


def run_selftest():
    # Synthetic gradient and checkerboard: generated entirely in memory.
    image = Image.new("RGB", (WIDTH, HEIGHT))
    pixels = image.load()
    for y in range(HEIGHT):
        for x in range(WIDTH):
            pixels[x, y] = (
                (x * 3 + y) % 256,
                (y * 2 + (x // 13) * 35) % 256,
                (x + y * 5) % 256,
            )

    def encode(quality):
        output = BytesIO()
        image.save(output, "JPEG", quality=quality, subsampling=0)
        return output.getvalue()

    jpg75, jpg95 = encode(75), encode(95)
    first, second = _audit_bytes(jpg75), _audit_bytes(jpg95)
    comparison = compare(first, second)
    assert first[0]["fully_decoded"]
    assert not first[0]["low_detail_warning"]
    assert not comparison["quantization_tables_match"]
    assert comparison["mean_absolute_rgb_difference"] >= 0

    for broken in (jpg75[:-2], jpg75[:80] + b"wrong" + jpg75[-2:]):
        try:
            _audit_bytes(broken)
        except InvalidJpeg:
            pass
        else:
            raise AssertionError("malformed or truncated JPEG was accepted")

    flat = Image.new("RGB", (WIDTH, HEIGHT), (12, 12, 12))
    output = BytesIO()
    flat.save(output, "JPEG", quality=75)
    assert _audit_bytes(output.getvalue())[0]["low_detail_warning"]
    print(
        "C5_STAGE6_SYNTHETIC_JPEG_AUDIT_PASS "
        "full_decode=true quality_fingerprint_differentiated=true "
        "malformed_refused=true low_detail_flagged=true capture=false"
    )
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, help="Local JPEG captured from known-clear material")
    parser.add_argument("--oneshot", type=Path, help="Local JPEG of the same static scene")
    parser.add_argument("--selftest", action="store_true", help="Test only generated synthetic data")
    args = parser.parse_args()
    if args.selftest:
        return run_selftest()
    if not args.native or not args.oneshot:
        parser.error("both --native and --oneshot are required (or --selftest)")
    try:
        result = compare(audit_file(args.native, "native"), audit_file(args.oneshot, "oneshot"))
    except (InvalidJpeg, OSError) as exc:
        print("C5_JPEG_AUDIT_REFUSE:", str(exc), file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
