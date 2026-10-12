# LG C5 native histogram ABI — verified exports, hardware untested

Evidence collected from the owner's LG C5 at 192.168.8.184 on 2026-10-12.
Do not commit full proprietary LG library or device logs to the public repository.

## Proven by direct read-only inspection

- Kernel: aarch64, Linux 5.4.268-324.
- Application userspace: 32-bit (confirmed by `getconf LONG_BIT`); dynamic loader `/lib/ld-linux.so.3`.
- Reported userspace glibc: 2.35.
- Native capture file: `/usr/lib/libvtcapture.so.1.0.0` (ELF32 ARM v7, EABI5, **soft-float ABI**).
- ELF BuildID: `9fb5e56d59c29ed242c0e4a66015c71601a4657e`.
- Hardware floating-point capability is VFPv3, but its presence does **not** indicate hard-float function argument passing.
- Linked libraries: `libresource_mgr_client.so.1`, `libglib-2.0.so.0`, `libstdc++.so.6`, `libgcc_s.so.1`, `libc.so.6`.
- All four public histogram entrypoints appear in the exported ELF symbol table:
  - `vtCapture_initHistogram` (1584-byte function)
  - `vtCapture_setPresetHistogram` (176-byte function)
  - `vtCapture_getHistogram` (172-byte function)
  - `vtCapture_finalizeHistogram` (244-byte function)
- Additional C++ `VtHistogram` symbol names are visible; they do not establish a separate supported public ABI.

## Build implications

- Keep target `armv7-unknown-linux-gnueabi` and the currently green ARMv7 CI gate.
- The compiled HyperDRM 0.13.0 service supplied by the owner has the same ELF32 ARM/EABI5/soft-float calling convention and `/lib/ld-linux.so.3` interpreter.
- The reference HyperDRM service has version references through GLIBC_2.34, compatible in principle with the C5's reported glibc 2.35. This is not a runtime test.
- The Rust `HistogramCapture` C ABI signatures and 0x604-byte record type match the reference HyperDRM source (`native/histogram.h`, `native/probe.c`, `native/service.c`), **not** a newly recovered OEM LG header.
- LG's `printLog` variadic symbol is supplied by a small C shim and verified by ARMv7 CI exports.

## Not yet proven

- Actual `dlopen` of `libvtcapture` from our daemon on the C5, including transitive runtime symbols.
- `initHistogram` success and permission/ownership conditions.
- Successful `setPresetHistogram`, nonempty valid histogram samples, spatial calibration or DRM content coverage.
- C5 `getActivePipelines` schema and `getSystemInfo` reply. The earlier SSH preflight recorded blank responses; the collector did not capture exit codes.
- Installed HyperDRM application and service. Earlier app launch returned `-101 not exist`, and the expected calibration file was absent.

## Next read-only TV diagnostics

```sh
ssh root@192.168.8.184 '
  printf "Luna binary: "; command -v luna-send || true
  printf "Luna help: "; luna-send -h 2>&1 | head -12
  echo MODEL;
  luna-send -n 1 luna://com.webos.service.tv.systemproperty/getSystemInfo '\''{"keys":["modelName"]}'\'' 2>&1
  echo "model_exit=$?"
  echo PIPELINES;
  luna-send -n 1 luna://com.webos.media/getActivePipelines '\''{}'\'' 2>&1
  echo "pipelines_exit=$?"
  echo APPLICATIONS;
  ls -ld /media/developer/apps/usr/palm/applications/org.webosbrew.histogramhyperion 2>&1
  echo SERVICES;
  ls -ld /media/developer/apps/usr/palm/services/org.webosbrew.histogramhyperion.service 2>&1
'
```

No TV write commands, firmware modifications or DRM circumvention are part of this check.

## Reference file evidence

- Supplied HyperDRM version: 0.13.0 ARM32 IPK.
- `native/probe.c` defines its C signatures and a 3-read test.
- `native/histogram.h` defines a 0x604 histogram record (one uint32 ID plus 3x256 uint16 bins).
- The package app ID is `org.webosbrew.histogramhyperion` and service ID is `org.webosbrew.histogramhyperion.service`.
