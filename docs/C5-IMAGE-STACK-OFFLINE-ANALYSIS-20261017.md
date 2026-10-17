# LG C5 image stack: offline evidence review (2026-10-17)

Evidence: privately supplied `c5-image-stack-20261017-223113.zip` (61 MB). The archive contains selected native TV files and `runtime.txt`; it does not contain /etc service roles, owned frame pixels, or hardware performance measurements. Do **not** check vendor libraries or private process maps into this repository.

## Observed platform

- Kernel: Linux 5.4.268-324, aarch64 (2024-09-30 build); userspace includes **ELF32 ARM EABI5** libraries. Runtime `getconf GNU_LIBC_VERSION` reports **glibc 2.35**.
- `/dev/video60` is named `vt-capture-dev` and owned `root:composit` with `0660` permissions.
- `/dev/video70` is named `gps-capture-dev`, also `root:composit` with `0660`. This name alone does **not** establish a safe or usable capture capability. No direct `/dev/video70` consumer is identified by the uploaded capture-specific ELF dependencies; the generic GStreamer v4l2 plugin contains a reference.
- `/dev/video240` is present as `dolby-metadata-dev`, which does not imply any raw-image access.
- Live process maps include `/usr/lib/libvtcapture.so.1.0.0`, `/usr/lib/libhalgal.so.2.0.1`, `/usr/lib/libgm-wayland.so.1.0.0`, `/usr/sbin/captureservice`, and the surface manager / compositor. The aggregated maps do not identify PID-to-library ownership; a per-PID attribution needs separate read-only collection.

## Capture paths recovered

