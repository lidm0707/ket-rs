#![allow(clippy::unwrap_used)]

use super::*;

/// Deterministic fixture: smooth seasonal signal with mild noise.
fn fixture(t: usize) -> f32 {
    const AMPLITUDE: f32 = 10.0;
    const PERIOD: f32 = 8.0;
    const NOISE: f32 = 0.37;
    const NOISE_SCALE: f32 = 0.1;
    AMPLITUDE * ((t as f32) * std::f32::consts::TAU / PERIOD).sin()
        + NOISE * (((t * 37) % 11) as f32 - 5.0) * NOISE_SCALE
}

#[test]
fn point_forecaster_extrapolates_two_tap() {
    let mut f = KetScoreForecaster;
    let mut out = 0.0;
    f.forecast_into(&[], 1, &mut out);
    assert_eq!(out, 0.0);
    f.forecast_into(&[3.0], 1, &mut out);
    assert_eq!(out, 3.0);
    // Exact on a locally-linear ramp: s = [2, 4] → 1.414·4 − 2.
    f.forecast_into(&[2.0, 4.0], 1, &mut out);
    assert!((out - (TAP_W1 * 4.0 - 2.0)).abs() < 1e-6);
}

#[test]
fn interval_brackets_point() {
    let mut kc = KetConfidence::new();
    for t in 0..64 {
        let projected = [fixture(t), fixture(t + 1)];
        kc.observe(fixture(t + 2), &projected);
    }
    let projected = [fixture(64), fixture(65)];
    let itv = kc.interval(&projected);
    assert!(itv.lower <= itv.point + 1e-4);
    assert!(itv.point <= itv.upper + 1e-4);
    assert!(itv.half_width().is_finite());
}

#[test]
fn coverage_in_band_on_fixture() {
    let mut kc = KetConfidence::new();
    const WARMUP: usize = 128;
    const EVAL: usize = 2_000;
    for t in 0..WARMUP {
        let projected = [fixture(t), fixture(t + 1)];
        kc.observe(fixture(t + 2), &projected);
    }
    let mut hits = 0_u32;
    for t in WARMUP..WARMUP + EVAL {
        let projected = [fixture(t), fixture(t + 1)];
        let itv = kc.interval(&projected);
        if itv.contains(fixture(t + 2)) {
            hits += 1;
        }
        kc.observe(fixture(t + 2), &projected);
    }
    let coverage = hits as f32 / EVAL as f32;
    assert!(
        (0.93..=0.97).contains(&coverage),
        "coverage out of band: {coverage}"
    );
}

#[test]
fn floor_calibrator_constructs_and_covers() {
    let mut floor = seasonal_naive_floor_calibrator();
    for t in 0..64 {
        let history = [fixture(t)];
        floor.observe_and_update(fixture(t + 1), &history, 0, 1);
    }
    let mut itv = PredictiveInterval::new(0.0, 0.0, 0.0, ALPHA);
    floor.interval_into(0, 1, ALPHA, &mut itv);
    assert!(itv.half_width().is_finite());
}
