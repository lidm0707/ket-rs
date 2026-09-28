//! Scoring: upstream `SectorProjection` (katgpt-sense, ternary directions,
//! sigmoid dot-products — never softmax) + upstream `SalienceTriGate`
//! (katgpt-core, two stacked sigmoids, GOAT-passed Plan 303, ~9 ns).

use katgpt_core::salience::{SalienceDecision, SalienceTriGate};
use katgpt_sense::sector::SectorProjection;

use crate::latent::STATE_DIM;
use crate::router::Domain;

/// Sector count of the direction bank.
pub const SECTOR_COUNT: usize = 4;

/// Ternary direction bank: row i = "axis i pushes the state".
/// Ternary {-1, 0, +1} per the upstream SIMD rationale (f32-cast once).
const DIRECTIONS: [[i8; STATE_DIM]; SECTOR_COUNT] =
    [[1, 0, 0, -1], [0, 1, 1, 0], [0, 1, 0, 1], [-1, 0, 1, 1]];

/// Tri-gate knob block. Hand-tuned; kept as one named const group.
/// (d_speak, d_delegate, w_z, w_c, beta_speak, beta_delegate,
///  tau_speak, tau_delegate, floor_speak, ceil_delegate)
const GATE_D_SPEAK: [f32; STATE_DIM] = [0.9, 0.1, 0.0, -0.1];
const GATE_D_DELEGATE: [f32; STATE_DIM] = [0.1, 0.8, 0.2, 0.0];
const GATE_W_Z: f32 = 0.0;
const GATE_W_C: f32 = 0.0;
const GATE_BETA_SPEAK: f32 = 6.0;
const GATE_BETA_DELEGATE: f32 = 6.0;
const GATE_TAU_SPEAK: f32 = 0.5;
const GATE_TAU_DELEGATE: f32 = 0.5;
const GATE_FLOOR_SPEAK: f32 = 0.5;
const GATE_CEIL_DELEGATE: f32 = 0.5;

/// ket's scoring stack: projection bank + emit gate. Zero-alloc after init.
pub struct KetScorer {
    projection: SectorProjection<SECTOR_COUNT, STATE_DIM>,
    gate: SalienceTriGate<ChoiceSlot, STATE_DIM>,
}

/// Delegate payload placeholder — the gate is generic over the payload type;
/// ket passes the winning [`ChoiceId`] through it at decide time.
pub type ChoiceSlot = crate::question::ChoiceId;

impl KetScorer {
    pub fn new() -> Self {
        Self {
            projection: SectorProjection::new(DIRECTIONS),
            gate: SalienceTriGate::new(
                GATE_D_SPEAK,
                GATE_D_DELEGATE,
                GATE_W_Z,
                GATE_W_C,
                GATE_BETA_SPEAK,
                GATE_BETA_DELEGATE,
                GATE_TAU_SPEAK,
                GATE_TAU_DELEGATE,
                GATE_FLOOR_SPEAK,
                GATE_CEIL_DELEGATE,
            ),
        }
    }

    /// Projects the state into per-sector scores. Zero-alloc; returns the
    /// internal score array (bounded by the projection's lifetime).
    pub fn project(&mut self, dims: &[f32; STATE_DIM]) -> [f32; SECTOR_COUNT] {
        *self.projection.project(dims)
    }

    /// Mean-projected score for a domain over the projected sectors.
    pub fn domain_score(projected: &[f32; SECTOR_COUNT], domain: Domain) -> f32 {
        let span = domain.sector_span();
        let offset = domain.sector_offset();
        let mut acc = 0.0_f32;
        for i in 0..span {
            acc += projected[offset + i];
        }
        acc / span as f32
    }

    /// Two-stacked-sigmoid ternary emit on the winning choice.
    pub fn gate(
        &self,
        dims: &[f32; STATE_DIM],
        payload: ChoiceSlot,
    ) -> SalienceDecision<ChoiceSlot> {
        self.gate.decide(dims, 0.0, 0.0, payload, 0)
    }
}

impl Default for KetScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/score.rs"]
mod tests;
