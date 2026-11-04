//! Adaptive quality governor: monitors frame time and adjusts preview quality.
//!
//! Section 3.7 of the Technical Proposal:
//! A controller tracks an exponential moving average (EMA) of frame time.
//! If it exceeds the frame budget, it steps down preview quality in order:
//! scale (100% -> 75% -> 50%), then expensive effects, then shadows/blur.
//! It steps back up when idle. The UI and audio are never sacrificed; preview
//! detail is. Full quality always returns on pause, and export always renders
//! at 100% quality.

use std::time::Duration;

/// Preview quality scale levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreviewScale {
    /// 25% preview resolution (extreme load / battery saver)
    Quarter = 0,
    /// 50% preview resolution (heavy load)
    Half = 1,
    /// 75% preview resolution (moderate load)
    ThreeQuarter = 2,
    /// 100% full preview resolution
    Full = 3,
}

impl PreviewScale {
    /// Floating-point multiplier for resolution width and height.
    pub fn factor(&self) -> f32 {
        match self {
            Self::Quarter => 0.25,
            Self::Half => 0.50,
            Self::ThreeQuarter => 0.75,
            Self::Full => 1.0,
        }
    }

    /// Step down one level, if possible.
    pub fn step_down(&self) -> Self {
        match self {
            Self::Full => Self::ThreeQuarter,
            Self::ThreeQuarter => Self::Half,
            Self::Half | Self::Quarter => Self::Quarter,
        }
    }

    /// Step up one level, if possible.
    pub fn step_up(&self) -> Self {
        match self {
            Self::Quarter => Self::Half,
            Self::Half => Self::ThreeQuarter,
            Self::ThreeQuarter | Self::Full => Self::Full,
        }
    }

    /// Compute scaled dimensions, ensuring at least 1x1 pixel.
    pub fn scale_dimensions(&self, width: u32, height: u32) -> (u32, u32) {
        let f = self.factor();
        (
            ((width as f32 * f).round() as u32).max(1),
            ((height as f32 * f).round() as u32).max(1),
        )
    }
}

/// Adaptive Quality Governor configuration.
#[derive(Debug, Clone)]
pub struct GovernorConfig {
    /// Target frame time budget (e.g., 16.6ms for 60fps).
    pub budget: Duration,
    /// Smoothing factor for exponential moving average (0.0 to 1.0).
    /// Higher values give more weight to recent frames. Default: 0.15.
    pub ema_alpha: f64,
    /// Consecutive over-budget frames before stepping down. Default: 3.
    pub over_budget_threshold: u32,
    /// Consecutive under-budget frames before stepping up. Default: 30.
    pub under_budget_threshold: u32,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            budget: Duration::from_micros(16_667), // 60 fps
            ema_alpha: 0.15,
            over_budget_threshold: 3,
            under_budget_threshold: 30,
        }
    }
}

/// Adaptive Quality Governor tracks frame timing and regulates preview fidelity.
#[derive(Debug)]
pub struct QualityGovernor {
    config: GovernorConfig,
    /// Exponential moving average of frame time in microseconds.
    ema_frame_time_us: f64,
    /// Current preview resolution scale.
    current_scale: PreviewScale,
    /// Consecutive frames where frame time exceeded budget.
    over_budget_streak: u32,
    /// Consecutive frames where frame time was safely under budget (e.g. < 70% budget).
    under_budget_streak: u32,
    /// Whether playback is currently active (vs paused/idle).
    is_playing: bool,
    /// Total frames recorded.
    total_frames: u64,
    /// Dropped / over-budget frame counter.
    dropped_frames: u64,
}

impl QualityGovernor {
    pub fn new(config: GovernorConfig) -> Self {
        let budget_us = config.budget.as_micros() as f64;
        Self {
            config,
            ema_frame_time_us: budget_us * 0.5, // Start with safe estimate
            current_scale: PreviewScale::Full,
            over_budget_streak: 0,
            under_budget_streak: 0,
            is_playing: false,
            total_frames: 0,
            dropped_frames: 0,
        }
    }

    /// Record the completion of a frame render and update adaptive governor state.
    pub fn record_frame(&mut self, render_time: Duration) {
        self.total_frames += 1;
        let time_us = render_time.as_micros() as f64;
        let budget_us = self.config.budget.as_micros() as f64;

        // Update Exponential Moving Average (EMA)
        self.ema_frame_time_us = self.config.ema_alpha * time_us
            + (1.0 - self.config.ema_alpha) * self.ema_frame_time_us;

        if time_us > budget_us {
            self.dropped_frames += 1;
            self.over_budget_streak += 1;
            self.under_budget_streak = 0;

            // Trigger step down if consistently exceeding budget
            if self.over_budget_streak >= self.config.over_budget_threshold {
                let prev = self.current_scale;
                self.current_scale = self.current_scale.step_down();
                if self.current_scale != prev {
                    log::warn!(
                        "Governor: stepping down preview scale from {:?} to {:?} (EMA: {:.2}ms, budget: {:.2}ms)",
                        prev,
                        self.current_scale,
                        self.ema_frame_time_us / 1000.0,
                        budget_us / 1000.0,
                    );
                }
                self.over_budget_streak = 0;
            }
        } else if self.ema_frame_time_us < budget_us * 0.70 {
            // Well under budget (headroom > 30%)
            self.under_budget_streak += 1;
            self.over_budget_streak = 0;

            // Trigger step up after sustained headroom
            if self.under_budget_streak >= self.config.under_budget_threshold {
                let prev = self.current_scale;
                self.current_scale = self.current_scale.step_up();
                if self.current_scale != prev {
                    log::info!(
                        "Governor: stepping up preview scale from {:?} to {:?} (EMA: {:.2}ms)",
                        prev,
                        self.current_scale,
                        self.ema_frame_time_us / 1000.0,
                    );
                }
                self.under_budget_streak = 0;
            }
        } else {
            // In the safe sweet spot
            self.over_budget_streak = 0;
            self.under_budget_streak = 0;
        }
    }