| Component | Native evidence | Relevance and limitation |
|---|---|---|
| Video VT | `/usr/lib/libvtcapture.so.1.0.0`, Build ID `9fb5e56d59c29ed242c0e4a66015c71601a4657e`, SHA256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c`; references `/dev/video60`, exports `vtCapture_currentCaptureBuffInfo` | **Exactly the previously guarded vendor build**. Its `VtHistogram::initHistogram()` Thumb function is 4 bytes (`07 20 70 47`): returns constant 7. No new safe CPU buffer lease or histogram function found. |
| Legacy DILE VT | `/usr/lib/libdile_vt.so.0.1.0` exports 4/6-byte stub-like `DILE_VT_*` functions | No proof of a functioning legacy video capture path. Do not enable fallback merely because the file exists. |
| LG capture service | `/usr/sbin/captureservice` imports `HAL_GAL_CaptureFrameBuffer`, links `libvtcapture`, contains `VideoCapture`, `GraphicCapture`, `DisplayCapture`, `blendedCapture`, `execute`, `executeOneShot`, `stop`, `getCapability`; registers `com.webos.service.capture` and a separate permission service | Highest-value alternative **investigation**: LG-managed, permission-checked capture with PNG/JPEG support. No proof of authorized high-rate, full-content, or HDR capture. |
| Built-in compositor screenshots | `/usr/lib/qml/WebOSCompositor/controllers/fullscreen/StarfishFullscreenCapture.qml` builds a request using `method: BLENDED` and calls `com.webos.service.capture/executeOneShot` with output path/format/width/height | **Proves first-party screenshot use of the capture service**. It does not establish that a third-party app is entitled to the same method, that video is included, or that continuous screenshot streaming is viable. |
| UI framebuffer | `/usr/lib/libhalgal.so.2.0.1` exports `HAL_GAL_CaptureFrameBuffer`, `HAL_GAL_GetFrameBufferList`, `HAL_GAL_CaptureFrameBufferEx`; capture service links this library | Candidate separate UI/OSD source when authorized, independently from VT video frames. Requires ABI and content coverage validation. |
| Legacy UI stack | `libgm-wayland.so.1.0.0` exports `GM_*Window`, `GM_GetWindowSurfaceInfo`, bitmap utility | Visible UI surface management; no evidence that it provides unprotected TV video pixels. |
| Qt/Wayland compositor | `libWebOSCoreCompositor.so.0.0.1` has `WebOSScreenShot::take`, `QQuickWindow::grabWindow`, `glReadPixels`; compositor plugin has `StarfishCompositor::prepareScreenCapture` | Screenshots/individual compositor surfaces, not automatically final post-video display frames. Requires cautious read-only entitlement inspection before any integration. |
| Vendor test suite | `/usr/bin/vtCaptureTestSuite` imports VT and HAL_GAL; embedded menu includes one-shot/continuous capture, RGB conversion and histogram commands | **Do not run**: these commands can open capture devices, affect V4L2 ownership, or exercise known-stubbed histogram code. It is useful for offline ABI/string review only. |

The binaries include vendor source-build path strings under `lib32-screen-capture-webos`; this gives class names and operation names but does **not** provide source code or validated ABI.

## Rust implementation priorities

1. **Keep C5 VT guards.** The SHA256 exactly matches the known frame/histogram blocked build. Preserve unknown/protected fail-closed routing, no `DILE_VT` fallback, no unsafe `currentCaptureBuffInfo` CPU pointer copy, and no second queue owner.
2. **Add a feature-gated capture-service metadata adapter only after permission verification.** Start with read-only `getCapability`/service availability; don't invoke `executeOneShot` or capture from Rust automatically. A screenshot file is not an owned, leased video buffer.
3. **Evaluate an authorized UI/OSD backend independently.** `libhalgal` is a more specific UI-path candidate than speculative direct V4L2 reads. No bypassing protected content; validate frame provenance.
4. **Separate source availability from physical capability.** Distinguish VT video, HAL_GAL UI, screenshot status, known-protected/unknown source, and stream latency in diagnostics.
5. **Measure one-shot cadence / capture coverage only after deliberate TV-side approval** using known-unprotected local UI or own test pattern. Don't claim Ambilight FPS from API availability.
6. Maintain the 32-bit ARM toolchain target. Do not build an ARM64 userspace binary solely because `uname -m` says `aarch64`.

## Minimal next read-only evidence

On a Mac (replace the host):

```sh
TV_HOST=192.168.8.X
ssh "root@$TV_HOST" 'luna-send -n 1 -f luna://com.webos.service.capture/getCapability "{}" 2>&1'
ssh "root@$TV_HOST" 'sh -s' <<'SH'
for p in /proc/[0-9]*/maps; do
  grep -q -E '/usr/lib/libvtcapture|/usr/lib/libhalgal|/usr/sbin/captureservice' "$p" 2>/dev/null || continue
  pid=${p#/proc/}
  pid=${pid%/maps}
  printf 'PID=%s COMM=' "$pid"
  cat "/proc/$pid/comm" 2>/dev/null
  grep -E '/usr/lib/libvtcapture|/usr/lib/libhalgal|/usr/sbin/captureservice' "$p" 2>/dev/null | awk '{print $NF}' | sort -u
done
SH
```

A capture service permission rejection is a valid result and must not be worked around by widening roles. The archive does not contain the LS2 service manifests and roles needed to assert entitlement. Record the denied response without mutation.

**No existing binary, service, permissions, capture session, or TV preferences were modified by this offline review.**

## Runtime `getCapability` reply and binary verification

TV response from `luna://com.webos.service.capture/getCapability` with `{}`: `{"returnValue":true}`, with no capability fields. This confirms an accepted response, **not** a successful screenshot, authorized video pixel access, a valid capture path, or a supported real-time mode.

The shipped `/usr/sbin/captureservice` ELF has dynamic symbol `CaptureProperties::processGetCapability(LSHandle*, LSMessage*, void*)` at Thumb address `0x1d65d`, size **4 bytes**. Offline Thumb disassembly of the same user-provided ELF shows a branch at `0x1d65c` directly to `CaptureProperties::processGetClipRegions` at `0x1d530`. The `processGetProperties`, `processGetOptions`, and several related read-only handlers also branch into this same implementation. Therefore an empty success response is not surprising, and repeatedly calling `getCapability` cannot establish what images are actually available.

The ELF contains explicit `permissionClient` and `forceCaptureClient` parsers and references `/etc/palm/whitelist-conf.json` and `/home/root/whitelist-conf.json`. Inspect the existing files **read-only**; do not add clients or override roles. Avoid sharing unrelated configuration or device identifiers from these files.

## C5 live capture whitelist (read-only; 2026-10-17)

The actual `/etc/palm/whitelist-conf.json` contains:

- `permissionClient`: `com.webos.service.capturepermission`, `com.webos.service.legacybroadcast.cipermission`, `com.webos.service.legacybroadcast.dvrpermission`.
- `forceCaptureClient`: **empty**.
- No filename containing `capture`, `compositor`, or `surface` appeared in listings of `/var/luna-service2/{roles.d,manifests.d,client-permissions.d}`.

**Interpretation:** `permissionClient` is used by the capture service's `PermissionData::isValidAppClient` check to gate **permission-management calls**. It must **not** be misinterpreted as the exhaustive list of consumers permitted to take screenshots. `forceCaptureClient` being empty provides no vendor-listed blanket force-capture clients; it says nothing about ordinary authorized per-pipeline captures. File-name matching is also insufficient to rule out role/manifest entries inside generically named files.

The extracted `captureservice` ELF explicitly implements `PermissionManager::processCheckPermission`, `processSetPermission`, `PermissionData::isValidAppClient`, a separate `PermissionService`, `getCurrentPipelineId` and `getCurrentVideoMuteState`. Permission-aware capture may be dependent on **current video pipeline and mute state**. Do not call `setPermission` or add entries to the whitelist.

### Next evidence: metadata only, no capture attempt

Inspect which service registry and manifest files refer to the capture services. Return **only matching filenames** to avoid exposing unrelated service role details:

```sh
ssh root@TV_IP 'sh -s' <<'SH'
echo "===== CAPTURE SERVICE REGISTRY REFERENCES ====="
for d in /var/luna-service2 /usr/share/luna-service2 /etc/palm/services /usr/share/dbus-1/services; do
  [ -d "$d" ] || continue
  grep -RIlE 'com[.]webos[.]service[.]capture(permission)?' "$d" 2>/dev/null | head -80
done
echo "===== CAPTURE SERVICE BINARY / APP PROCESSES ====="
ps | grep -Ei 'captureservice|capturepermission|compositor|surface-manager' | grep -v grep || true
echo "===== CAPTURE SERVICE FILES ====="
find /etc/palm /usr/share/luna-service2 /var/luna-service2 -maxdepth 4 -type f \( -iname '*capture*' -o -iname '*screen*' \) -print 2>/dev/null | head -80
SH
```

The `getCapability` success response is not proof of screenshot access and must not change Rust DRM/capture policy.

## Follow-up C5 Luna registry discovery

The TV's active service declarations are located in **`/usr/share/luna-service2`**, not `/var/luna-service2`; the earlier filename-only query in `/var` did not discover these files. In the live TV output, `captureservice` (PID 4729 at the time) and `surface-manager` (PID 2259) were running.

Present relevant files:

- `api-permissions.d/com.webos.service.capture.api.json` and `api-permissions.d/compat.api.json`
- `client-permissions.d/com.webos.service.capture.perm.json`
- `groups.d/com.webos.service.capture.groups.json`
- `manifests.d/screen-capture-webos.manifest.json`
- `roles.d/com.webos.service.capture.role.json` and `roles.d/com.webos.service.capture.testsuite.role.json`
- `roles.d/com.webos.service.lightingcontroller.role.json` and `roles.d/com.webos.service.livepick.role.json`
- `services.d/com.webos.service.capture.service`

The existence of these files shows **where** registered API groups/client permissions and service roles are defined; **their contents have not been inspected yet**, so it is premature to infer that the Rust daemon is authorized to call one-shot capture. The `lightingcontroller` and `livepick` names are leads only. No permission-management calls, whitelisting, role changes, or image capture have been performed.

## Capture service policy bundle: verified C5 LS2 permissions

On 2026-10-17 the owner supplied all 11 requested configuration files in a private ZIP. These facts are directly verified against parsed JSON from that ZIP:

- `screen-capture-webos.manifest.json` points to its native executable role, test-suite role, API permissions, groups, client permissions and D-Bus service entry.
- `com.webos.service.capture.service` launches `/usr/sbin/captureservice`, providing both `com.webos.service.capture` and `com.webos.service.capturepermission`.
- `com.webos.service.capture.groups.json` declares **two non-public API groups**, `capture.client` and `capturepermission.security`.
- `capture.client` lists **15** methods: `createHandle`, `destroyHandle`, `getClipRegions`, `getOptions`, `getProperties`, `isLocked`, `lock`, `setClipRegions`, `setOptions`, `setOutput`, `setProperties`, `unlock`, `getCapability`, `execute`, `executeOneShot`.
- `capturepermission.security` exclusively covers `com.webos.service.capturepermission/checkPermission` and `setPermission`. **Neither may be called by the Rust daemon** for an ambient-light capture experiment; permission management is not a preflight entitlement workaround.
- `com.webos.service.capture.role.json` identifies the service binary itself and the private names the service may register, not authorization for arbitrary third-party applications.
- `com.webos.service.capture.perm.json` grants the capture service's own names `all` groups; it is **not** a grant to `org.webosbrew.lg-hue-sync` or any process run from a root SSH shell.
- `com.webos.service.lightingcontroller.role.json`: the built-in `/usr/sbin/lightingcontroller` can register `com.webos.service.capture.client*` and has wildcard outbound permission under that name; it also uses the media service. This is a **strong lead** for finding a vendor-provided lighting/video integration, but does not establish that its stream is safe, continuous or usable by another service.
- `com.webos.service.livepick.role.json` lists `com.webos.service.capture` as an outbound target for `com.webos.service.livepick` and `.req`. It is a different privileged consumer, and does not authorize impersonation.
- `com.webos.service.capture.testsuite.role.json` contains broad test-suite privileges. Do not execute the test suite: it can perform real captures and manage capture buffers.
- The large `compat.api.json` lists `com.webos.surfacemanager/captureCompositorOutput` in **private** but has no general public grant for this capture service. API names alone do not establish frame access.

### Implications for our Rust code

1. **No automatic `execute` or `executeOneShot` calls.** Capture service may implement direct video path, and returns only an ACK for `getCapability` on this build. Only a known-authorized clear test source and a legitimate registration role can justify future controlled capture evaluation.
2. **Do not provision wildcard Luna names or privileges.** The historical `scripts/provision_luna.sh` used privileged roles, `all` client permissions and service-name matches including `com.webos.service.capturepermission`; it has now been replaced by a fail-closed warning in the development branch. It was not run against the TV and no TV role was modified.
3. **Lightcontroller is the next static reference.** Inspect its real binary, own manifest, group and client-permission files, service dependency paths, and method usage OFFLINE. Registration identity and permissions cannot be borrowed by our daemon.
4. **Do not infer an authorization grant from `luna-send` as root.** The daemon's actual executable/service identity and effective LS2 client groups must be verified separately. Preserve the current protected/unknown blackout policy.

### Minimal next TV collection (read-only)

Run on Mac; replace `TV_IP`, check the contents before sharing:

```sh
TV=TV_IP
OUT="$HOME/Downloads/c5-lighting-service-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT"
ssh "root@$TV" 'sh -s' <<'SH' | tar -xf - -C "$OUT"
for f in /usr/sbin/lightingcontroller /usr/sbin/livepick; do
    [ -f "$f" ] && printf '%s\n' "$f"
done
for d in /usr/share/luna-service2/manifests.d /usr/share/luna-service2/client-permissions.d /usr/share/luna-service2/api-permissions.d /usr/share/luna-service2/groups.d /usr/share/luna-service2/roles.d /usr/share/luna-service2/services.d; do
    [ -d "$d" ] || continue
    find "$d" -maxdepth 1 -type f \( -iname '*lightingcontroller*' -o -iname '*livepick*' \) -print
done
SH
```

The example above intentionally enumerates paths only: copying binaries requires a second tar pass over the enumerated paths, and metadata-only listing must not be confused with collection of those files. Prefer the inline complete collection command in the chat response.
