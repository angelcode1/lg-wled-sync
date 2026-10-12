//! Screen-edge geometry and processing shared across any addressable LED vendor.
use crate::color::{zones::FloatColor, ActiveRect, ColorProcessor, RgbColor, ZoneSampler};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartCorner {
    TopLeft,
    TopRight,
    BottomRight,
    #[default]
    BottomLeft,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point { pub x: f32, pub y: f32 }

/// Index i describes the *physical LED*; screen coordinates stay vendor-neutral.
pub fn positions(count: usize, corner: StartCorner, reverse: bool, offset: usize) -> Vec<Point> {
    if count == 0 { return Vec::new(); }
    const TOP: f32 = 16.0;
    const SIDE: f32 = 9.0;
    const TOTAL: f32 = 2.0 * (TOP + SIDE);
    let origin = match corner {
        StartCorner::TopLeft => 0.0,
        StartCorner::TopRight => TOP,
        StartCorner::BottomRight => TOP + SIDE,
        StartCorner::BottomLeft => 2.0 * TOP + SIDE,
    };
    (0..count).map(|i| {
        let step = (i + offset % count) % count;
        let travel = (step as f32 + 0.5) / count as f32 * TOTAL;
        let d = (origin + if reverse { -travel } else { travel }).rem_euclid(TOTAL);
        if d < TOP {
            Point { x: d / TOP, y: 0.0 }
        } else if d < TOP + SIDE {
            Point { x: 1.0, y: (d - TOP) / SIDE }
        } else if d < TOP * 2.0 + SIDE {
            Point { x: 1.0 - (d - TOP - SIDE) / TOP, y: 1.0 }
        } else {
            Point { x: 0.0, y: 1.0 - (d - TOP * 2.0 - SIDE) / SIDE }
        }
    }).collect()
}

/// Device-neutral sampler. Native histogram colour maps will feed this output
/// layer separately; they must not be represented as invented pixel frames.
pub struct PerimeterSampler {
    points: Vec<Point>,
    previous: Vec<FloatColor>,
    processor: ColorProcessor,
    active: ActiveRect,
    frames: u64,
    last: Option<Instant>,
}
impl PerimeterSampler {
    pub fn new(count: usize, corner: StartCorner, reverse: bool, offset: usize,
        saturation: f32, noise: f32, gamma: f32, strict_black: bool) -> Self {
        let mut processor = ColorProcessor::new(0.35, false, saturation, noise);
        processor.set_gamma(gamma);
        processor.set_strict_blackout(strict_black);
        processor.set_max_color_step(12);
        Self {
            points: positions(count,corner,reverse,offset),
            previous: vec![FloatColor::default();count],
            processor,
            active: ActiveRect::default(), frames: 0, last: None,
        }
    }
    pub fn screen_positions(&self) -> &[Point] { &self.points }
    pub fn sample_frame(&mut self, data: &[u8], width: u32, height: u32, bgra: bool,
        letterbox: bool) -> Option<Vec<RgbColor>> {
        let w=usize::try_from(width).ok()?;
        let h=usize::try_from(height).ok()?;
        if w == 0 || h == 0 || data.len() < w.checked_mul(h)?.checked_mul(4)? {
            return None;
        }
        self.frames += 1;
        if letterbox && (self.frames == 1 || self.frames.is_multiple_of(15)) {
            self.active = ZoneSampler::detect_active_rect(data,width,height,bgra);
        } else if !letterbox {
            self.active = ActiveRect::default();
        }
        let now=Instant::now();
        let elapsed=self.last.replace(now).map(|t| now.saturating_duration_since(t))
            .unwrap_or(Duration::from_millis(33));
        let mut out=Vec::with_capacity(self.points.len());
        for (i,p) in self.points.iter().enumerate() {
            let x=self.active.x_min + p.x*(self.active.x_max-self.active.x_min);
            let y=self.active.y_min + p.y*(self.active.y_max-self.active.y_min);
            // Sample a small inset region to avoid extreme border noise.
            let x=(x.clamp(0.0,1.0)*(w-1) as f32).round() as usize;
            let y=(y.clamp(0.0,1.0)*(h-1) as f32).round() as usize;
            let (mut r,mut g,mut b,mut n)=(0f32,0f32,0f32,0f32);
            for oy in -1isize..=1 {
                for ox in -1isize..=1 {
                    let px=x.saturating_add_signed(ox).min(w-1);
                    let py=y.saturating_add_signed(oy).min(h-1);
                    let index=(py*w+px)*4;
                    let pixel=&data[index..index+3];
                    r+=pixel[if bgra {2} else {0}] as f32;
                    g+=pixel[1] as f32;
                    b+=pixel[if bgra {0} else {2}] as f32;
                    n+=1.0;
                }
            }
            let mean=FloatColor::new(r/n,g/n,b/n);
            let updated=self.processor.process(mean,mean,mean.luminance(),
                self.previous[i],false,elapsed);
            self.previous[i]=updated;
            out.push(updated.to_rgb());
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_layout_handles_arbitrary_led_count() {
        for count in [1, 40, 241] {
            let p=positions(count,StartCorner::BottomLeft,true,7);
            assert_eq!(p.len(),count);
            for v in p { assert!((0.0..=1.0).contains(&v.x) && (0.0..=1.0).contains(&v.y)); }
        }
    }
    #[test]
    fn uniform_rgba_maps_to_all_leds() {
        let mut s=PerimeterSampler::new(8,StartCorner::TopLeft,false,0,1.0,0.0,1.0,false);
        let mut frame=vec![0u8;8*8*4];
        for px in frame.chunks_exact_mut(4) { px.copy_from_slice(&[64,0,0,255]); }
        let result=s.sample_frame(&frame,8,8,false,false).unwrap();
        assert_eq!(result.len(),8);
        assert!(result.iter().all(|c| c.r>0 && c.g==0 && c.b==0));
        assert!(s.sample_frame(&[0;2],8,8,false,false).is_none());
    }
}
