#!/usr/bin/env python3
"""Convert a measured LG C5 HyperDRM profile into a model-bound config fragment."""
import argparse
import json
import math
import re
from pathlib import Path

C5_MODEL = re.compile(r"OLED[0-9]{2}C5[A-Za-z0-9._-]*", re.ASCII)

def convert(profile, model):
    if not isinstance(model, str) or not C5_MODEL.fullmatch(model):
        raise ValueError("expected exact LG C5 modelName, e.g. OLED65C5PSA")
    if not isinstance(profile, dict) or profile.get("profile") != "custom":
        raise ValueError("Save a valid custom HyperDRM profile first")
    if profile.get("model") != model:
        raise ValueError("saved calibration model does not match TV modelName")
    points = profile.get("points")
    if not isinstance(points, list) or not 4 <= len(points) <= 16:
        raise ValueError("expected 4..16 measured screen-rim points")
    checked, edges, last = [], set(), -1.0
    for i, point in enumerate(points):
        if not isinstance(point, dict):
            raise ValueError(f"point {i} must be an object")
        position, region = point.get("position"), point.get("region")
        if (isinstance(position, bool) or not isinstance(position, (float, int))
                or not math.isfinite(position) or not 0 <= position < 4
                or position <= last):
            raise ValueError(f"invalid/unsorted rim position at point {i}")
        if isinstance(region, bool) or not isinstance(region, int) or not 0 <= region < 16:
            raise ValueError(f"invalid histogram region ID at point {i}")
        checked.append({"position": position, "region": region})
        edges.add(int(position))
        last = position
    if edges != {0, 1, 2, 3}:
        raise ValueError(f"incomplete edge coverage: {sorted(edges)}")
    return {
        "capture_mode": "auto-drm-first",
        "histogram": {"profile": "custom", "model": model, "points": checked},
    }

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", type=Path, help="saved HyperDRM calibration.json")
    parser.add_argument("--model", required=True, help="exact modelName reported by the TV")
    parser.add_argument("--output", type=Path, default=Path("c5-histogram-config.json"))
    args = parser.parse_args(argv)
    try:
        result = convert(json.loads(args.profile.read_text(encoding="utf-8")), args.model)
    except (OSError, json.JSONDecodeError, ValueError) as error:
        parser.error(str(error))
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Validated {len(result['histogram']['points'])} points for {args.model}.")
    print(f"Wrote {args.output}; merge into existing full config.json.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
