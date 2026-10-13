# webOS Open Source Edition GitHub — relevance to LG C5 V4L2 VT driver

Checked 2026-10-13. Candidate suggested:
[webosose GitHub organisation](https://github.com/webosose).

## What is published

The organisation publishes **webOS Open Source Edition (OSE)**,
a shared/open platform with independent build targets and API
compatibility. This is **not** the complete commercial LG TV
firmware/kernel source tree for the C5.

Its official
[API reference](https://www.webosose.org/docs/reference/)
explicitly warns:

> We DO NOT guarantee that webOS OSE APIs are compatible with LG products
> based on webOS, such as webOS TV, webOS Signage and Pro:Centric.

The primary source repos are:

- [webosose/build-webos](https://github.com/webosose/build-webos):
  orchestrates OpenEmbedded builds and advertises target
  `raspberrypi4-64` in its README; no C5/O22N3 kernel source
  supplied there.
- [webosose/meta-webosose](https://github.com/webosose/meta-webosose):
  BitBake layers including `meta-webos-raspberrypi` and
  `meta-webos-backports`.
  Its
  [`linux-raspberrypi_6.1.bb` recipe](https://github.com/webosose/meta-webosose/blob/master/meta-webos-backports/meta-webos-backports-5.1/dynamic-layers/raspberrypi/recipes-kernel/linux/linux-raspberrypi_6.1.bb)
  sets `LINUX_VERSION=6.1.77` and fetches
  `github.com/raspberrypi/linux` (along with Yocto kernel metadata),
  **not an LG TV Linux 5.4.268 tree**. A matching 6.6
  Raspberry Pi recipe and a
  [Raspberry Pi kernel bbappend](https://github.com/webosose/meta-webosose/blob/master/meta-webos-raspberrypi/recipes-kernel/linux/linux-raspberrypi_%25.bbappend)
  exist. The latter patches BCM2835 V4L2/video and
  audio-driver functionality; it does **not** establish O22N3
  capture driver support.
- [webosose/ls2-helpers](https://github.com/webosose/ls2-helpers):
  generic Luna Service Bus C++ helpers, useful for studying
  event, request and client lifecycle APIs. Do not copy its
  assumptions about roles or private services to the C5.

## Scoped source search result

The GitHub-connected source-index searches for
`v4l2_vt`, `vt_capture_thread`, and `o22n3` scoped to
`webosose` returned **no indexed file matches**.
Direct checks of the two kernel-related OSE recipes confirm
Raspberry Pi/Yocto provenance.

Search limitations: code search indexes a repository's default
branch and may omit unindexed repositories; these negative results
**do not prove** no reference exists anywhere across historical
branches, package downloads or binary artifacts. They do show
that the public OSE organisation is not presently a verified
source for the exact C5 driver.

## Decision for angelcode1/lg-wled-sync

Useful OSE components can inform **vendor-neutral userspace
architecture**, app lifecycle, public LS2 methods or shared
OpenEmbedded build patterns. OSE is not a source for C5-specific
kernel capture buffer ownership, DMA cache operations, DRM/HDCP
authorization, `v4l2_vt_vidioc_dqbuf`,
`v4l2_vt_vidioc_expbuf` or `vt_capture_thread`
without actual matching source code.

For those implementations, continue prioritizing
[LG's official product-specific source service](https://opensource.lge.com/product)
for `OLED42C5PSA.AAUQLJD`, `O22N3`,
firmware `33.00.71`, webOS TV `10.0.0`,
kernel `5.4.268-324`.

No C5 capture/DRM checks, root privileges, installation, or
buffer policy changed in this review. The exact-ELF
fail-closed raw pixel guard remains in force.
