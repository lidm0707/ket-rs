//! Zero-copy state view + semantic conflict validation.
//!
//! Conflict checking delegates to the upstream
//! `katgpt_core::speculative::types::HealConflictDetector` trait
//! (heal_validation, default-on upstream, GOAT-passed Issue 133).

/// Fixed state dimensionality for all ket banks and projections.
pub const STATE_DIM: usize = 4;

pub const FLAG_ARMED: u8 = 1 << 0;
pub const FLAG_SAFE: u8 = 1 << 1;
pub const FLAG_DISARMED: u8 = 1 << 2;

#[cfg(feature = "ket")]
use katgpt_core::speculative::types::HealConflictDetector;
/// Zero-copy view of program state.
#[derive(Debug, Clone, Copy)]
pub struct LatentState<'a> {
    pub dims: &'a [f32],
    pub flags: u8,
}

/// Rejects semantically impossible states before scoring:
/// NaN/Inf dims (upstream trait) + contradictory flags (ket-level).
#[cfg(feature = "ket")]
pub struct KetConflictDetector;

#[cfg(feature = "ket")]
impl HealConflictDetector for KetConflictDetector {
    fn is_heal_conflicted(&self, healed_state: &[f32]) -> bool {
        healed_state.iter().any(|d| !d.is_finite())
    }
}

#[cfg(feature = "ket")]
impl KetConflictDetector {
    /// Full validation: dims via the upstream trait, flags via ket law.
    pub fn is_valid(&self, state: &LatentState<'_>) -> bool {
        if state.flags & FLAG_ARMED != 0 && state.flags & FLAG_DISARMED != 0 {
            return false;
        }
        !self.is_heal_conflicted(state.dims)
    }
}

#[cfg(test)]
#[cfg(feature = "ket")]
#[path = "tests/latent.rs"]
mod tests;
