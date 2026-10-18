#!/usr/bin/env python3
"""Stage 7 offline-only C5 perimeter color-fidelity audit.

Requires owner-held, already existing 320x180 JPEGs for a stationary,
independently verified clear source. No TV, LS2, capture or image output.
The selftest uses synthetic, in-memory JPEGs; no source image is retained.
"""
import argparse
from io import BytesIO
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageStat
from c5_jpeg_audit import WIDTH, HEIGHT, _audit_bytes, audit_file, InvalidJpeg

# Central edge strips avoid corners where adjacent sides overlap.
# Their sizes match the Stage 6 coarse top/right/bottom/left strips.
CENTRAL_EDGE_BOXES = {
    "top": (80, 0, 240, 18),
    "right": (288, 45, 320, 135),
    "bottom": (80, 162, 240, 180),
    "left": (0, 45, 32, 135),
}


def central_edge_means(rgb):
    """Use decoded pixels, not JPEG file size, to describe perimeter data."""
    return {
        label: [round(channel, 2) for channel in ImageStat.Stat(rgb.crop(box)).mean]
        for label, box in CENTRAL_EDGE_BOXES.items()
    }


def edge_comparison(native, oneshot):
    native_meta, native_rgb = native
    shot_meta, shot_rgb = oneshot
    first, second = central_edge_means(native_rgb), central_edge_means(shot_rgb)
    differences = {
        edge: round(max(abs(a - b) for a, b in zip(first[edge], second[edge])), 2)
        for edge in CENTRAL_EDGE_BOXES
    }
    return {
        "native": {"jpeg_bytes": native_meta["jpeg_bytes"], "center_edge_rgb": first},
        "oneshot": {"jpeg_bytes": shot_meta["jpeg_bytes"], "center_edge_rgb": second},
        "max_edge_channel_difference": max(differences.values()),
        "per_edge_max_channel_difference": differences,
        "same_quantization_tables": (native_meta["quantization_fingerprint"]
                                     == shot_meta["quantization_fingerprint"]),
        "same_sampling": native_meta["sampling_code"] == shot_meta["sampling_code"],
        "caution": (
            "Compare only identical stationary DRM-free scenes with the same image geometry; "
            "these numbers do not establish capture-source identity, DRM authorization, "
            "or sustained frame rate. No source image bytes are saved or sent."
        ),
    }


def synthetic_reference():
    palette = {
        "top": (235, 20, 25),
        "right": (20, 205, 60),
        "bottom": (30, 50, 220),
        "left": (220, 160, 25),
    }
    im = Image.new("RGB", (WIDTH, HEIGHT), (18, 18, 18))
    draw = ImageDraw.Draw(im)
    draw.rectangle((0, 0, WIDTH - 1, 17), fill=palette["top"])
    draw.rectangle((0, 162, WIDTH - 1, HEIGHT - 1), fill=palette["bottom"])
    draw.rectangle((0, 18, 31, 161), fill=palette["left"])
    draw.rectangle((288, 18, WIDTH - 1, 161), fill=palette["right"])
    return im, palette


def jpeg_bytes(image, quality):
    memory = BytesIO()
    image.save(memory, "JPEG", quality=quality, subsampling=0)
    return memory.getvalue()


def selftest():
    im, palette = synthetic_reference()
    native = _audit_bytes(jpeg_bytes(im, 75), source="synthetic-native")
    oneshot = _audit_bytes(jpeg_bytes(im, 95), source="synthetic-oneshot")
    report = edge_comparison(native, oneshot)
    for label, reference in palette.items():
        for sample in (report["native"]["center_edge_rgb"][label],
                       report["oneshot"]["center_edge_rgb"][label]):
            assert max(abs(a - b) for a, b in zip(sample, reference)) <= 8, label
    assert report["max_edge_channel_difference"] <= 8, report
    assert not report["same_quantization_tables"]

    flipped = _audit_bytes(jpeg_bytes(im.transpose(Image.Transpose.ROTATE_180), 75))
    swapped = edge_comparison(native, flipped)
    assert swapped["max_edge_channel_difference"] >= 100

    solid = Image.new("RGB", (WIDTH, HEIGHT), (0, 0, 0))
    uniform = _audit_bytes(jpeg_bytes(solid, 75))
    uniform_edges = central_edge_means(uniform[1])
    assert all(max(v) <= 1 for v in uniform_edges.values())
    print(
        "C5_STAGE7_OFFLINE_EDGE_FIDELITY_PASS "
        "decoded=true four_edges=true palette_accuracy=true "
        "orientation_mismatch_detected=true black_valid=true "
        "capture=false no_images_written=true"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, help="Owner-held known-clear 320x180 native JPEG")
    parser.add_argument("--oneshot", type=Path, help="Same stationary known-clear scene, one-shot JPEG")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return
    if args.native is None or args.oneshot is None:
        parser.error("provide both --native and --oneshot, or --selftest")
    try:
        result = edge_comparison(audit_file(args.native, "native"),
                                 audit_file(args.oneshot, "oneshot"))
    except (InvalidJpeg, OSError, ValueError) as exc:
        parser.exit(1, f"C5_STAGE7_EDGE_REFUSE: {exc}\n")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
