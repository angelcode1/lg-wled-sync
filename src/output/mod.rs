//! Vendor-neutral colour output. Capture provenance and geometry are independent
//! of transport; a vendor adapter receives only an RGB colour frame.
//! It MUST NOT assume full-frame provenance when source is histogram.
pub mod gate;
pub mod perimeter;
pub mod wled;

use crate::color::{pipeline::ColorField, HueGamut, RgbColor};
use crate::config::LightZone;
use crate::hue::{HueDtlsClient, HueStreamPacketBuilder};
use crate::nanoleaf::NanoleafUdpStreamer;
use crate::output::perimeter::Point;
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

impl HueOutput<'_> {
    /// Hue owns 2D Entertainment Area geometry; the shared colour field is
    /// already processed and never undergoes additional smoothing here.
    pub fn map_field(field: &ColorField, zones: &[LightZone], gain: f32) -> Vec<RgbColor> {
        zones
            .iter()
            .map(|z| field.zone(z.x_min, z.x_max, z.y_min, z.y_max).scale(gain))
            .collect()
    }
}

impl NanoleafUdpStreamer {
    /// Nanoleaf's physical panel ordering comes from the controller, not
    /// from any assumptions about frame or histogram spatial samples.
    pub fn map_field(field: &ColorField, positions: &[Point], gain: f32) -> Vec<RgbColor> {
        field
            .points(positions)
            .into_iter()
            .map(|c| c.scale(gain))
            .collect()
    }
}

impl LightOutput for HueOutput<'_> {
    fn kind(&self) -> &'static str {
        "hue-entertainment"
    }

    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        if frame.colors.len() != self.channel_ids.len() {
            bail!("Hue channel count does not match colour count");
        }
        let channels: Vec<(u8, (u16, u16, u16))> = self
            .channel_ids
            .iter()
            .zip(frame.colors)
            .map(|(&id, &color)| {
                (
                    id,
                    if self.use_xy {
                        color.to_xy_u16(HueGamut::GamutC)
                    } else {
                        color.to_u16()
                    },
                )
            })
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
    fn kind(&self) -> &'static str {
        "nanoleaf-4d"
    }
    fn send(&mut self, frame: &LightFrame<'_>) -> Result<()> {
        if frame.colors.len() != self.panel_count() {
            bail!("Nanoleaf panel count does not match colour count");
        }
        NanoleafUdpStreamer::send_frame(self, frame.colors, 0)
    }
}

#[cfg(test)]
mod spatial_fanout_tests {
    use super::*;
    use crate::capture::histogram::HistogramSettings;
    use crate::color::pipeline::ColorPipeline;
    use crate::config::Config;

    #[test]
    fn same_histogram_field_maps_to_all_vendors_without_more_processing() {
        let mut pipeline = ColorPipeline::new(1.0, 0.0, false);
        let pixels = [RgbColor::new(60, 80, 100); 16];
        let field = pipeline
            .histogram(&pixels, &HistogramSettings::default())
            .unwrap();
        assert_eq!(field.source(), ColorSource::Histogram);
        let led_points = [Point { x: 0.0, y: 0.5 }, Point { x: 1.0, y: 0.5 }];
        let wled = wled::WledUdpStreamer::map_field(&field, &led_points, 1.0);
        let nanoleaf = NanoleafUdpStreamer::map_field(&field, &led_points, 1.0);
        let cfg = Config::new_default("", "", "", "");
        let hue = HueOutput::map_field(&field, &cfg.zones, 1.0);
        assert_eq!(wled, nanoleaf);
        assert!(hue.iter().all(|&c| c == wled[0]));
        assert_eq!(wled, vec![RgbColor::new(60, 80, 100); 2]);
    }

    #[test]
    fn mapping_applies_only_final_vendor_brightness_trim() {
        let mut pipeline = ColorPipeline::new(1.0, 0.0, false);
        let raw = [RgbColor::new(120, 80, 40); 16];
        let field = pipeline
            .histogram(&raw, &HistogramSettings::default())
            .unwrap();
        let pos = [Point { x: 0.0, y: 0.0 }];
        let full = wled::WledUdpStreamer::map_field(&field, &pos, 1.0);
        let half = wled::WledUdpStreamer::map_field(&field, &pos, 0.5);
        assert_eq!(full[0], raw[0]);
        assert_eq!(half[0], RgbColor::new(60, 40, 20));
    }
}
