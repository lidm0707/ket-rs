//! Confidence via the upstream conformal overlay
//! (katgpt-core::conformal, default-on, GOAT-passed Plan 468 /
//! "Report the Floor" Issue 010): `ConformalIntervalCalibrator` wrapped over
//! a ket score-source `PointForecaster`, floored by
//! `ConformalIntervalCalibrator<SeasonalNaiveForecaster>`.

use katgpt_core::conformal::{
    ConformalIntervalCalibrator, DecayUnit, PointForecaster, PredictiveInterval, ResidualMode,
    SeasonalNaiveForecaster, seasonal_naive_floor,
};

/// Channel count (ket scores one scalar signal).
pub const CHANNELS: usize = 1;
/// Max horizon ket queries (single-step decisions).
pub const MAX_HORIZON: usize = 1;
/// Seasonal period (non-seasonal).
pub const SEASON: usize = 1;
/// Ring-buffer capacity per (channel, horizon bucket).
pub const POOL_CAPACITY: usize = 256;
/// Two-tailed miscoverage level (95% interval).
pub const ALPHA: f32 = 0.05;
/// Exponential recency decay rate (0 disables weighting).
const EXP_LAMBDA: f32 = 0.0;

/// Two-tap linear extrapolator weights over the last two projected scores:
/// ŷ_{t+1} ≈ W1·s_t + W0·s_{t−1}. For a locally-sinusoidal signal of period
/// PERIOD_SAMPLES these exact weights (closed-form, modelless) cancel the
/// signal term and leave only noise in the residual pool.
const TAP_W1: f32 = std::f32::consts::SQRT_2;
const TAP_W0: f32 = -1.0;

/// Ket's score source: a fixed two-tap linear extrapolation of the projected
/// sector-score history. Zero-alloc, closed-form, no training.
pub struct KetScoreForecaster;

impl PointForecaster for KetScoreForecaster {
    fn forecast_into(&mut self, delay_state: &[f32], _h: usize, out: &mut f32) {
        match delay_state.len() {
            0 => *out = 0.0,
            1 => *out = delay_state[0],
            _ => {
                let n = delay_state.len();
                *out = TAP_W1 * delay_state[n - 1] + TAP_W0 * delay_state[n - 2];
            }
        }
    }
}

/// Calibrated confidence wrapper over the ket score source.
pub struct KetConfidence {
    calibrator: ConformalIntervalCalibrator<KetScoreForecaster>,
}

impl KetConfidence {
    pub fn new() -> Self {
        Self {
            calibrator: ConformalIntervalCalibrator::new(
                KetScoreForecaster,
                CHANNELS,
                MAX_HORIZON,
                SEASON,
                POOL_CAPACITY,
                EXP_LAMBDA,
                DecayUnit::Step,
                ResidualMode::Paper,
                false,
            ),
        }
    }

    /// Feed one realized score against the forecaster's prediction.
    /// Advances the calibration tick so ring eviction stays FIFO — with a
    /// frozen tick, all entries tie and eviction strips the sorted minimum.
    pub fn observe(&mut self, actual_score: f32, projected: &[f32]) {
        self.calibrator.step();
        self.calibrator
            .observe_and_update(actual_score, projected, 0, 1);
    }

    /// Calibrated interval `[lo, point, hi]` at level 1 − ALPHA. The point
    /// is the current projection mean; quantiles come from the residual pool.
    pub fn interval(&mut self, projected: &[f32]) -> PredictiveInterval {
        let mut point = 0.0_f32;
        KetScoreForecaster.forecast_into(projected, 1, &mut point);
        let mut out = PredictiveInterval::new(0.0, 0.0, 0.0, ALPHA);
        self.calibrator
            .interval_from_point_into(point, 0, 1, ALPHA, &mut out);
        out
    }
}

impl Default for KetConfidence {
    fn default() -> Self {
        Self::new()
    }
}

/// The floor ket must report against: seasonal-naive conformal.
pub fn seasonal_naive_floor_calibrator() -> ConformalIntervalCalibrator<SeasonalNaiveForecaster> {
    ConformalIntervalCalibrator::new(
        seasonal_naive_floor(POOL_CAPACITY),
        CHANNELS,
        MAX_HORIZON,
        SEASON,
        POOL_CAPACITY,
        EXP_LAMBDA,
        DecayUnit::Step,
        ResidualMode::Paper,
        false,
    )
}

#[cfg(test)]
#[path = "tests/conformal.rs"]
mod tests;
