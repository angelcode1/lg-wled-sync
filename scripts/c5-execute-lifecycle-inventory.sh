#!/bin/sh
# LG webOS 6+ capture-service execute API: READ-ONLY policy + runtime inventory.
# No capture requests, bus name registration, role changes, or process signals.
# Run: ssh root@TV 'sh -s' < scripts/c5-execute-lifecycle-inventory.sh
set -u
BASE=/usr/share/luna-service2
echo "C5_EXECUTE_INVENTORY_BEGIN"
date 2>/dev/null || true
echo "===== CAPTURE SERVICE BINARY ====="
for p in /usr/sbin/captureservice /usr/sbin/lightingcontroller; do
    if [ -r "$p" ]; then
        ls -l "$p" 2>/dev/null
        sha256sum "$p" 2>/dev/null || true
    else
        echo "ABSENT=$p"
    fi
done

echo "===== CAPTURE API AND ROLE POLICY (EXCERPTS ONLY) ====="
for f in \
 "$BASE/groups.d/com.webos.service.capture.groups.json" \
 "$BASE/api-permissions.d/com.webos.service.capture.api.json" \
 "$BASE/client-permissions.d/com.webos.service.capture.perm.json" \
 "$BASE/roles.d/com.webos.service.capture.role.json" \
 "$BASE/roles.d/com.webos.service.lightingcontroller.role.json" \
 "$BASE/roles.d/com.webos.service.livepick.role.json" \
 "$BASE/manifests.d/screen-capture-webos.manifest.json" \
 "$BASE/services.d/com.webos.service.capture.service"
do
    if [ -r "$f" ]; then
        echo "FILE=$f"
        grep -n -E 'capture\.client|capturepermission|execute|createHandle|destroyHandle|setOutput|setOptions|setProperties|capture[\"/.:*]|allowedNames|outbound|groups|services|captur' "$f" 2>/dev/null | head -55 || true
    else
        echo "ABSENT=$f"
    fi
done

echo "===== REGISTERED CLIENT EXECUTABLE ROLES MATCHING OUR BINARY ====="
for dir in "$BASE/roles.d" "$BASE/client-permissions.d"; do
    [ -d "$dir" ] || continue
    grep -il -E 'lg.hue.sync|lg-wled-sync|/tmp/lg-hue-sync-blended-test|/usr/bin/luna-send|/bin/luna-send' "$dir"/* 2>/dev/null | head -20 || true
done

echo "===== CAPTURE SERVICE PROCESS / HANDLES (READ-ONLY) ====="
for p in /proc/[0-9]*/comm; do
    [ -r "$p" ] || continue
    read -r name < "$p" || continue
    case "$name" in
        captureservice|lightingcontroller)
            pid=$(printf '%s' "$p" | cut -d/ -f3)
            echo "PID=$pid COMM=$name"
            for fd in /proc/"$pid"/fd/*; do
                device=$(readlink "$fd" 2>/dev/null) || continue
                case "$device" in
                    /dev/video*|/dev/fb*|/dev/dri/*)
                        echo "FD=$(basename "$fd") DEVICE=$device"
                        ;;
                esac
            done
            ;;
    esac
done

echo "===== BUILT-IN PERSISTENT LUNA CLIENT AVAILABILITY ====="
if command -v node >/dev/null 2>&1; then
    echo "NODE=$(command -v node)"
    node -e 'try { console.log("PALMBUS_MODULE="+require.resolve("palmbus")); } catch(e) { console.log("PALMBUS_MODULE=UNAVAILABLE"); }' 2>&1 | head -3
else
    echo "NODE=UNAVAILABLE"
fi
if command -v luna-send >/dev/null 2>&1; then
    echo "LUNA_SEND=$(command -v luna-send)"
else
    echo "LUNA_SEND=UNAVAILABLE"
fi
echo "C5_EXECUTE_INVENTORY_DONE"
