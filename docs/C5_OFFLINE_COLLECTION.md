# C5 native capture: offline evidence collection

Updated 2026-10-29. This procedure downloads existing files through the owner's
existing SSH access. It writes only on the Mac. It does not run a capture service,
open a video device, change permissions, restart a service or send light output.

## Current analysis baseline

The main source snapshot reviewed was
`5e1e9cc8d1cbb0e3af4aeae275dfaedb79cfc542`.
Its [CI run](https://github.com/angelcode1/lg-wled-sync/actions/runs/39257202708)
passed Rust formatting, nextest, Clippy, native guards, dashboard syntax and
ARMv7 / glibc <= 2.28 verification.

Further work exists on
[`feature/wled-protocols-c5-readonly-20261017`](https://github.com/angelcode1/lg-wled-sync/tree/012d73eb0052480123444ddcc6969a309b974514)
at `012d73eb0052480123444ddcc6969a309b974514`.
Its [CI run](https://github.com/angelcode1/lg-wled-sync/actions/runs/39190577350)
also passed the host and ARMv7 jobs. It adds DDP / RGBW output, an opt-in BLENDED
backend and Stage 11/12 capture-contract audits. Those changes have not been
merged or deployed by this inspection.

The latest relevant
[Stage 12 audit](https://github.com/angelcode1/lg-wled-sync/blob/012d73eb0052480123444ddcc6969a309b974514/docs/C5-STAGE12-EXACT-CAPTURE-CONTRACT-AUDIT.md)
requires instruction-level review of exact owner-held C5 files. Earlier archives
named `c5-image-stack-20261017-223113.zip` and
`c5-capture-deep-20261017-230452.zip` were not found in the current file search.
Reuse them if available locally; otherwise the bounded collection below obtains
the five native candidates recognized by the existing Stage 12 scanner.

## Collect on the Mac

Prerequisites: Python 3, SSH access already configured, and remote `tar` and
`sha256sum`. Replace the placeholder address. Run the complete block in Terminal.
If a requested file is missing or its hash changes, collection stops rather than
producing an apparently complete bundle.

```bash
(
set -eu
TV_IP='YOUR_TV_IP'
mkdir -p "$HOME/Downloads"
OUT="$(mktemp -d "$HOME/Downloads/c5-native-analysis-XXXXXX")"
FILES='usr/sbin/captureservice usr/lib/libvtcapture.so.1.0.0 usr/lib/libhalgal.so.2.0.1 usr/sbin/lightingcontroller usr/sbin/livepick'

ssh -T "root@$TV_IP" "cd / && sha256sum $FILES" > "$OUT/TV-SHA256SUMS"
ssh -T "root@$TV_IP" "cd / && tar -cf - $FILES" > "$OUT/native.tar"

python3 - "$OUT" <<'PY'
import hashlib
import io
import pathlib
import sys
import tarfile
import zipfile

root = pathlib.Path(sys.argv[1])
expected = {
    "usr/sbin/captureservice",
    "usr/lib/libvtcapture.so.1.0.0",
    "usr/lib/libhalgal.so.2.0.1",
    "usr/sbin/lightingcontroller",
    "usr/sbin/livepick",
}
checksums = {}
for line in (root / "TV-SHA256SUMS").read_text().splitlines():
    digest, name = line.split(None, 1)
    name = name.lstrip("*")
    if name in checksums or len(digest) != 64:
        raise SystemExit("Invalid or duplicate remote checksum")
    checksums[name] = digest
if set(checksums) != expected:
    raise SystemExit("Remote checksum list is incomplete or unexpected")
archive = root / "native.tar"
if not 0 < archive.stat().st_size <= 100 * 1024 * 1024:
    raise SystemExit("Archive exceeds collection budget")
files = {}
with tarfile.open(fileobj=io.BytesIO(archive.read_bytes()), mode="r:") as source:
    for item in source:
        if item.name not in expected or item.name in files or not item.isfile():
            raise SystemExit("Unexpected, duplicate or nonregular archive member")
        if not 4 <= item.size <= 30 * 1024 * 1024:
            raise SystemExit("Native file exceeds collection budget")
        data = source.extractfile(item).read()
        if not data.startswith(b"\x7fELF"):
            raise SystemExit("Native candidate is not ELF")
        if hashlib.sha256(data).hexdigest() != checksums[item.name]:
            raise SystemExit("Hash mismatch; collect again from a stable system")
        files[item.name] = data
if set(files) != expected or sum(map(len, files.values())) > 85 * 1024 * 1024:
    raise SystemExit("Incomplete or oversized native file set")
destination = root / "c5-native-analysis.zip"
with zipfile.ZipFile(destination, "x", zipfile.ZIP_DEFLATED) as target:
    for name, data in sorted(files.items()):
        target.writestr(name, data)
    target.write(root / "TV-SHA256SUMS", "TV-SHA256SUMS")
archive.unlink()
print("Verified five ELF files against TV hashes.")
print(destination)
PY
)
```

The ZIP contains proprietary vendor binaries. Keep it private and out of the
public repository. It contains no paired `config.json`, screenshots, process
memory, full device-information JSON or credentials.

## Run the existing offline scanner

Download the scanner from its pinned research commit; no git or gh is required.
Run in the directory containing the ZIP, or replace the ZIP argument with its
full path.

```bash
curl -fL --retry 3 \
  https://raw.githubusercontent.com/angelcode1/lg-wled-sync/012d73eb0052480123444ddcc6969a309b974514/scripts/c5_stage12_elf_contract_inventory.py \
  -o c5_stage12_elf_contract_inventory.py
python3 c5_stage12_elf_contract_inventory.py --selftest
python3 c5_stage12_elf_contract_inventory.py \
  --archive c5-native-analysis.zip > c5-native-contract-inventory.json
```

The self-test and Stage 11 source-contract audit passed in the current inspection.
The scanner reports static hints; its output cannot prove a usable timestamp,
buffer lease, permission grant or capture rate.

## Next analysis

1. Compare each ELF SHA-256 with the previous audited build, especially
   `libvtcapture.so.1.0.0` hash
   `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`.
2. Trace capture-service response construction for real source timestamps,
   frame sequence values, their clock origins and forwarding to clients.
3. Trace native buffer acquire/return and fence/synchronization lifetimes.
   A function named `release` or a timestamp string alone is insufficient.
4. Separate HAL_GAL thread affinity and UI scope from video-plane evidence.
5. Retain the exact-C5 raw-pointer block and protected/unknown fallback policy.
   The recorded C5 histogram initializer is a stub; exports alone do not make
   it a working capture provider.

If native files changed, establish their new ABI before using old conclusions.
Collection and offline analysis do not establish deployment readiness.

