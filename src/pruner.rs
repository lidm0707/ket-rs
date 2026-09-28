//! Choice filtering: ket implements the upstream `ConstraintPruner` trait
//! (katgpt-core::traits) for typed questions; `BinaryScreeningPruner`
//! adapts it to graded relevance for the valid-only pass.

use katgpt_core::traits::{BinaryScreeningPruner, ConstraintPruner, ScreeningPruner};

use crate::question::ChoiceId;

/// Max tree depth ket will score into.
pub const MAX_DEPTH: usize = 8;
/// Max candidate token index per depth (choice capacity).
pub const MAX_TOKENS_PER_DEPTH: usize = 64;

/// Structural constraint: depth caps, token range, parent-path length.
pub struct DepthConstraintPruner;

impl ConstraintPruner for DepthConstraintPruner {
    fn is_valid(&self, depth: usize, token_idx: usize, parent_tokens: &[usize]) -> bool {
        depth <= MAX_DEPTH && token_idx < MAX_TOKENS_PER_DEPTH && parent_tokens.len() <= depth
    }
}

/// Graded relevance: 1.0 valid, 0.0 pruned — via the upstream adapter.
pub fn depth_screening() -> BinaryScreeningPruner<DepthConstraintPruner> {
    BinaryScreeningPruner(DepthConstraintPruner)
}

/// Score only choices that pass the screener. Zero-alloc: writes survivors
/// to `out`, returns the survivor count.
pub fn prune_choices(
    screener: &dyn ScreeningPruner,
    depth: usize,
    choices: &[ChoiceId],
    parent_tokens: &[usize],
    out: &mut [ChoiceId],
) -> usize {
    let mut n = 0;
    for (token_idx, choice) in choices.iter().enumerate() {
        if n == out.len() {
            break;
        }
        if screener.relevance(depth, token_idx, parent_tokens) > 0.0 {
            out[n] = *choice;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
#[path = "tests/pruner.rs"]
mod tests;
