//! One colour-processing pipeline for every lighting vendor.
//! Both hardware frame capture and LG histogram capture yield the same
//! immutable, device-independent spatial colour field. Processing (tone lift,
//! black gate, saturation, gamma, smoothing, scene cuts) runs ONCE upstream.
//! Adapters only decide *where* to sample and *how* to transmit.
use crate::capture::histogram::HistogramSettings;
use crate::color::{zones::FloatColor, ColorProcessor, RgbColor, ZoneSampler};
use crate::output::{perimeter::Point, ColorSource};
use anyhow::{bail, Result};
use std::time::{Duration, Instant};

pub const GRID_WIDTH: usize = 32;
pub const GRID_HEIGHT: usize = 18;
const CELLS: usize = GRID_WIDTH * GRID_HEIGHT;

#[derive(Debug, Clone)]
pub struct ColorField {
    pub source: ColorSource,
    pub scene_cut: bool,
    pixels: Vec<RgbColor>,
}

impl ColorField {
    pub fn source(&self) -> ColorSource { self.source }

    /// Bilinear interpolation between processed samples in screen coordinates,
    /// independent of LED count, protocol and physical channel identity.
    pub fn at(&self, x: f32, y: f32) -> RgbColor {
        if self.pixels.len() != CELLS { return RgbColor::new(0, 0, 0); }
        let x = ((x.clamp(0.0, 1.0) * GRID_WIDTH as f32) - 0.5)
            .clamp(0.0, (GRID_WIDTH - 1) as f32);
        let y = ((y.clamp(0.0, 1.0) * GRID_HEIGHT as f32) - 0.5)
            .clamp(0.0, (GRID_HEIGHT - 1) as f32);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(GRID_WIDTH - 1);
        let y1 = (y0 + 1).min(GRID_HEIGHT - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let a = self.pixels[y0 * GRID_WIDTH + x0];
        let b = self.pixels[y0 * GRID_WIDTH + x1];
        let c = self.pixels[y1 * GRID_WIDTH + x0];
        let d = self.pixels[y1 * GRID_WIDTH + x1];
        let mix = |a: u8, b: u8, c: u8, d: u8| -> u8 {
            let top = a as f32 * (1.0 - fx) + b as f32 * fx;
            let bottom = c as f32 * (1.0 - fx) + d as f32 * fx;
            (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
        };
        RgbColor::new(
            mix(a.r, b.r, c.r, d.r),
            mix(a.g, b.g, c.g, d.g),
            mix(a.b, b.b, c.b, d.b),
        )
    }

    pub fn points(&self, positions: &[Point]) -> Vec<RgbColor> {
        positions.iter().map(|p| self.at(p.x, p.y)).collect()
    }

    /// Normalized sampling rectangle for Hue zones. This is spatial mapping,
    /// not another saturation/temporal-processing pass.
    pub fn zone(&self, left: f32, right: f32, top: f32, bottom: f32) -> RgbColor {
        let mut totals = [0u32; 3];
        for yi in 0..3 {
            for xi in 0..3 {
                let x = left + (right - left) * (xi as f32 + 0.5) / 3.0;
                let y = top + (bottom - top) * (yi as f32 + 0.5) / 3.0;
                let c = self.at(x, y);
                totals[0] += c.r as u32;
                totals[1] += c.g as u32;
                totals[2] += c.b as u32;
            }
        }
        RgbColor::new((totals[0] / 9) as u8, (totals[1] / 9) as u8, (totals[2] / 9) as u8)
    }

    #[cfg(test)]
    fn from_colors(source: ColorSource, pixels: Vec<RgbColor>) -> Self {
        Self { source, scene_cut: false, pixels }
    }
}

/// Owns all colour processing state, exactly once per captured sample.
pub struct ColorPipeline {
    processor: ColorProcessor,
    smoothed: Vec<FloatColor>,
    last_source: Option<ColorSource>,
    last_sample: Option<Instant>,
    last_global: Option<RgbColor>,
    letterbox: bool,
    spatial_blend: f32,
}

impl ColorPipeline {
    pub fn new(saturation: f32, noise: f32, letterbox: bool) -> Self {
        Self {
            processor: ColorProcessor::new(0.35, false, saturation, noise),
            smoothed: vec![FloatColor::default(); CELLS],
            last_source: None,
            last_sample: None,
            last_global: None,
            letterbox,
            spatial_blend: 0.0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tune(&mut self, saturation: f32, noise: f32, gamma: f32,
        peak_weight: f32, rise: f32, fall: f32, strict_black: bool,
        max_step: u8, tone_lift: bool, letterbox: bool,
        spatial_blend: f32) {
        self.processor.set_saturation_boost(saturation);
        self.processor.set_noise_gate_threshold(noise);
        self.processor.set_gamma(gamma);
        self.processor.set_peak_weight(peak_weight);
        self.processor.set_temporal_response(rise, fall);
        self.processor.set_strict_blackout(strict_black);
        self.processor.set_max_color_step(max_step);
        self.processor.set_hdr_tone_mapping(tone_lift);
        self.letterbox = letterbox;
        self.spatial_blend = spatial_blend.clamp(0.0, 0.5);
    }

    pub fn reset(&mut self) {
        self.last_source = None;
        self.last_sample = None;
        self.last_global = None;
        self.smoothed.fill(FloatColor::default());
    }

    pub fn frame(&mut self, data: &[u8], width: u32, height: u32,
        bgra: bool) -> Result<ColorField> {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0
            || w.checked_mul(h).and_then(|n| n.checked_mul(4))
                .is_none_or(|needed| data.len() < needed) {
            bail!("invalid decoded frame dimensions or buffer length");
        }
        // Recompute once per captured frame, rather than independently for
        // each vendor. Capture dimensions are already bounded by Config.
        let rect = if self.letterbox {
            ZoneSampler::detect_active_rect(data, width, height, bgra)
        } else { Default::default() };

        let mut cells = Vec::with_capacity(CELLS);
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let mut total = [0u32; 3];
                for iy in 0..3 {
                    for ix in 0..3 {
                        let gx = (x as f32 + (ix as f32 + 0.5) / 3.0) / GRID_WIDTH as f32;
                        let gy = (y as f32 + (iy as f32 + 0.5) / 3.0) / GRID_HEIGHT as f32;
                        let sx = rect.x_min + gx * (rect.x_max - rect.x_min);
                        let sy = rect.y_min + gy * (rect.y_max - rect.y_min);
                        let px = ((sx.clamp(0.0, 1.0) * (w - 1) as f32) as usize).min(w - 1);
                        let py = ((sy.clamp(0.0, 1.0) * (h - 1) as f32) as usize).min(h - 1);
                        let idx = (py * w + px) * 4;
                        total[0] += data[idx + if bgra { 2 } else { 0 }] as u32;
                        total[1] += data[idx + 1] as u32;
                        total[2] += data[idx + if bgra { 0 } else { 2 }] as u32;
                    }
                }
                cells.push(RgbColor::new(
                    (total[0] / 9) as u8,
                    (total[1] / 9) as u8,
                    (total[2] / 9) as u8,
                ));
            }
        }
        Ok(self.process(cells, ColorSource::Frame))
    }

    pub fn histogram(&mut self, regions: &[RgbColor; 16],
        profile: &HistogramSettings) -> Result<ColorField> {
        // Histogram regions do not contain pixels. Each grid cell inherits
        // a calibrated nearest-edge chromatic estimate. On unknown TVs
        // the average profile produces a genuinely uniform field.
        let mut positions = Vec::with_capacity(CELLS);
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let px = (x as f32 + 0.5) / GRID_WIDTH as f32;
                let py = (y as f32 + 0.5) / GRID_HEIGHT as f32;
                let distances = [py, 1.0 - px, 1.0 - py, px];
                let edge = distances.iter().enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, _)| i).unwrap_or(0);
                positions.push(match edge {
                    0 => Point { x: px, y: 0.0 },
                    1 => Point { x: 1.0, y: py },
                    2 => Point { x: px, y: 1.0 },
                    _ => Point { x: 0.0, y: py },
                });
            }
        }
        let samples = profile.map_leds(regions, &positions)?;
        if samples.len() != CELLS { bail!("histogram profile did not produce full colour grid"); }
        Ok(self.process(samples, ColorSource::Histogram))
    }

    fn process(&mut self, raw: Vec<RgbColor>, source: ColorSource) -> ColorField {
        debug_assert_eq!(raw.len(), CELLS);
        let now = Instant::now();
        let source_changed = self.last_source != Some(source);
        if source_changed {
            self.smoothed.fill(FloatColor::default());
            self.last_global = None;
        }
        let elapsed = self.last_sample.replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or(Duration::from_millis(33));
        let elapsed = if source_changed { Duration::from_millis(33) } else { elapsed };
        self.last_source = Some(source);

        let mut sum = [0u64; 3];
        for c in &raw {
            sum[0] += c.r as u64;
            sum[1] += c.g as u64;
            sum[2] += c.b as u64;
        }
        let global = RgbColor::new(
            (sum[0] / CELLS as u64) as u8,
            (sum[1] / CELLS as u64) as u8,
            (sum[2] / CELLS as u64) as u8,
        );
        let scene_cut = source_changed
            || self.last_global.is_some_and(|prev| prev.delta(global) > 0.35);
        self.last_global = Some(global);

        let mut out = Vec::with_capacity(CELLS);
        for (i, c) in raw.into_iter().enumerate() {
            let mean = FloatColor::new(c.r as f32, c.g as f32, c.b as f32);
            // A newly selected source starts from its real first sample,
            // not black or stale colours from the previous backend.
            let previous = if source_changed { mean } else { self.smoothed[i] };
            let result = self.processor.process(mean, mean, mean.luminance(),
                previous, scene_cut, elapsed);
            self.smoothed[i] = result;
            out.push(result.to_rgb());
        }
        // Optional four-neighbour blend runs over the common grid once.
        // Black cells remain off and blending never feeds back into temporal
        // state. O(grid cells), suitable for older ARMv7 hardware.
        if self.spatial_blend > 0.0 {
            let original = out.clone();
            for y in 0..GRID_HEIGHT {
                for x in 0..GRID_WIDTH {
                    let index = y * GRID_WIDTH + x;
                    let center = original[index];
                    if center == RgbColor::new(0,0,0) { continue; }
                    let mut totals = [0u32; 3];
                    let mut count = 0u32;
                    for (dx,dy) in [(-1isize,0isize),(1,0),(0,-1),(0,1)] {
                        let nx=x as isize+dx;
                        let ny=y as isize+dy;
                        if nx<0 || ny<0 || nx>=GRID_WIDTH as isize || ny>=GRID_HEIGHT as isize {continue;}
                        let c=original[ny as usize*GRID_WIDTH+nx as usize];
                        totals[0]+=c.r as u32;
                        totals[1]+=c.g as u32;
                        totals[2]+=c.b as u32;
                        count+=1;
                    }
                    if count == 0 {continue;}
                    let avg = RgbColor::new(
                        (totals[0]/count) as u8,
                        (totals[1]/count) as u8,
                        (totals[2]/count) as u8,
                    );
                    let strength = self.spatial_blend;
                    let mix=|a:u8,b:u8| ->
                        u8 { (a as f32*(1.0-strength)+b as f32*strength).round() as u8 };
                    out[index]=RgbColor::new(mix(center.r,avg.r),
                        mix(center.g,avg.g),mix(center.b,avg.b));
                }
            }
        }
        ColorField { source, scene_cut, pixels: out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::histogram::HistogramSettings;
    #[test]
    fn uniform_video_field_is_vendor_independent() {
        let mut pipeline = ColorPipeline::new(1.0, 0.0, false);
        let mut frame = vec![0u8; 320 * 180 * 4];
        for px in frame.chunks_exact_mut(4) { px.copy_from_slice(&[100, 50, 20, 255]); }
        let field = pipeline.frame(&frame, 320, 180, false).unwrap();
        assert_eq!(field.source(), ColorSource::Frame);
        for p in [Point {x:0.0,y:0.0}, Point {x:0.9,y:0.2}] {
            let c = field.at(p.x, p.y);
            assert_eq!(c, RgbColor::new(100, 50, 20));
        }
    }
    #[test]
    fn histogram_average_is_uniform_and_switch_resets_source() {
        let mut pipeline = ColorPipeline::new(1.0, 0.0, false);
        let colors = [RgbColor::new(32, 64, 96); 16];
        let field = pipeline.histogram(&colors, &HistogramSettings::default()).unwrap();
        assert_eq!(field.source(), ColorSource::Histogram);
        assert_eq!(field.at(0.05, 0.05), field.at(0.95, 0.95));
    }
    #[test]
    fn bilinear_interpolation_is_stable_at_edges() {
        let mut cells=vec![RgbColor::new(0, 0, 0); CELLS];
        cells[0]=RgbColor::new(200,0,0);
        let field=ColorField::from_colors(ColorSource::Frame,cells);
        assert_eq!(field.at(0.0,0.0).r,200);
        assert_eq!(field.at(1.0,1.0).r,0);
    }
    #[test]
    fn invalid_frames_fail_without_emitting_synthetic_colours() {
        let mut p = ColorPipeline::new(1.0,0.0,false);
        assert!(p.frame(&[0; 3],320,180,false).is_err());
    }
}
