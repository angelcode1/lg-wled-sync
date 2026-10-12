//! Vendor-neutral colour output. Capture provenance and geometry are independent
//! of transport; a vendor adapter receives only an RGB colour frame.
//! It MUST NOT assume full-frame provenance when source is histogram.
pub mod perimeter;
pub mod wled;

use crate::color::{HueGamut, RgbColor};
use crate::hue::{HueDtlsClient, HueStreamPacketBuilder};
use crate::nanoleaf::NanoleafUdpStreamer;
use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorSource {
    Frame,
    Histogram,
}

pub struct LightFrame<'a> {
    pub colors: &'a [RgbColor],
    pub source: ColorSource,
}

/// Vendor adapter API. No Hue identifiers, Nanoleaf panel IDs or WLED offsets
/// appear in the capture/colour frame. Adapters own those mappings.
pub trait LightOutput {
    fn kind(&self) -> &'static str;
    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()>;
}

/// Adapter for existing Hue Entertainment v2 transport and authoritative
/// Entertainment Area channel IDs.
pub struct HueOutput<'a> {
    pub transport: &'a mut HueDtlsClient,
    pub packets: &'a mut HueStreamPacketBuilder,
    pub channel_ids: &'a [u8],
    pub use_xy: bool,
}

impl LightOutput for HueOutput<'_> {
    fn kind(&self) -> &'static str { "hue-entertainment" }

    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        if frame.colors.len() != self.channel_ids.len() {
            bail!("Hue channel count does not match colour count");
        }
        let channels: Vec<(u8, (u16, u16, u16))> = self.channel_ids.iter()
            .zip(frame.colors)
            .map(|(&id, &color)| (id, if self.use_xy {
                color.to_xy_u16(HueGamut::GamutC)
            } else {
                color.to_u16()
            }))
            .collect();
        let payload = if self.use_xy {
            self.packets.build_xy_packet(&channels)
        } else {
            self.packets.build_rgb_packet(&channels)
        };
        self.transport.send(&payload)
    }
}

impl LightOutput for NanoleafUdpStreamer {
    fn kind(&self) -> &'static str { "nanoleaf-4d" }
    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        if frame.colors.len() != self.panel_count() {
            bail!("Nanoleaf panel count does not match colour count");
        }
        NanoleafUdpStreamer::send_frame(self, frame.colors, 0)
    }
}