    /// Set playback state.
    /// When paused, full quality is always restored immediately so the user
    /// sees crisp, uncompromised detail when examining a still frame.
    pub fn set_playing(&mut self, is_playing: bool) {
        self.is_playing = is_playing;
        if !is_playing {
            // Restore full quality immediately on pause
            self.current_scale = PreviewScale::Full;
            self.over_budget_streak = 0;
            self.under_budget_streak = 0;
        }
    }

    /// Current preview scale. When paused, always returns Full.
    pub fn current_scale(&self) -> PreviewScale {
        if !self.is_playing {
            PreviewScale::Full
        } else {
            self.current_scale
        }
    }

    /// Current exponential moving average frame time in milliseconds.
    pub fn ema_frame_time_ms(&self) -> f64 {
        self.ema_frame_time_us / 1000.0
    }

    /// Percentage of frames that missed the budget (0.0 to 100.0).
    pub fn drop_rate_percent(&self) -> f64 {
        if self.total_frames == 0 {
            0.0
        } else {
            (self.dropped_frames as f64 / self.total_frames as f64) * 100.0
        }
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.total_frames = 0;
        self.dropped_frames = 0;
        self.over_budget_streak = 0;
        self.under_budget_streak = 0;
    }
}

impl Default for QualityGovernor {
    fn default() -> Self {
        Self::new(GovernorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_dimensions() {
        assert_eq!(
            PreviewScale::Full.scale_dimensions(1920, 1080),
            (1920, 1080)
        );
        assert_eq!(PreviewScale::Half.scale_dimensions(1920, 1080), (960, 540));
        assert_eq!(
            PreviewScale::ThreeQuarter.scale_dimensions(1920, 1080),
            (1440, 810)
        );
        assert_eq!(
            PreviewScale::Quarter.scale_dimensions(1920, 1080),
            (480, 270)
        );
    }

    #[test]
    fn test_step_down_under_load() {
        let mut gov = QualityGovernor::new(GovernorConfig {
            budget: Duration::from_millis(16),
            ema_alpha: 0.5,
            over_budget_threshold: 2,
            under_budget_threshold: 10,
        });

        gov.set_playing(true);
        assert_eq!(gov.current_scale(), PreviewScale::Full);

        // Frame 1: heavy (30ms > 16ms budget)
        gov.record_frame(Duration::from_millis(30));
        assert_eq!(gov.current_scale(), PreviewScale::Full);

        // Frame 2: heavy again -> should step down to ThreeQuarter
        gov.record_frame(Duration::from_millis(30));
        assert_eq!(gov.current_scale(), PreviewScale::ThreeQuarter);

        // Frame 3 & 4: still heavy -> should step down to Half
        gov.record_frame(Duration::from_millis(30));
        gov.record_frame(Duration::from_millis(30));
        assert_eq!(gov.current_scale(), PreviewScale::Half);
    }

    #[test]
    fn test_full_quality_restores_on_pause() {
        let mut gov = QualityGovernor::new(GovernorConfig {
            budget: Duration::from_millis(16),
            ema_alpha: 0.5,
            over_budget_threshold: 1,
            under_budget_threshold: 10,
        });

        gov.set_playing(true);
        gov.record_frame(Duration::from_millis(40));
        assert_eq!(gov.current_scale(), PreviewScale::ThreeQuarter);

        // Pause playback
        gov.set_playing(false);
        // Must immediately report Full quality on pause
        assert_eq!(gov.current_scale(), PreviewScale::Full);
    }

    #[test]
    fn test_step_up_when_idle() {
        let mut gov = QualityGovernor::new(GovernorConfig {
            budget: Duration::from_millis(16),
            ema_alpha: 0.5,
            over_budget_threshold: 1,
            under_budget_threshold: 2,
        });

        gov.set_playing(true);
        gov.record_frame(Duration::from_millis(30));
        assert_eq!(gov.current_scale(), PreviewScale::ThreeQuarter);

        // Deliver fast frames to bring EMA down and trigger step up
        gov.record_frame(Duration::from_millis(4));
        gov.record_frame(Duration::from_millis(4));
        gov.record_frame(Duration::from_millis(4));
        // Under-budget streak hits threshold -> step back up to Full
        assert_eq!(gov.current_scale(), PreviewScale::Full);
    }
}
