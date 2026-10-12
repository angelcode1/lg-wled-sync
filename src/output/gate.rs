//! Per-output transport deduplication. This does not change colours.
use crate::color::RgbColor;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct OutputGate {
    sent: HashMap<String, (Vec<RgbColor>, Instant)>,
}

impl OutputGate {
    pub fn should_send(&self, key: &str, colors: &[RgbColor], adaptive: bool) -> bool {
        if !adaptive {
            return true;
        }
        match self.sent.get(key) {
            None => true,
            Some((old, time)) => {
                old.len() != colors.len()
                    || old.iter().zip(colors).any(|(a, b)| a.delta(*b) >= 0.005)
                    || time.elapsed() >= Duration::from_millis(500)
            }
        }
    }
    /// Record successful local transmission only; a failed socket send must retry.
    pub fn record(&mut self, key: &str, colors: &[RgbColor]) {
        self.sent
            .insert(key.to_string(), (colors.to_vec(), Instant::now()));
    }
    pub fn clear(&mut self) {
        self.sent.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_frame_skips_and_update_sends() {
        let mut gate = OutputGate::default();
        let black = [RgbColor::new(0, 0, 0)];
        let red = [RgbColor::new(200, 0, 0)];
        assert!(gate.should_send("wled-0", &black, true));
        gate.record("wled-0", &black);
        assert!(!gate.should_send("wled-0", &black, true));
        assert!(gate.should_send("wled-0", &red, true));
        assert!(gate.should_send("wled-1", &black, true));
        gate.clear();
        assert!(gate.should_send("wled-0", &black, true));
    }
}
