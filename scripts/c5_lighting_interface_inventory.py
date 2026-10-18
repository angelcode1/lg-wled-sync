#!/usr/bin/env python3
"""Read-only offline webOS C5 lighting and histogram ABI inventory.

Reads the owner-exported ZIP, not the TV. Extracts only selected ELF files to a
private temporary directory for static readelf inspection; never executes them.
No keys, images, DRM buffers, permissions or device state are accessed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from zipfile import ZipFile, BadZipFile

API_NAMES = ("lightingcontroller", "livepick", "livepickfull", "capture")
ELF_PATHS = (
    "usr/sbin/lightingcontroller",
    "usr/sbin/livepick",
    "usr/lib/libvtcapture.so.1.0.0",
    "usr/lib/libkadaptor.so.2.0.1",
)
MAX_API = 64 * 1024
MAX_ELF = 8 * 1024 * 1024
INTERESTING = re.compile(r"(?:histogram|KADP_APR_|vtCapture_|YUV420toRGB|kMeansClustering|AplMinMax|OSDAlphaAPL|ledController|GetColorHistogram)", re.I)

def parse_dynsym(lines):
    exported, imported = [], []
    for line in lines.splitlines():
        parts = line.split()
        if len(parts) < 8 or parts[3] not in ("FUNC", "OBJECT"):
            continue
        symbol = parts[7].split("@", 1)[0]
        if not INTERESTING.search(symbol):
            continue
        if parts[6] == "UND":
            imported.append(symbol)
        elif parts[4] in ("GLOBAL", "WEAK"):
            exported.append(symbol)
    return sorted(set(exported)), sorted(set(imported))

def inventory(archive):
    result = {"source": "owner-supplied-offline-zip", "executable_run": False,
              "api_files": {}, "elf_symbol_evidence": {}, "interpretation":
              "Only listed declarations and dynamic symbols; neither proves runtime access or DRM permission."}
    with ZipFile(archive) as zf:
        members = {}
        for info in zf.infolist():
            anchor = "/files/"
            if anchor in info.filename and not info.is_dir():
                key = info.filename.split(anchor, 1)[1]
                if key in members:
                    raise ValueError("duplicate system path in archive")
                members[key] = info
        for name in API_NAMES:
            path = f"usr/share/luna-service2/api-permissions.d/com.webos.service.{name}.api.json"
            info = members.get(path)
            if info is None:
                result["api_files"][name] = {"status": "not_collected"}
                continue
            if info.file_size > MAX_API:
                raise ValueError(f"oversized API file: {name}")
            raw = zf.read(info)
            obj = json.loads(raw)
            if not isinstance(obj, dict):
                raise ValueError(f"unexpected API declaration: {name}")
            groups = {str(group): sorted(set(methods)) for group, methods in obj.items()
                      if isinstance(methods, list) and all(isinstance(m, str) for m in methods)}
            all_methods = sorted(set(m for methods in groups.values() for m in methods))
            result["api_files"][name] = {
                "status": "collected", "sha256": hashlib.sha256(raw).hexdigest(),
                "groups": {k: len(v) for k, v in groups.items()},
                "methods": all_methods,
                "declared_histogram_or_region_color_methods": [m for m in all_methods
                    if re.search(r"histogram|region.?colou?r|screen.?colou?r", m, re.I)],
            }
        for path in ELF_PATHS:
            info = members.get(path)
            if info is None:
                result["elf_symbol_evidence"][path] = {"status": "not_collected"}
                continue
            if info.file_size > MAX_ELF:
                raise ValueError(f"oversized ELF: {path}")
            data = zf.read(info)
            if not data.startswith(b"\x7fELF"):
                raise ValueError(f"not ELF: {path}")
            with tempfile.TemporaryDirectory(prefix="c5-static-symbols-") as work:
                tmp = Path(work) / "inspected.elf"
                tmp.write_bytes(data)
                p = subprocess.run(["readelf", "--dyn-syms", "--wide", str(tmp)],
                                   capture_output=True, text=True, timeout=10, check=True)
            exports, imports = parse_dynsym(p.stdout)
            result["elf_symbol_evidence"][path] = {
                "status": "collected", "sha256": hashlib.sha256(data).hexdigest(),
                "exports": exports, "imports": imports,
            }
    return result

def selftest():
    e, i = parse_dynsym("""
  5: 00000000 0 FUNC GLOBAL DEFAULT UND vtCapture_process
  6: 00019dfd 172 FUNC GLOBAL DEFAULT 11 vtCapture_getHistogram
  7: 00012c19 4 FUNC GLOBAL DEFAULT 11 _ZN11VtHistogram13initHistogramEv
  8: 00000000 0 FUNC GLOBAL DEFAULT UND irrelevant
""")
    assert i == ["vtCapture_process"]
    assert e == ["_ZN11VtHistogram13initHistogramEv", "vtCapture_getHistogram"]
    print("C5_LIGHTING_STATIC_INVENTORY_SELFTEST_PASS execute=false capture=false")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
    elif args.archive:
        try:
            print(json.dumps(inventory(args.archive), indent=2, sort_keys=True))
        except (BadZipFile, OSError, ValueError, subprocess.SubprocessError, json.JSONDecodeError) as e:
            sys.exit("C5_LIGHTING_INVENTORY_REFUSE: " + str(e))
    else:
        parser.error("Provide --archive ZIP or --selftest")
