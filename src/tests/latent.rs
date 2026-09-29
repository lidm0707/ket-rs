#![allow(clippy::unwrap_used)]

use super::*;

const DIMS: [f32; STATE_DIM] = [0.1, -0.2, 0.3, 0.4, 0.0, 0.0];

#[test]
fn rejects_nan_state() {
    let dirty = [f32::NAN, 0.0, 0.0, 0.0];
    let state = LatentState {
        dims: &dirty,
        flags: 0,
    };
    assert!(KetConflictDetector.is_heal_conflicted(&dirty));
    assert!(!KetConflictDetector.is_valid(&state));
}

#[test]
fn rejects_contradictory_flags() {
    let state = LatentState {
        dims: &DIMS,
        flags: FLAG_ARMED | FLAG_DISARMED,
    };
    assert!(!KetConflictDetector.is_valid(&state));
}

#[test]
fn accepts_clean_state() {
    let state = LatentState {
        dims: &DIMS,
        flags: FLAG_SAFE,
    };
    assert!(KetConflictDetector.is_valid(&state));
}
