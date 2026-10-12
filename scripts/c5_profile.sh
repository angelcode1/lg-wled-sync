#!/usr/bin/env bash
# LG C5: AArch64 kernel, 32-bit ARM application userspace.
# Read-only evidence collection, plus explicit launch of installed wizard.
set -euo pipefail
TV_HOST="${1:-${TV_HOST:-}}"
MODE="${2:-preflight}"
SSH_PORT="${SSH_PORT:-22}"
TV_USER="${TV_USER:-root}"
OUT="${OUT:-$PWD/c5-evidence}"
if [[ -z "$TV_HOST" || ! "$TV_HOST" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo 'Usage: scripts/c5_profile.sh TV_IP {preflight|pull-lib|record|launch|fetch}' >&2
  exit 2
fi
mkdir -p "$OUT"
SSH=(ssh -o ConnectTimeout=10 -p "$SSH_PORT" "$TV_USER@$TV_HOST")
case "$MODE" in
preflight)
  "${SSH[@]}" 'sh -s' > "$OUT/preflight.txt" <<'REMOTE'
printf '\n=== kernel versus userspace (distinct architectures) ===\n'
id; uname -a
printf 'kernel machine: '; uname -m
printf 'shell/userspace bitness: '; getconf LONG_BIT 2>&1 || true
printf 'glibc: '; getconf GNU_LIBC_VERSION 2>&1 || true
printf 'file utility: '; command -v file || echo '(not installed)'
printf 'readelf utility: '; command -v readelf || echo '(not installed)'
if command -v file >/dev/null 2>&1; then
  file -L /bin/sh /usr/lib/libvtcapture.so.1.0.0 2>&1 || true
fi
if command -v readelf >/dev/null 2>&1; then
  readelf -h /usr/lib/libvtcapture.so.1.0.0 2>&1 | grep -E 'Class:|Machine:|OS/ABI:|Flags:' || true
fi
printf '\n=== Luna availability ===\n'
command -v luna-send || echo 'luna-send not found in SSH PATH'
printf '\n=== model ===\n'
luna-send -n 1 -f -w 2000 luna://com.webos.service.tv.systemproperty/getSystemInfo '{"keys":["modelName"]}' 2>&1
printf 'model Luna exit code: %s\n' "$?"
echo 'NOTE: exit 0 with no JSON is still an unanswered Luna request'
printf '\n=== media pipelines ===\n'
luna-send -n 1 -f -w 2000 luna://com.webos.media/getActivePipelines '{}' 2>&1
printf 'pipeline Luna exit code: %s\n' "$?"
echo 'NOTE: empty reply is Unknown playback classification'
printf '\n=== LG capture symbols (absence is NOT proven if readelf missing) ===\n'
ls -l /usr/lib/libvtcapture.so* 2>&1 || true
if command -v readelf >/dev/null 2>&1; then
  readelf -Ws /usr/lib/libvtcapture.so.1.0.0 2>&1 | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)' || echo 'No matching symbol names found (inspect complete dynamic table offline)'
else
  echo 'READ_ELF_MISSING: use pull-lib mode for offline analysis on Mac'
fi
printf '\n=== competing capture processes ===\n'
ps | grep -E 'lg-hue-sync|lg-wled-sync|histogram-service|piccap|hyperion' | grep -v grep || true
printf '\n=== HyperDRM installation candidates ===\n'
for dir in /media/developer/apps/usr/palm/applications /media/developer/apps/usr/palm/services; do
  if [ -d "$dir" ]; then
    ls "$dir" 2>&1 | grep -iE 'histogram|hyperdrm' || echo "(no matching entries under $dir)"
  else
    echo "$dir is absent"
  fi
done
printf '\n=== HyperDRM status ===\n'
luna-send -n 1 -f -w 2000 luna://org.webosbrew.histogramhyperion.service/status '{}' 2>&1
printf 'HyperDRM status Luna exit code: %s\n' "$?"
printf '\n=== saved profile path ===\n'
ls -l /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json 2>&1 || true
REMOTE
  echo "Saved $OUT/preflight.txt"
  ;;
pull-lib)
  # Copy the existing native library without modifying the television.
  part="$OUT/libvtcapture.so.1.0.0.partial"
  if "${SSH[@]}" 'cat /usr/lib/libvtcapture.so.1.0.0' > "$part" && [[ -s "$part" ]]; then
    mv "$part" "$OUT/libvtcapture.so.1.0.0"
  else
    rm -f "$part"
    echo 'Unable to read libvtcapture; no library copy retained' >&2
    exit 1
  fi
  echo "Saved $OUT/libvtcapture.so.1.0.0"
  if command -v file >/dev/null 2>&1; then
    file "$OUT/libvtcapture.so.1.0.0"
  fi
  if command -v greadelf >/dev/null 2>&1; then
    greadelf -h "$OUT/libvtcapture.so.1.0.0"
    greadelf --dyn-syms -W "$OUT/libvtcapture.so.1.0.0" | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)' || true
  elif command -v llvm-readelf >/dev/null 2>&1; then
    llvm-readelf -h "$OUT/libvtcapture.so.1.0.0"
    llvm-readelf --dyn-syms "$OUT/libvtcapture.so.1.0.0" | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)' || true
  else
    echo 'Install binutils on Mac (brew install binutils) for greadelf symbol inspection.'
  fi
  ;;
record)
  # No data exists until the app and service are installed. Fail before a
  # pointless 90-second capture, without touching TV processes or services.
  if ! "${SSH[@]}" 'test -d /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service'; then
    echo 'HyperDRM service is not installed: cannot record regions.' >&2
    exit 3
  fi
  "${SSH[@]}" 'sh -s' > "$OUT/region-observations.log" <<'REMOTE'
i=0
while [ "$i" -lt 360 ]; do
  printf 'SAMPLE %s\n' "$(date +%s 2>/dev/null || echo 0)"
  luna-send -n 1 -f -w 2000 luna://org.webosbrew.histogramhyperion.service/status '{}' 2>&1
  printf '\nEND_SAMPLE\n'
  i=$((i+1))
  sleep 0.25
done
REMOTE
  echo "Saved $OUT/region-observations.log"
  ;;
launch)
  if ! "${SSH[@]}" 'test -d /media/developer/apps/usr/palm/applications/org.webosbrew.histogramhyperion'; then
    echo 'HyperDRM app is not installed: cannot launch its calibration wizard.' >&2
    exit 3
  fi
  "${SSH[@]}" "luna-send -n 1 -f -w 2000 luna://com.webos.applicationManager/launch '{\"id\":\"org.webosbrew.histogramhyperion\",\"params\":{\"action\":\"calibrate\"}}'" | tee "$OUT/launch.txt"
  echo 'Use TV remote to choose Calibration > Start Auto Calibration if needed.'
  ;;
fetch)
  if ! "${SSH[@]}" 'test -s /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json'; then
    echo 'Saved HyperDRM calibration does not exist; nothing to fetch.' >&2
    exit 3
  fi
  "${SSH[@]}" 'cat /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json' > "$OUT/saved-calibration.json"
  "${SSH[@]}" "luna-send -n 1 -f -w 2000 luna://org.webosbrew.histogramhyperion.service/status '{}'" > "$OUT/service-status.txt" || true
  echo "Saved $OUT/saved-calibration.json and $OUT/service-status.txt"
  ;;
*) echo "Unknown mode: $MODE" >&2; exit 2 ;;
esac
