//! DRM-first source policy. This is deliberately independent from LG's unverified
//! histogram native ABI and from the eventual media-pipeline classifier.
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackClass { Protected, Clear, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode { #[default] AutoDrmFirst, FrameOnly, HistogramOnly }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistogramHealth { Valid, NotReady }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Frame, Histogram, Hold }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub source: Source,
    pub protected_latched: bool,
}

pub struct DrmCaptureRouter {
    mode: CaptureMode,
    protected: bool,
    clear_since: Option<Instant>,
    clear_stability: Duration,
}

impl Default for DrmCaptureRouter {
    fn default() -> Self {
        Self { mode: CaptureMode::AutoDrmFirst, protected: false,
            clear_since: None, clear_stability: Duration::from_secs(2) }
    }
}

impl DrmCaptureRouter {
    pub fn set_mode(&mut self, mode: CaptureMode) { self.mode = mode; }
    pub fn decide(&mut self, playback: PlaybackClass, histogram: HistogramHealth,
                  now: Instant) -> Decision {
        match playback {
            PlaybackClass::Protected => {
                self.protected = true;
                self.clear_since = None;
            }
            PlaybackClass::Clear if self.protected => {
                let since = *self.clear_since.get_or_insert(now);
                if now.saturating_duration_since(since) >= self.clear_stability {
                    self.protected = false;
                    self.clear_since = None;
                }
            }
            PlaybackClass::Unknown => self.clear_since = None,
            PlaybackClass::Clear => (),
        }
        let use_histogram = match self.mode {
            CaptureMode::AutoDrmFirst => self.protected,
            CaptureMode::FrameOnly => false,
            CaptureMode::HistogramOnly => true,
        };
        Decision {
            source: if use_histogram {
                if histogram == HistogramHealth::Valid { Source::Histogram } else { Source::Hold }
            } else { Source::Frame },
            protected_latched: self.protected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_session_never_silently_falls_back() {
        let mut r=DrmCaptureRouter::default();
        let t=Instant::now();
        let d=r.decide(PlaybackClass::Protected,HistogramHealth::NotReady,t);
        assert_eq!(d.source,Source::Hold);
        assert!(d.protected_latched);
        assert_eq!(r.decide(PlaybackClass::Unknown,HistogramHealth::Valid,t).source,Source::Histogram);
    }
    #[test]
    fn clear_session_requires_stable_classification() {
        let mut r=DrmCaptureRouter::default();
        let t=Instant::now();
        assert_eq!(r.decide(PlaybackClass::Clear,HistogramHealth::NotReady,t).source,Source::Frame);
        r.decide(PlaybackClass::Protected,HistogramHealth::Valid,t);
        assert_eq!(r.decide(PlaybackClass::Clear,HistogramHealth::Valid,t).source,Source::Histogram);
        assert_eq!(r.decide(PlaybackClass::Clear,HistogramHealth::Valid,t+Duration::from_secs(2)).source,Source::Frame);
    }
    #[test]
    fn unknown_interrupts_clear_timer() {
        let mut r=DrmCaptureRouter::default();
        let t=Instant::now();
        r.decide(PlaybackClass::Protected,HistogramHealth::Valid,t);
        r.decide(PlaybackClass::Clear,HistogramHealth::Valid,t+Duration::from_secs(1));
        r.decide(PlaybackClass::Unknown,HistogramHealth::Valid,t+Duration::from_secs(2));
        assert_eq!(r.decide(PlaybackClass::Clear,HistogramHealth::Valid,t+Duration::from_secs(3)).source,Source::Histogram);
    }
}