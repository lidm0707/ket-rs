//! The ket pipeline: validate → route → project → prune → score → gate →
//! tag → (confidence). Zero-alloc hot path: all buffers caller-owned.

use katgpt_core::salience::SalienceDecision;

use crate::latent::{KetConflictDetector, LatentState, STATE_DIM};
use crate::probe::UrgencyProbe;
use crate::pruner::{depth_screening, prune_choices};
use crate::question::{ChoiceId, TypedQuestion, Urgency};
use crate::router::{Domain, KeywordRouter};
use crate::score::KetScorer;

/// Structured decision output. No prose, ever.
#[derive(Debug, Clone, PartialEq)]
pub struct KetDecision {
    pub choice: ChoiceId,
    pub urgency: Urgency,
    pub emit: SalienceDecision<ChoiceId>,
}

/// Errors: impossible states or starved candidate sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KetError {
    ConflictedState,
    NoValidChoices,
}

/// The engine. Owns the stack primitives; zero-alloc after construction.
pub struct KetEngine {
    scorer: KetScorer,
    probe: Option<UrgencyProbe>,
    detector: KetConflictDetector,
}

/// Max choices a plan can carry (fixed array — zero alloc).
pub const MAX_PLAN_CHOICES: usize = 16;

/// Route + prune result, computed once per question. The decide step then
/// runs at pure projection speed per tick — string routing never re-runs.
#[derive(Debug, Clone, Copy)]
pub struct DecisionPlan {
    domain: Domain,
    candidates: [ChoiceId; MAX_PLAN_CHOICES],
    n: usize,
}

impl KetEngine {
    /// Returns None only if the urgency bank fails to construct (unreachable
    /// with ket's committed consts).
    pub fn new() -> Option<Self> {
        Some(Self {
            scorer: KetScorer::new(),
            probe: UrgencyProbe::new(),
            detector: KetConflictDetector,
        })
    }

    /// Plan phase: validate state, route domain, prune choices. Runs once
    /// per question; see [`DecisionPlan::decide_into`] for the per-tick path.
    pub fn plan(&self, query: KetQuery<'_>) -> Result<DecisionPlan, KetError> {
        if !self.detector.is_valid(query.state) {
            return Err(KetError::ConflictedState);
        }
        let (domain, choices) = match query.question {
            TypedQuestion::Score { choices } => (Domain::General, choices),
            TypedQuestion::Pick { keywords, choices } => (KeywordRouter::route(keywords), choices),
        };
        let mut candidates = [ChoiceId(u16::MAX); MAX_PLAN_CHOICES];
        let n = prune_choices(&depth_screening(), 1, choices, &[], &mut candidates);
        if n == 0 {
            return Err(KetError::NoValidChoices);
        }
        Ok(DecisionPlan {
            domain,
            candidates,
            n,
        })
    }

    /// Convenience one-shot: plan + decide. Zero-alloc; `choices_out` only
    /// receives the pruned candidate list.
    pub fn decide_into(
        &mut self,
        query: KetQuery<'_>,
        choices_out: &mut [ChoiceId],
        scores_out: &mut [f32],
    ) -> Result<KetDecision, KetError> {
        let plan = self.plan(query)?;
        let n = plan.n.min(choices_out.len());
        choices_out[..n].copy_from_slice(&plan.candidates[..n]);
        plan.decide_into(self, query.state, scores_out)
    }

    fn score_state(&mut self, state: &LatentState<'_>) -> [f32; STATE_DIM] {
        let mut dims = [0.0_f32; STATE_DIM];
        dims.copy_from_slice(&state.dims[..STATE_DIM.min(state.dims.len())]);
        self.scorer.project(&dims)
    }

    fn gate_probe(
        &self,
        dims: &[f32; STATE_DIM],
        best: ChoiceId,
    ) -> (Urgency, SalienceDecision<ChoiceId>) {
        let urgency = self
            .probe
            .as_ref()
            .map(|p| p.tag(dims))
            .unwrap_or(Urgency::Routine);
        (urgency, self.scorer.gate(dims, best))
    }
}

impl DecisionPlan {
    /// Per-tick hot path: project, score, gate, tag. No routing, no string
    /// work, no allocation.
    pub fn decide_into(
        &self,
        engine: &mut KetEngine,
        state: &LatentState<'_>,
        scores_out: &mut [f32],
    ) -> Result<KetDecision, KetError> {
        if !engine.detector.is_valid(state) {
            return Err(KetError::ConflictedState);
        }
        if scores_out.len() < self.n {
            return Err(KetError::NoValidChoices);
        }
        let projected = engine.score_state(state);
        let score = KetScorer::domain_score(&projected, self.domain);
        let mut best = (self.candidates[0], f32::NEG_INFINITY);
        for (i, slot) in scores_out[..self.n].iter_mut().enumerate() {
            *slot = score;
            let choice = self.candidates[i];
            if score > best.1 {
                best = (choice, score);
            }
        }
        let (urgency, emit) = engine.gate_probe(&projected, best.0);
        Ok(KetDecision {
            choice: best.0,
            urgency,
            emit,
        })
    }

    pub fn domain(&self) -> Domain {
        self.domain
    }

    pub fn candidates(&self) -> &[ChoiceId] {
        &self.candidates[..self.n]
    }
}

impl Default for KetEngine {
    fn default() -> Self {
        Self::new().expect("KetEngine construction is infallible with committed consts")
    }
}

/// Borrowing query view: (state, typed question). No ownership, no clone.
#[derive(Debug, Clone, Copy)]
pub struct KetQuery<'a> {
    pub state: &'a LatentState<'a>,
    pub question: TypedQuestion<'a>,
}

#[cfg(test)]
#[path = "tests/decision.rs"]
mod tests;
