//! DRM-first source policy. This is deliberately independent from LG's unverified
//! histogram native ABI and from the eventual media-pipeline classifier.
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackClass {
    Protected,
    Clear,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureMode {
    #[default]
    AutoDrmFirst,
    FrameOnly,
    HistogramOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistogramHealth {
    Valid,
    NotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Frame,
    Histogram,
    Hold,
}

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
        Self {
            mode: CaptureMode::AutoDrmFirst,
            protected: false,
            clear_since: None,
            clear_stability: Duration::from_secs(2),
        }
    }
}

impl DrmCaptureRouter {
    pub fn set_mode(&mut self, mode: CaptureMode) {
        self.mode = mode;
    }
    pub fn decide(
        &mut self,
        playback: PlaybackClass,
        histogram: HistogramHealth,
        now: Instant,
    ) -> Decision {
        // No frame capture on Unknown, including startup and a failed/stale
        // media query. A reported Clear must remain stable for two seconds
        // before we permit *initialization*, not just publication.
        let clear_stable = match playback {
            PlaybackClass::Protected => {
                self.protected = true;
                self.clear_since = None;
                false
            }
            PlaybackClass::Unknown => {
                self.clear_since = None;
                false
            }
            PlaybackClass::Clear => {
                let since = *self.clear_since.get_or_insert(now);
                if now.saturating_duration_since(since) >= self.clear_stability {
                    self.protected = false;
                    true
                } else {
                    false
                }
            }
        };

        // A protection latch is never overridden by the FrameOnly option.
        // Histogram access is separate from video frame access and can only
        // be used when a real, healthy backend exists.
        let source = if self.protected {
            if self.mode != CaptureMode::FrameOnly && histogram == HistogramHealth::Valid {
                Source::Histogram
            } else {
                Source::Hold
            }
        } else if !clear_stable {
            Source::Hold
        } else {
            match self.mode {
                CaptureMode::AutoDrmFirst | CaptureMode::FrameOnly => Source::Frame,
                CaptureMode::HistogramOnly if histogram == HistogramHealth::Valid => {
                    Source::Histogram
                }
                CaptureMode::HistogramOnly => Source::Hold,
            }
        };
        Decision {
            source,
            protected_latched: self.protected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_session_never_silently_falls_back() {
        let mut r = DrmCaptureRouter::default();
        let t = Instant::now();
        let d = r.decide(PlaybackClass::Protected, HistogramHealth::NotReady, t);
        assert_eq!(d.source, Source::Hold);
        assert!(d.protected_latched);
        assert_eq!(
            r.decide(PlaybackClass::Unknown, HistogramHealth::Valid, t)
                .source,
            Source::Histogram
        );
    }
    #[test]
    fn clear_session_requires_stable_classification() {
        let mut r = DrmCaptureRouter::default();
        let t = Instant::now();
        assert_eq!(
            r.decide(PlaybackClass::Clear, HistogramHealth::NotReady, t)
                .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::NotReady,
                t + Duration::from_secs(2)
            )
            .source,
            Source::Frame
        );
        r.decide(PlaybackClass::Protected, HistogramHealth::Valid, t);
        assert_eq!(
            r.decide(PlaybackClass::Clear, HistogramHealth::Valid, t)
                .source,
            Source::Histogram
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::Valid,
                t + Duration::from_secs(2)
            )
            .source,
            Source::Frame
        );
    }
    #[test]
    fn unknown_interrupts_clear_timer() {
        let mut r = DrmCaptureRouter::default();
        let t = Instant::now();
        r.decide(PlaybackClass::Protected, HistogramHealth::Valid, t);
        r.decide(
            PlaybackClass::Clear,
            HistogramHealth::Valid,
            t + Duration::from_secs(1),
        );
        r.decide(
            PlaybackClass::Unknown,
            HistogramHealth::Valid,
            t + Duration::from_secs(2),
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::Valid,
                t + Duration::from_secs(3)
            )
            .source,
            Source::Histogram
        );
    }
    #[test]
    fn unknown_startup_and_clear_stability_fail_closed() {
        let mut r = DrmCaptureRouter::default();
        let t = Instant::now();
        for mode in [
            CaptureMode::AutoDrmFirst,
            CaptureMode::FrameOnly,
            CaptureMode::HistogramOnly,
        ] {
            r.set_mode(mode);
            assert_eq!(
                r.decide(PlaybackClass::Unknown, HistogramHealth::NotReady, t)
                    .source,
                Source::Hold
            );
        }
        r.set_mode(CaptureMode::AutoDrmFirst);
        assert_eq!(
            r.decide(PlaybackClass::Clear, HistogramHealth::NotReady, t)
                .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::NotReady,
                t + Duration::from_millis(1999)
            )
            .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::NotReady,
                t + Duration::from_secs(2)
            )
            .source,
            Source::Frame
        );
        // A lost classification immediately revokes the Frame route.
        assert_eq!(
            r.decide(
                PlaybackClass::Unknown,
                HistogramHealth::NotReady,
                t + Duration::from_secs(3)
            )
            .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::NotReady,
                t + Duration::from_secs(4)
            )
            .source,
            Source::Hold
        );
    }

    #[test]
    fn frame_only_cannot_override_protected_latch() {
        let mut r = DrmCaptureRouter::default();
        r.set_mode(CaptureMode::FrameOnly);
        let t = Instant::now();
        assert_eq!(
            r.decide(PlaybackClass::Protected, HistogramHealth::Valid, t)
                .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Unknown,
                HistogramHealth::Valid,
                t + Duration::from_secs(1)
            )
            .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::Valid,
                t + Duration::from_secs(2)
            )
            .source,
            Source::Hold
        );
        assert_eq!(
            r.decide(
                PlaybackClass::Clear,
                HistogramHealth::NotReady,
                t + Duration::from_secs(4)
            )
            .source,
            Source::Frame
        );
    }
}
