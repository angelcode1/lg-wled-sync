# C5 read-only improvement inventory (2026-10-17)

This is a diagnostic-only preflight for the known OLED42C5PSA.AAUQLJD. It does not call the LG capture API, open /dev/video60, issue ioctl operations, collect video pixels, alter Luna roles or change any TV settings.

## Mac host invocation

Run from this repository checkout, using the C5 IP from your own network. Keep the report local.

```sh
TV_HOST=192.168.8.X
ssh "root@$TV_HOST" 'sh -s' < scripts/c5-readonly-improvement-inventory.sh > c5-readonly-inventory.txt 2>&1
```

Inspect before sharing, as the output may include network identifiers in process command lines or kernel metadata.

## Two additional bounded read-only private Luna checks

The tested C5 often returns no useful stdout from one-shot `luna-send -n 1`. Use interactive private Luna with a short open stdin:

```sh
TV_HOST=192.168.8.X
ssh "root@$TV_HOST" 'sleep 5 | luna-send -i -f luna://com.webos.media/getActivePipelines '\''{}'\''' > c5-pipelines.txt 2>&1
ssh "root@$TV_HOST" 'sleep 5 | luna-send -i -f luna://com.webos.service.tv.systemproperty/getSystemInfo '\''{"keys":["modelName","sdkVersion"]}'\''' > c5-model.txt 2>&1
```

These are observations, **not** a trusted HDCP classification. HDMI_INPUT without SVP_CPB remains Unknown.

## Evidence to interpret

- Exact `libvtcapture` SHA256 and native exports. The original tested SHA256 `6483fc0b82a9da6f65e11e85f9dc43bb163de80fe2c3c84b84afa251b0f6892c` has a histogram implementation that unconditionally returns 7, and no proven CPU buffer ownership.
- /sys/class/video4linux/video60 name, dev, and platform driver identity.
- Competing daemon ownership and marker state.
- Real-time frame capture is deliberately not initiated and cannot be inferred from these metadata checks.

## Out of scope until qualified

- No `vtCapture_initHistogram` retry for the known-stubbed ELF.
- No `REQBUFS`, `STREAMON`, unmanaged MMAP, buffer-copy probes or permissions changes.
- No custom HDMI capture workaround or HDCP policy changes.
- No install/deploy/restart as part of evidence gathering.

Document any new capture firmware identity before changing the Rust guard. Other device families must not inherit guessed C5 quirks.
