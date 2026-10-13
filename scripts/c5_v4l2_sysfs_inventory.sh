#!/bin/sh
# LG C5 /dev/video60 topology discovery. Reads sysfs and filesystem metadata
# only; deliberately never opens a V4L2 device or calls any ioctl.
set -eu

SYSFS_ROOT=${C5_V4L2_SYSFS_ROOT:-/sys/class/video4linux/video60}
DEVICE=${C5_V4L2_DEVICE:-/dev/video60}
MARKER=${C5_V4L2_MARKER:-/var/luna/preferences/vt_devmode}

printf '%s\n' '===== C5 V4L2 READ-ONLY INVENTORY ====='
printf 'device=%s\n' "$DEVICE"
printf 'sysfs=%s\n' "$SYSFS_ROOT"
printf 'uname='; uname -r
printf '%s\n' 'device_open_calls=0 ioctls=0 stream_actions=0 pixel_reads=0'

if [ -e "$MARKER" ]; then
  echo 'VT_MARKER_PRESENT (no action taken)'
else
  echo 'VT_MARKER_ABSENT'
fi

if [ -c "$DEVICE" ]; then
  printf '%s\n' 'VIDEO60_CHAR_DEVICE_PRESENT'
  ls -ld "$DEVICE"
elif [ -e "$DEVICE" ]; then
  printf '%s\n' 'VIDEO60_EXISTS_BUT_NOT_CHAR_DEVICE'
else
  printf '%s\n' 'VIDEO60_DEVICE_ABSENT'
fi

if [ ! -d "$SYSFS_ROOT" ]; then
  printf '%s\n' 'VIDEO60_SYSFS_UNAVAILABLE'
  exit 0
fi

printf '%s\n' '===== DEVICE SYSFS METADATA ====='
for relative in dev name uevent device/uevent device/modalias; do
  path="$SYSFS_ROOT/$relative"
  if [ -f "$path" ] && [ -r "$path" ]; then
    printf '%s\n' "----- $relative -----"
    cat "$path"
  fi
done

printf '%s\n' '===== SYSFS LINKS ====='
for relative in . device device/driver device/driver/module; do
  path="$SYSFS_ROOT/$relative"
  if [ -e "$path" ] || [ -L "$path" ]; then
    ls -ld "$path"
    if command -v readlink >/dev/null 2>&1; then
      printf 'link_target(%s)=' "$relative"
      readlink "$path" || true
    fi
  fi
done

printf '%s\n' '===== CAPTURE OWNERSHIP (BEST EFFORT) ====='
if command -v fuser >/dev/null 2>&1 && [ -c "$DEVICE" ]; then
  owners=$(fuser "$DEVICE" 2>/dev/null || true)
  if [ -n "$owners" ]; then
    printf 'VIDEO60_OWNER_DETECTED: %s\n' "$owners"
  else
    printf '%s\n' 'VIDEO60_NO_OWNER_DETECTED_BY_FUSER'
  fi
else
  printf '%s\n' 'VIDEO60_OWNER_UNKNOWN (fuser unavailable or device absent)'
fi
printf '%s\n' 'C5_V4L2_SYSFS_INVENTORY_COMPLETE'
