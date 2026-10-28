//! Keyframe system: animate any parameter over time.
//!
//! Every parameter in the system (opacity, position, effect params, etc.)
//! can be animated via keyframes with configurable easing.

use serde::{Deserialize, Serialize};

use crate::TimestampUs;

/// Easing function for interpolation between keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    /// Linear interpolation (constant rate).
    Linear,
    /// Ease in (starts slow, accelerates).
    EaseIn,
    /// Ease out (starts fast, decelerates).
    EaseOut,
    /// Ease in and out (slow start and end).
    EaseInOut,
    /// Custom cubic bezier curve: (x1, y1, x2, y2).
    CubicBezier(f32, f32, f32, f32),
    /// Hold the previous value until the next keyframe (step function).
    Hold,
}

impl Default for Easing {
    fn default() -> Self {
        Easing::Linear
    }
}

impl Easing {
    /// Evaluate the easing function at parameter t (0.0 to 1.0).
    /// Returns the eased value (0.0 to 1.0).
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => {
                cubic_bezier_evaluate(t, *x1, *y1, *x2, *y2)
            }
            Easing::Hold => 0.0, // Always returns start value
        }
    }
}

/// A single keyframe at a specific time with a value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    /// Time position in microseconds (relative to clip start).
    pub time_us: TimestampUs,
    /// The value at this keyframe.
    pub value: f32,
    /// Easing function to the *next* keyframe.
    pub easing: Easing,
}

/// A track of keyframes for animating a single parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeTrack {
    /// The parameter being animated (e.g., "opacity", "brightness").
    pub parameter: String,
    /// Keyframes sorted by time.
    keyframes: Vec<Keyframe>,
}

impl KeyframeTrack {
    /// Create a new keyframe track for a parameter.
    pub fn new(parameter: impl Into<String>) -> Self {
        Self {
            parameter: parameter.into(),
            keyframes: Vec::new(),
        }
    }

    /// Add a keyframe. Maintains sorted order by time.
    pub fn add_keyframe(&mut self, keyframe: Keyframe) {
        let pos = self
            .keyframes
            .binary_search_by_key(&keyframe.time_us, |k| k.time_us)
            .unwrap_or_else(|e| e);
        self.keyframes.insert(pos, keyframe);
    }

    /// Remove a keyframe at the given index.
    pub fn remove_keyframe(&mut self, index: usize) -> Option<Keyframe> {
        if index < self.keyframes.len() {
            Some(self.keyframes.remove(index))
        } else {
            None
        }
    }

    /// Get all keyframes.
    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    /// Evaluate the animated value at a given time.
    ///
    /// - Before the first keyframe: returns the first keyframe's value.
    /// - After the last keyframe: returns the last keyframe's value.
    /// - Between keyframes: interpolates using the easing function.
    pub fn evaluate(&self, time_us: TimestampUs) -> Option<f32> {
        if self.keyframes.is_empty() {
            return None;
        }

        // Before first keyframe
        if time_us <= self.keyframes[0].time_us {
            return Some(self.keyframes[0].value);
        }

        // After last keyframe
        if time_us >= self.keyframes.last().unwrap().time_us {
            return Some(self.keyframes.last().unwrap().value);
        }

        // Find the two surrounding keyframes
        for i in 0..self.keyframes.len() - 1 {
            let kf_a = &self.keyframes[i];
            let kf_b = &self.keyframes[i + 1];

            if time_us >= kf_a.time_us && time_us < kf_b.time_us {
                if matches!(kf_a.easing, Easing::Hold) {
                    return Some(kf_a.value);
                }

                let duration = (kf_b.time_us - kf_a.time_us) as f32;
                let t = (time_us - kf_a.time_us) as f32 / duration;
                let eased_t = kf_a.easing.evaluate(t);
                let value = kf_a.value + (kf_b.value - kf_a.value) * eased_t;
                return Some(value);
            }
        }

        None
    }

    /// Whether this track has any keyframes.
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    /// Number of keyframes.
    pub fn len(&self) -> usize {
        self.keyframes.len()
    }
}

/// Evaluate a cubic bezier curve at parameter t.
/// Control points are (0,0), (x1,y1), (x2,y2), (1,1).
fn cubic_bezier_evaluate(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Newton-Raphson to find the t parameter for the given x value
    // Then evaluate y at that parameter

    // For a cubic bezier: B(t) = 3(1-t)²t·P1 + 3(1-t)t²·P2 + t³
    let sample_curve_x = |t: f32| -> f32 {
        ((1.0 - 3.0 * x2 + 3.0 * x1) * t + (3.0 * x2 - 6.0 * x1)) * t + 3.0 * x1 * t
    };

    let sample_curve_y = |t: f32| -> f32 {
        ((1.0 - 3.0 * y2 + 3.0 * y1) * t + (3.0 * y2 - 6.0 * y1)) * t + 3.0 * y1 * t
    };

    let sample_curve_derivative_x = |t: f32| -> f32 {
        (3.0 * (1.0 - 3.0 * x2 + 3.0 * x1)) * t * t
            + (2.0 * (3.0 * x2 - 6.0 * x1)) * t
            + 3.0 * x1
    };

    // Newton-Raphson iteration
    let mut guess_t = t;
    for _ in 0..8 {
        let current_x = sample_curve_x(guess_t) - t;
        let derivative = sample_curve_derivative_x(guess_t);
        if derivative.abs() < 1e-6 {
            break;
        }
        guess_t -= current_x / derivative;
    }

    sample_curve_y(guess_t.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_easing() {
        assert!((Easing::Linear.evaluate(0.0) - 0.0).abs() < 0.001);
        assert!((Easing::Linear.evaluate(0.5) - 0.5).abs() < 0.001);
        assert!((Easing::Linear.evaluate(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_keyframe_interpolation() {
        let mut track = KeyframeTrack::new("opacity");
        track.add_keyframe(Keyframe {
            time_us: 0,
            value: 0.0,
            easing: Easing::Linear,
        });
        track.add_keyframe(Keyframe {
            time_us: 1_000_000,
            value: 1.0,
            easing: Easing::Linear,
        });

        assert!((track.evaluate(0).unwrap() - 0.0).abs() < 0.001);
        assert!((track.evaluate(500_000).unwrap() - 0.5).abs() < 0.001);
        assert!((track.evaluate(1_000_000).unwrap() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_keyframe_hold() {
        let mut track = KeyframeTrack::new("opacity");
        track.add_keyframe(Keyframe {
            time_us: 0,
            value: 0.0,
            easing: Easing::Hold,
        });
        track.add_keyframe(Keyframe {
            time_us: 1_000_000,
            value: 1.0,
            easing: Easing::Linear,
        });

        // Hold: should stay at 0.0 until reaching the next keyframe
        assert!((track.evaluate(500_000).unwrap() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_keyframe_extrapolation() {
        let mut track = KeyframeTrack::new("brightness");
        track.add_keyframe(Keyframe {
            time_us: 1_000_000,
            value: 0.5,
            easing: Easing::Linear,
        });
        track.add_keyframe(Keyframe {
            time_us: 2_000_000,
            value: 1.0,
            easing: Easing::Linear,
        });

        // Before first keyframe: clamp to first value
        assert!((track.evaluate(0).unwrap() - 0.5).abs() < 0.001);
        // After last keyframe: clamp to last value
        assert!((track.evaluate(3_000_000).unwrap() - 1.0).abs() < 0.001);
    }
}
