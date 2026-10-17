#!/bin/sh
# Read-only C5 capture inventory: no device opens, ioctls, driver calls or configuration changes.
set -u
umask 077
section() { printf '\n===== %s =====\n' "$1"; }
section 'kernel and ABI'
uname -srm; getconf GNU_LIBC_VERSION; getconf LONG_BIT
section 'capture device metadata'
for p in /sys/class/video4linux/video60/name /sys/class/video4linux/video60/dev /sys/class/video4linux/video60/device/modalias; do
  if [ -r "$p" ]; then printf '%s: ' "$p"; head -c 200 "$p"; printf '\n'; fi
done
ls -ld /dev/video60 /usr/lib/libvtcapture.so.1 /usr/lib/libvtcapture.so.1.0.0 /usr/lib/libdile_vt.so.0 2>&1 || true
section 'native library fingerprint'
sha256sum /usr/lib/libvtcapture.so.1.0.0 2>&1 || true
readlink -f /usr/lib/libvtcapture.so.1 2>&1 || true
section 'selected native exports (if readelf installed)'
if command -v readelf >/dev/null 2>&1; then
  readelf -Ws /usr/lib/libvtcapture.so.1 2>/dev/null | grep -E 'vtCapture_(init|preprocess|currentCaptureBuffInfo|planeInfo|initHistogram|getHistogram|finalizeHistogram)' || true
else echo 'readelf unavailable'; fi
section 'existing capture process hints'
ps 2>/dev/null | grep -Ei 'piccap|hyperion|hyperhdr|hyperdrm|lg-hue-sync|lg-wled-sync' | grep -v grep || true
command -v fuser >/dev/null 2>&1 && fuser /dev/video60 2>&1 || true
section 'capture marker presence'
if [ -e /var/luna/preferences/vt_devmode ]; then ls -l /var/luna/preferences/vt_devmode; else echo ABSENT; fi
section 'existing daemon state'
command -v systemctl >/dev/null 2>&1 && systemctl is-active lg-hue-sync 2>&1 || true
section 'media-device class names'
for p in /sys/class/video4linux/video*/name; do
  [ -r "$p" ] || continue
  printf '%s: ' "$p"; head -c 100 "$p"; printf '\n'
done
section 'Luna tools'
command -v luna-send || true
command -v luna-send-pub || true
echo 'DONE: metadata only; no media pixels, HDCP state, capture sessions or protected video accessed'
