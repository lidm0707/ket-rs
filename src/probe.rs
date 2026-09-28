//! Urgency tagging via the upstream `IndicatorProbeBank`
//! (katgpt-core::pruners::indicator_probe_bank, default-on, GOAT-passed
//! Plan 320): fixed BLAKE3-committed direction bank, dot + sigmoid,
//! OR-fused firing label.

use katgpt_core::pruners::indicator_probe_bank::{IndicatorLabel, IndicatorProbeBank};

use crate::latent::STATE_DIM;
use crate::question::Urgency;

/// Probe direction rows, one per urgency level (Routine, Elevated, Critical),
/// flattened row-major for the upstream bank constructor.
const PROBE_DIRECTIONS_FLAT: [f32; 3 * STATE_DIM] = [
    0.1, 0.1, 0.1, 0.1, // Routine
    0.5, 0.2, 0.1, 0.0, // Elevated
    0.9, 0.1, 0.0, -0.2, // Critical
];

/// Sigmoid-input thresholds per label; a label fires above its threshold.
const PROBE_THRESHOLDS: [f32; 3] = [0.45, 0.55, 0.75];

/// Bank-level OR-fusion floor: the strongest firing label must also beat this.
const TAU_FIRE: f32 = 0.45;

/// ket's label taxonomy for the bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UrgencyLabel {
    Routine = 0,
    Elevated = 1,
    Critical = 2,
}

impl IndicatorLabel for UrgencyLabel {
    fn as_u8(&self) -> u8 {
        *self as u8
    }

    fn from_u8(d: u8) -> Option<Self> {
        match d {
            0 => Some(Self::Routine),
            1 => Some(Self::Elevated),
            2 => Some(Self::Critical),
            _ => None,
        }
    }

    const COUNT: usize = 3;
}

/// Wrapper owning the constructed upstream bank.
pub struct UrgencyProbe {
    bank: IndicatorProbeBank<UrgencyLabel, STATE_DIM>,
}

impl UrgencyProbe {
    /// Bank::new returns None on malformed input; ket's consts are valid by
    /// construction, so the None arm is unreachable.
    pub fn new() -> Option<Self> {
        let bank =
            IndicatorProbeBank::new(PROBE_DIRECTIONS_FLAT.to_vec(), PROBE_THRESHOLDS.to_vec())?;
        Some(Self { bank })
    }

    /// OR-fused urgency tag for the state.
    pub fn tag(&self, dims: &[f32; STATE_DIM]) -> Urgency {
        let mut scores = [0.0_f32; UrgencyLabel::COUNT];
        self.bank.project_all_into(dims, &mut scores);
        match self.bank.or_fused_fire(&scores, TAU_FIRE) {
            Some(UrgencyLabel::Critical) => Urgency::Critical,
            Some(UrgencyLabel::Elevated) => Urgency::Elevated,
            _ => Urgency::Routine,
        }
    }
}

#[cfg(test)]
#[path = "tests/probe.rs"]
mod tests;
