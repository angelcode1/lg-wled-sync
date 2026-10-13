#!/bin/sh
# LG C5 v4l2_vt built-in provenance: read-only sysfs, kernel config and names.
# No device opens, ioctls, module loads, debugfs, sysctl or kernel memory dumps.
set -eu

SYS=/sys
PROC=/proc
NODE=$SYS/class/video4linux/video60
DRIVER=$SYS/bus/platform/drivers/v4l2_vt
KVER=$(uname -r)

echo '===== C5 V4L2_VT BUILTIN PROVENANCE ====='
printf 'kernel=%s architecture=' "$KVER"
uname -m
echo 'device_open_calls=0 ioctl_calls=0 pixel_reads=0 kernel_changes=0'

if [ ! -d "$NODE" ] || [ "$(cat "$NODE/dev" 2>/dev/null || true)" != '81:7' ] ||
   [ "$(cat "$NODE/name" 2>/dev/null || true)" != 'vt-capture-dev' ]; then
    echo 'ABORT: VERIFIED_VIDEO60_IDENTITY_MISMATCH'
    exit 1
fi
if ! grep -qx 'DRIVER=v4l2_vt' "$NODE/device/uevent"; then
    echo 'ABORT: VERIFIED_V4L2_VT_DRIVER_MISMATCH'
    exit 1
fi

echo '===== SYSFS DRIVER PROVENANCE ====='
for path in "$DRIVER" "$DRIVER/module" "$SYS/module/v4l2_vt" \
            "$SYS/bus/platform/devices/v4l2_vt" \
            "$SYS/bus/platform/devices/v4l2_vt/of_node"; do
    if [ -e "$path" ] || [ -L "$path" ]; then
        ls -ld "$path"
        if [ -L "$path" ]; then
            readlink "$path" || true
        fi
    else
        printf 'NOT_PRESENT: %s\n' "$path"
    fi
done

echo '===== KERNEL BUILD CONFIGURATION ====='
found=0
if [ -r "$PROC/config.gz" ] && command -v zcat >/dev/null 2>&1; then
    echo 'CONFIG_SOURCE=/proc/config.gz'
    found=1
    zcat "$PROC/config.gz" | grep -E '^(CONFIG_(MODULES|IKCONFIG|IKCONFIG_PROC|KALLSYMS|MEDIA_SUPPORT|VIDEO_DEV|V4L2_MEM2MEM_DEV|VIDEO_V4L2)|CONFIG_[A-Za-z0-9_]*(LG_VT|V4L2_VT|VT_CAPTURE|VT_CAP)[A-Za-z0-9_]*=|# CONFIG_[A-Za-z0-9_]*(LG_VT|V4L2_VT|VT_CAPTURE|VT_CAP)[A-Za-z0-9_]* is not set)' || true
fi
for path in "/boot/config-$KVER" "/lib/modules/$KVER/build/.config"; do
    if [ "$found" -eq 0 ] && [ -r "$path" ]; then
        printf 'CONFIG_SOURCE=%s\n' "$path"
        found=1
        grep -E '^(CONFIG_(MODULES|IKCONFIG|IKCONFIG_PROC|KALLSYMS|MEDIA_SUPPORT|VIDEO_DEV|V4L2_MEM2MEM_DEV|VIDEO_V4L2)|CONFIG_[A-Za-z0-9_]*(LG_VT|V4L2_VT|VT_CAPTURE|VT_CAP)[A-Za-z0-9_]*=|# CONFIG_[A-Za-z0-9_]*(LG_VT|V4L2_VT|VT_CAPTURE|VT_CAP)[A-Za-z0-9_]* is not set)' "$path" || true
    fi
done
[ "$found" -eq 1 ] || echo 'KERNEL_CONFIG_NOT_EXPOSED'

echo '===== MATCHING KERNEL SYMBOL NAMES ONLY ====='
if [ -r "$PROC/kallsyms" ]; then
    # Names only: never output kernel pointers or addresses.
    awk '($3 ~ /(v4l2_vt|vt_capture|vtcapture|vt60)/) {
        if (!seen[$3]++) {
            print $3
            count++
        }
        if (count >= 80) exit
    }' "$PROC/kallsyms"
else
    echo 'KALLSYMS_UNAVAILABLE'
fi
echo '===== BUILTIN PROVENANCE COMPLETE ====='
