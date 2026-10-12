#!/usr/bin/env bash
# Read-only C5 evidence collection, plus explicit launch of installed wizard.
set -euo pipefail
TV_HOST="${1:-${TV_HOST:-}}"
MODE="${2:-preflight}"
SSH_PORT="${SSH_PORT:-22}"
TV_USER="${TV_USER:-root}"
OUT="${OUT:-$PWD/c5-evidence}"
if [[ -z "$TV_HOST" || ! "$TV_HOST" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo 'Usage: scripts/c5_profile.sh TV_IP {preflight|record|launch|fetch}' >&2
  exit 2
fi
mkdir -p "$OUT"
SSH=(ssh -o ConnectTimeout=10 -p "$SSH_PORT" "$TV_USER@$TV_HOST")
case "$MODE" in
preflight)
  "${SSH[@]}" 'sh -s' > "$OUT/preflight.txt" <<'REMOTE'
printf '\n=== identity ===\n'; id; uname -a
printf '\n=== model ===\n'
luna-send -n 1 luna://com.webos.service.tv.systemproperty/getSystemInfo '{"keys":["modelName"]}' 2>&1 || true
printf '\n=== media pipelines ===\n'
luna-send -n 1 luna://com.webos.media/getActivePipelines '{}' 2>&1 || true
printf '\n=== glibc ===\n'; getconf GNU_LIBC_VERSION 2>&1 || true
printf '\n=== LG capture symbols ===\n'
ls -l /usr/lib/libvtcapture.so* 2>&1 || true
(readelf -Ws /usr/lib/libvtcapture.so.1 2>/dev/null || true) | grep -E 'vtCapture_(initHistogram|setPresetHistogram|getHistogram|finalizeHistogram)' || true
printf '\n=== competing capture processes ===\n'
ps | grep -E 'lg-hue-sync|lg-wled-sync|histogram-service|piccap|hyperion' | grep -v grep || true
printf '\n=== HyperDRM status ===\n'
luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}' 2>&1 || true
printf '\n=== saved profile path ===\n'
ls -l /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json 2>&1 || true
REMOTE
  echo "Saved $OUT/preflight.txt"
  ;;
record)
  "${SSH[@]}" 'sh -s' > "$OUT/region-observations.log" <<'REMOTE'
i=0
while [ "$i" -lt 360 ]; do
  printf 'SAMPLE %s\n' "$(date +%s 2>/dev/null || echo 0)"
  luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}' 2>&1
  printf '\nEND_SAMPLE\n'
  i=$((i+1))
  sleep 0.25
done
REMOTE
  echo "Saved $OUT/region-observations.log"
  ;;
launch)
  "${SSH[@]}" "luna-send -n 1 luna://com.webos.applicationManager/launch '{\"id\":\"org.webosbrew.histogramhyperion\",\"params\":{\"action\":\"calibrate\"}}'" | tee "$OUT/launch.txt"
  echo 'Use TV remote to choose Calibration > Start Auto Calibration if needed.'
  ;;
fetch)
  "${SSH[@]}" 'cat /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service/calibration.json' > "$OUT/saved-calibration.json"
  "${SSH[@]}" "luna-send -n 1 luna://org.webosbrew.histogramhyperion.service/status '{}'" > "$OUT/service-status.txt" || true
  echo "Saved $OUT/saved-calibration.json and $OUT/service-status.txt"
  ;;
*) echo "Unknown mode: $MODE" >&2; exit 2 ;;
esac
