//! Exploration vs selection on upstream primitives:
//! - Thompson exploration: `BayesianFilterArm::thompson_sample`
//!   (katgpt-core::manifold_bandit, Jöhnk's Beta sampler over fastrand).
//! - Conservative selection: `best_belief_score` / `select_best_belief`
//!   (katgpt-core::best_belief, ε-quantile Beta lower bound).

use katgpt_core::best_belief::{best_belief_score, select_best_belief};
use katgpt_core::manifold_bandit::BayesianFilterArm;

/// Deterministic seed for reproducible exploration streams (G1).
pub const EXPLORATION_SEED: u64 = 42;
/// Epsilon for conservative selection.
pub const SELECTION_EPSILON: f32 = 0.05;
/// Belief drift rate per arm (stationary by default).
const ARM_DRIFT_RATE: f32 = 0.01;

/// One Beta-Bernoulli arm per choice; zero-alloc fixed array.
pub struct ExplorationArms<const N: usize> {
    arms: [BayesianFilterArm; N],
}

impl<const N: usize> ExplorationArms<N> {
    pub fn new() -> Self {
        Self {
            arms: std::array::from_fn(|_| BayesianFilterArm::new(ARM_DRIFT_RATE)),
        }
    }

    /// Thompson draw for choice `i` with the deterministic seed.
    pub fn thompson(&self, i: usize) -> f32 {
        let mut rng = fastrand::Rng::with_seed(EXPLORATION_SEED.wrapping_add(i as u64));
        self.arms[i].thompson_sample(&mut rng)
    }

    /// Conjugate update: reward ∈ [0, 1] at choice `i`, step `t`.
    pub fn observe(&mut self, i: usize, reward: f32, step: u64) {
        self.arms[i].update(reward, step);
    }

    /// ε-quantile Beta lower bound for choice `i` (integer round of the
    /// posterior mean × 32 — the upstream score takes (u32, u32) counts).
    pub fn best_belief(&self, i: usize) -> f32 {
        let successes = (self.arms[i].alpha - 1.0).round().max(0.0) as u32;
        let failures = (self.arms[i].beta - 1.0).round().max(0.0) as u32;
        best_belief_score(successes, failures, SELECTION_EPSILON)
    }

    /// Conservative argmax over all arms via the upstream selector.
    pub fn select_conservative(&self) -> usize {
        let candidates: [(u32, u32); N] = std::array::from_fn(|i| {
            let s = (self.arms[i].alpha - 1.0).round().max(0.0) as u32;
            let f = (self.arms[i].beta - 1.0).round().max(0.0) as u32;
            (s, f)
        });
        select_best_belief(&candidates, SELECTION_EPSILON, None)
    }
}

impl<const N: usize> Default for ExplorationArms<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/exploration.rs"]
mod tests;
