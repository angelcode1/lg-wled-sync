#!/usr/bin/env python3
"""C5 Stage 9 offline 64x48 NV12 colour/perimeter reference test.

No TV access, native library loading, LS2 request, V4L2 handle, capture,
network, image output or pixel logging. Synthetic reference images are
constructed in memory. The optional raw-file reader accepts an ALREADY
EXISTING 64x48 8-bit NV12 file, not hardware pointers or device nodes.

IMPORTANT: 64x48 Y(3072) + UV(1536) is consistent with the statically
observed LG lighting-controller buffer sizes but does NOT prove NV12,
BT.601 limited range, chroma order, stride, colour primaries or lease safety.
RGB numeric measurements below are under EXPLICIT layout/matrix assumptions.
"""
import argparse
import json
from pathlib import Path
import stat
import sys

WIDTH, HEIGHT = 64, 48
Y_BYTES = WIDTH * HEIGHT
UV_BYTES = Y_BYTES // 2
NV12_BYTES = Y_BYTES + UV_BYTES
EDGE_BOXES = {
    "top": (16, 0, 48, 8),
    "right": (56, 12, 64, 36),
    "bottom": (16, 40, 48, 48),
    "left": (0, 12, 8, 36),
}
PALETTE = {
    "top": (235, 20, 25),
    "right": (20, 205, 60),
    "bottom": (30, 50, 220),
    "left": (220, 160, 25),
}


class InvalidYuv(ValueError):
    pass


def _clip(x):
    return max(0, min(255, int(x)))


def _yuv_pixel(y, u, v, uv_order="NV12"):
    if uv_order == "NV21":
        u, v = v, u
    elif uv_order != "NV12":
        raise InvalidYuv("unknown chroma byte order")
    c = max(0, y - 16)
    d = u - 128
    e = v - 128
    # Rec. 601 8-bit studio-range approximation, not a confirmed C5 format.
    return (
        _clip((298 * c + 409 * e + 128) >> 8),
        _clip((298 * c - 100 * d - 208 * e + 128) >> 8),
        _clip((298 * c + 516 * d + 128) >> 8),
    )


