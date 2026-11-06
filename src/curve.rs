//! Volume reduction profiles.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Profile {
    /// Constant rate of change.
    #[default]
    Linear,
    /// Slow at the beginning, faster towards the end.
    Parabolic,
    /// Fast at the beginning, slower towards the end.
    Exponential,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Linear, Profile::Parabolic, Profile::Exponential];

    pub fn label(self) -> &'static str {
        match self {
            Profile::Linear => "Linear",
            Profile::Parabolic => "Parabólico",
            Profile::Exponential => "Exponencial",
        }
    }

    /// Progress shape in [0,1] for normalized time `t` in [0,1].
    pub fn shape(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Profile::Linear => t,
            Profile::Parabolic => t * t,
            Profile::Exponential => {
                const K: f32 = 5.0;
                (1.0 - (-K * t).exp()) / (1.0 - (-K).exp())
            }
        }
    }

    /// Volume (0..=1) at normalized time `t`, going from `start` to `end`.
    pub fn volume(self, start: f32, end: f32, t: f32) -> f32 {
        (start + (end - start) * self.shape(t)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_exact() {
        for p in Profile::ALL {
            assert!((p.volume(0.8, 0.1, 0.0) - 0.8).abs() < 1e-5, "{p:?} start");
            assert!((p.volume(0.8, 0.1, 1.0) - 0.1).abs() < 1e-5, "{p:?} end");
        }
    }

    #[test]
    fn monotonic_decreasing() {
        for p in Profile::ALL {
            let mut last = f32::MAX;
            for i in 0..=100 {
                let v = p.volume(1.0, 0.0, i as f32 / 100.0);
                assert!(v <= last + 1e-6, "{p:?} not monotonic at {i}");
                last = v;
            }
        }
    }

    #[test]
    fn shapes_differ_as_described() {
        let mid = 0.5;
        assert!(Profile::Parabolic.shape(mid) < Profile::Linear.shape(mid));
        assert!(Profile::Exponential.shape(mid) > Profile::Linear.shape(mid));
    }
}
