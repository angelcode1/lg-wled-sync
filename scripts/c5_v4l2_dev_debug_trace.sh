#!/bin/sh
# C5 LG VT node diagnostic: temporarily log V4L2 ioctl metadata during the
# previously proven native capture lifecycle. No independent capture logic.
set -eu

SYS=/sys/class/video4linux/video60
NODE=/dev/video60
DBG=$SYS/dev_debug
MARKER=/var/luna/preferences/vt_devmode
BIN=/var/home/root/c5-histogram-probe
PREVIOUS=/var/home/root/c5-frame-initonly/lg-hue-sync.59ec38e
TRACE_SECONDS=30
TAG="C5VTTRACE$(date +%s)_$$"
ORIG=
TRACE_ACTIVE=0
TMP_OUT=

restore_debug() {
    if [ "$TRACE_ACTIVE" -eq 1 ]; then
        if printf '%s\n' "$ORIG" > "$DBG"; then
            TRACE_ACTIVE=0
        else
            echo 'CRITICAL: COULD_NOT_RESTORE_DEV_DEBUG (restore manually)' >&2
        fi
    fi
}
cleanup() {
    restore_debug
    if [ -n "$TMP_OUT" ]; then
        rm -f "$TMP_OUT"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

echo '===== V4L2 TRACE PREFLIGHT ====='
uname -m
uname -r
[ -x "$BIN" ] || { echo 'ABORT: KNOWN_GOOD_CAPTURE_BINARY_MISSING'; exit 1; }
[ -f "$PREVIOUS" ] || { echo 'ABORT: KNOWN_GOOD_BINARY_REFERENCE_MISSING'; exit 1; }
cmp "$BIN" "$PREVIOUS" || { echo 'ABORT: CAPTURE_BINARY_DIFFERS_FROM_59ec38e'; exit 1; }
[ -c "$NODE" ] || { echo 'ABORT: VIDEO60_NOT_CHAR_DEVICE'; exit 1; }
[ ! -e "$MARKER" ] || { echo 'ABORT: VT_MARKER_PRESENT'; exit 1; }
[ "$(cat "$SYS/dev")" = '81:7' ] || { echo 'ABORT: DEVICE_IDENTITY_MISMATCH'; exit 1; }
[ "$(cat "$SYS/name")" = 'vt-capture-dev' ] || { echo 'ABORT: DEVICE_NAME_MISMATCH'; exit 1; }
grep -qx 'DRIVER=v4l2_vt' "$SYS/device/uevent" || { echo 'ABORT: DRIVER_MISMATCH'; exit 1; }
[ -f "$DBG" ] && [ -r "$DBG" ] && [ -w "$DBG" ] || { echo 'ABORT: DEV_DEBUG_UNAVAILABLE'; exit 1; }
[ -w /dev/kmsg ] || { echo 'ABORT: KMSG_UNAVAILABLE'; exit 1; }
for tool in cmp timeout fuser dmesg sed grep mktemp; do
    command -v "$tool" >/dev/null 2>&1 || { echo "ABORT: TOOL_MISSING_$tool"; exit 1; }
done
if [ -n "$(fuser "$NODE" 2>/dev/null || true)" ]; then
    echo 'ABORT: VIDEO60_OWNER_PRESENT'; exit 1
fi
if command -v pidof >/dev/null 2>&1 && pidof lg-hue-sync >/dev/null 2>&1; then
    echo 'ABORT: EXISTING_LG_HUE_SYNC_PROCESS'; exit 1
fi
ORIG=$(cat "$DBG")
case "$ORIG" in ''|*[!0-9]*) echo 'ABORT: NONNUMERIC_DEV_DEBUG_STATE'; exit 1;; esac
printf 'DEV_DEBUG_ORIG=%s\n' "$ORIG"
printf '%s\n' 'dev_debug=7: IOCTL + IOCTL_ARG + file open/release/mmap (no STREAMING/POLL bits)'
printf 'CAPTURE_CMD=%s %s %s\n' "$BIN" test-frame-vma-flags --acknowledge-native-capture
printf 'TRACE_TIMEOUT_SECONDS=%s\n' "$TRACE_SECONDS"
TMP_OUT=$(mktemp /tmp/c5-v4l2-trace.XXXXXX)

printf '%s BEGIN\n' "$TAG" > /dev/kmsg
TRACE_ACTIVE=1
printf '7\n' > "$DBG"
echo '===== TRACED KNOWN-GOOD NATIVE CAPTURE ====='
set +e
timeout "$TRACE_SECONDS" "$BIN" test-frame-vma-flags --acknowledge-native-capture > "$TMP_OUT" 2>&1
RC=$?
set -e
restore_debug
printf '%s END\n' "$TAG" > /dev/kmsg
printf 'CAPTURE_EXIT=%s\n' "$RC"
printf 'DEV_DEBUG_NOW=%s\n' "$(cat "$DBG")"
echo '===== CAPTURE RESULT (LAST 25 LINES) ====='
tail -n 25 "$TMP_OUT"
echo '===== VIDEO60 KERNEL TRACE ====='
# Filter to this bounded marker window; never clear the kernel ring buffer.
dmesg | sed -n "/$TAG BEGIN/,/$TAG END/p" | grep -E "video60|$TAG" | head -n 240 || true
echo '===== KERNEL MARKERS / COUNTS ====='
dmesg | grep -F "$TAG BEGIN" | tail -n 1 || true
dmesg | grep -F "$TAG END" | tail -n 1 || true
printf 'TRACE_LINES=%s\n' "$(dmesg | sed -n "/$TAG BEGIN/,/$TAG END/p" | grep -c 'video60' || true)"
echo '===== POST-TEST STATE ====='
[ -e "$MARKER" ] && echo VT_MARKER_PRESENT || echo VT_MARKER_ABSENT
[ -n "$(fuser "$NODE" 2>/dev/null || true)" ] && echo VIDEO60_OWNER_PRESENT || echo VIDEO60_NO_OWNER_DETECTED
[ "$RC" -eq 0 ] || { echo 'CAPTURE_FAILED: no other native capture attempt'; exit 1; }
[ "$(cat "$DBG")" = "$ORIG" ] || { echo 'ERROR: DEV_DEBUG_NOT_RESTORED'; exit 1; }
echo 'C5_V4L2_TRACE_COMPLETE'
