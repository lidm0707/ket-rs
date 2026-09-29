//! The ket pipeline: validate → route → project → prune → score → gate →
//! tag → (confidence). Zero-alloc hot path: all buffers caller-owned.

use katgpt_core::salience::SalienceDecision;

use crate::latent::{KetConflictDetector, LatentState, STATE_DIM};
use crate::probe::UrgencyProbe;
use crate::pruner::{depth_screening, prune_choices};
use crate::question::{ChoiceId, TypedQuestion, Urgency};
use crate::router::{Domain, KeywordRouter};
use crate::score::{KetScorer, SECTOR_COUNT};

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
pub struct DecisionPlan<'a> {
    domain: Domain,
    candidates: [ChoiceId; MAX_PLAN_CHOICES],
    n: usize,
    dirs: Option<&'a [[f32; STATE_DIM]]>,
}

impl KetEngine {
    /// Creates the engine over the default rule bank
    /// ([`crate::score`]). Returns None only if the urgency bank fails to
    /// construct (unreachable with ket's committed consts).
    pub fn new() -> Option<Self> {
        Self::with_scorer(KetScorer::new())
    }

    /// Creates the engine over a custom scorer — use [`KetScorer::builder`]
    /// to override the default rules.
    pub fn with_scorer(scorer: KetScorer) -> Option<Self> {
        Some(Self {
            scorer,
            probe: UrgencyProbe::new(),
            detector: KetConflictDetector,
        })
    }

    /// Plan phase: validate state, route domain, prune choices. Runs once
    /// per question; see [`DecisionPlan::decide_into`] for the per-tick path.
    pub fn plan<'a>(&self, query: KetQuery<'a>) -> Result<DecisionPlan<'a>, KetError> {
        if !self.detector.is_valid(query.state) {
            return Err(KetError::ConflictedState);
        }
        let (domain, choices, dirs) = match query.question {
            TypedQuestion::Score { choices } => (Domain::General, choices, None),
            TypedQuestion::Pick { keywords, choices } => {
                (KeywordRouter::route(keywords), choices, None)
            }
            TypedQuestion::PickWeighted {
                keywords,
                choices,
                directions,
            } => (KeywordRouter::route(keywords), choices, Some(directions)),
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
            dirs,
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

    fn raw_dims(&self, state: &LatentState<'_>) -> [f32; STATE_DIM] {
        let n = STATE_DIM.min(state.dims.len());
        let mut dims = [0.0_f32; STATE_DIM];
        dims[..n].copy_from_slice(&state.dims[..n]);
        dims
    }

    fn score_state(&mut self, state: &LatentState<'_>) -> [f32; SECTOR_COUNT] {
        self.scorer.project(&self.raw_dims(state))
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

impl<'a> DecisionPlan<'a> {
    /// Per-tick hot path: project, score, gate, tag. No routing, no string
    /// work, no allocation. With weighted choices, each candidate gets
    /// state_score/2 + logistic(dot(dir, dims))/2 and the argmax wins.
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
        let dims = engine.raw_dims(state);
        let projected = engine.score_state(state);
        let state_score = logistic(KetScorer::domain_score(&projected, self.domain));
        let mut best = (self.candidates[0], f32::NEG_INFINITY);
        for (i, slot) in scores_out[..self.n].iter_mut().enumerate() {
            let choice = self.candidates[i];
            let s = match self.dirs {
                Some(dirs) => {
                    CHOICE_STATE_WEIGHT * state_score
                        + CHOICE_DIR_WEIGHT * logistic(dot(&dirs[i], &dims))
                }
                None => state_score,
            };
            *slot = s;
            if s > best.1 {
                best = (choice, s);
            }
        }
        let (urgency, emit) = engine.gate_probe(&dims, best.0);
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

/// Blend: weighted picks split their score evenly between state evidence
/// (domain projection) and per-choice direction fit.
const CHOICE_STATE_WEIGHT: f32 = 0.5;
const CHOICE_DIR_WEIGHT: f32 = 0.5;

/// Logistic over the shared sigmoid sharpness — same operating point as
/// the gate stack.
fn logistic(x: f32) -> f32 {
    1.0 / (1.0 + (-crate::score::BETA * x).exp())
}

fn dot(a: &[f32; STATE_DIM], b: &[f32; STATE_DIM]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
#[path = "tests/decision.rs"]
mod tests;