def decode_nv12(data, *, uv_order="NV12"):
    if type(data) is not bytes or len(data) != NV12_BYTES:
        raise InvalidYuv("expected exactly 3072 Y and 1536 interleaved UV bytes")
    if uv_order not in ("NV12", "NV21"):
        raise InvalidYuv("unsupported chroma byte order")
    pixels = []
    for y in range(HEIGHT):
        row = y * WIDTH
        uv_row = Y_BYTES + (y // 2) * WIDTH
        for x in range(WIDTH):
            pos = uv_row + (x // 2) * 2
            pixels.append(_yuv_pixel(data[row + x], data[pos], data[pos + 1], uv_order))
    return pixels


def rgb_to_yuv(r, g, b):
    # Integer coefficients for nominal studio-range BT.601 signal generation.
    return (
        _clip(round(16 + 0.257 * r + 0.504 * g + 0.098 * b)),
        _clip(round(128 - 0.148 * r - 0.291 * g + 0.439 * b)),
        _clip(round(128 + 0.439 * r - 0.368 * g - 0.071 * b)),
    )


def synthetic_rgb():
    pixels = [(18, 18, 18)] * (WIDTH * HEIGHT)
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if y < 8:
                rgb = PALETTE["top"]
            elif y >= 40:
                rgb = PALETTE["bottom"]
            elif x < 8:
                rgb = PALETTE["left"]
            elif x >= 56:
                rgb = PALETTE["right"]
            else:
                rgb = (18, 18, 18)
            pixels[y * WIDTH + x] = rgb
    return pixels


def encode_synthetic_nv12(rgb_pixels):
    if len(rgb_pixels) != Y_BYTES:
        raise InvalidYuv("synthetic RGB geometry mismatch")
    y_plane = bytearray(Y_BYTES)
    uv_plane = bytearray(UV_BYTES)
    for y in range(0, HEIGHT, 2):
        for x in range(0, WIDTH, 2):
            chroma_u = chroma_v = 0
            for dy in range(2):
                for dx in range(2):
                    i = (y + dy) * WIDTH + x + dx
                    yc, uc, vc = rgb_to_yuv(*rgb_pixels[i])
                    y_plane[i] = yc
                    chroma_u += uc
                    chroma_v += vc
            p = (y // 2) * WIDTH + x
            uv_plane[p] = _clip(round(chroma_u / 4))
            uv_plane[p + 1] = _clip(round(chroma_v / 4))
    return bytes(y_plane + uv_plane)


def edge_means(pixels):
    if len(pixels) != Y_BYTES:
        raise InvalidYuv("decoded RGB geometry mismatch")
    result = {}
    for name, (x0, y0, x1, y1) in EDGE_BOXES.items():
        totals = [0, 0, 0]
        n = (x1 - x0) * (y1 - y0)
        for y in range(y0, y1):
            for x in range(x0, x1):
                pix = pixels[y * WIDTH + x]
                for k in range(3):
                    totals[k] += pix[k]
        result[name] = tuple(round(v / n, 3) for v in totals)
    return result


def global_mean(pixels):
    if len(pixels) != Y_BYTES:
        raise InvalidYuv("decoded RGB geometry mismatch")
    return tuple(round(sum(p[k] for p in pixels) / Y_BYTES, 3) for k in range(3))


def compare_palette(observed, reference=PALETTE):
    return {
        edge: round(max(abs(o - r) for o, r in zip(observed[edge], reference[edge])), 3)
        for edge in EDGE_BOXES
    }


def rotate_180(pixels):
    if len(pixels) != Y_BYTES:
        raise InvalidYuv("decoded RGB geometry mismatch")
    return list(reversed(pixels))


def inspect(data):
    pix = decode_nv12(data)
    return {
        "source": "unverified_existing_file_or_synthetic",
        "assumed_geometry": [WIDTH, HEIGHT],
        "assumed_layout": "8-bit NV12, contiguous 3072-byte Y plus 1536-byte UV, no stride padding",
        "assumed_matrix": "BT.601 studio-range integer approximation",
        "center_edge_rgb_means": edge_means(pix),
        "whole_frame_rgb_mean": global_mean(pix),
        "known_reference_available": False,
        "capture_source_identified": False,
        "colour_matrix_confirmed_by_vendor": False,
        "unsafe_buffer_lease_or_device_attempted": False,
        "hardware_fps_measured": False,
        "raw_pixels_saved_or_printed": False,
        "verdict": "INCONCLUSIVE_UNVERIFIED_FORMAT_AND_SOURCE",
    }


def safe_read_existing(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_size != NV12_BYTES:
        raise InvalidYuv("require existing regular 4608-byte file; no links or devices")
    with path.open("rb") as handle:
        data = handle.read(NV12_BYTES + 1)
    if len(data) != info.st_size:
        raise InvalidYuv("file changed during bounded read")
    return data


def selftest():
    rgb = synthetic_rgb()
    payload = encode_synthetic_nv12(rgb)
    assert len(payload) == 4608
    observed = edge_means(decode_nv12(payload))
    errors = compare_palette(observed)
    assert max(errors.values()) <= 5, errors
    assert len(set(observed.values())) == 4

    rotated = encode_synthetic_nv12(rotate_180(rgb))
    assert max(compare_palette(edge_means(decode_nv12(rotated))).values()) >= 100
    swapped = edge_means(decode_nv12(payload, uv_order="NV21"))
    assert max(compare_palette(swapped).values()) >= 60

    black = [(0, 0, 0)] * Y_BYTES
    decoded_black = decode_nv12(encode_synthetic_nv12(black))
    assert all(max(v) <= 2 for v in edge_means(decoded_black).values())
    assert inspect(encode_synthetic_nv12(black))["verdict"].startswith("INCONCLUSIVE_")

    # An unsplit global mean cannot carry spatial location, even if accurate.
    assert global_mean(rgb) == global_mean(rotate_180(rgb))
    assert edge_means(rgb) != edge_means(rotate_180(rgb))

    for bad in (b"", payload[:-1], payload + b"\0", payload[:Y_BYTES], bytearray(payload)):
        try:
            decode_nv12(bad)
        except InvalidYuv:
            pass
        else:
            raise AssertionError("wrong layout/length was accepted")

    print(
        "C5_STAGE9_NV12_SPATIAL_SELFTEST_PASS "
        "geometry=64x48 planes=3072+1536 four_edges=true "
        "palette_error_at_most_5=true orientation_guard=true "
        "chroma_swap_detected=true black_valid=true "
        "global_mean_loses_geometry=true malformed_refused=true "
        "layout_vendor_confirmed=false capture=false pixels_written=false"
    )
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true", help="offline synthetic fixtures only")
    parser.add_argument("--existing-nv12", type=Path, help="already-existing regular 4608-byte file")
    parser.add_argument("--assume-bt601-limited", action="store_true", help="explicitly acknowledge unverified colour format assumptions")
    args = parser.parse_args()
    if args.selftest:
        if args.existing_nv12 or args.assume_bt601_limited:
            parser.error("--selftest must be used alone")
        return selftest()
    if not args.existing_nv12 or not args.assume_bt601_limited:
        parser.error("use --selftest or --existing-nv12 FILE --assume-bt601-limited")
    try:
        result = inspect(safe_read_existing(args.existing_nv12))
    except (InvalidYuv, OSError) as exc:
        print("C5_STAGE9_NV12_REFUSE:", str(exc), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0  # Analytics are inconclusive by design, even with valid bytes.


if __name__ == "__main__":
    raise SystemExit(main())