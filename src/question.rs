//! ket domain types: typed questions, choices, urgency. Enums, never strings.

use crate::latent::STATE_DIM;

/// Identity of a candidate choice. Opaque handle, Copy, ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChoiceId(pub u16);

/// What kind of question is being asked.
#[derive(Debug, Clone, Copy)]
pub enum TypedQuestion<'a> {
    /// Route the state to a domain expert by keywords, then score choices.
    Pick {
        keywords: &'a [&'a str],
        choices: &'a [ChoiceId],
    },
    /// Pick with a per-choice direction row over the raw latent dims —
    /// each choice reads the state through its own evidence profile, so
    /// the argmax is choice-sensitive, not just state-sensitive.
    PickWeighted {
        keywords: &'a [&'a str],
        choices: &'a [ChoiceId],
        directions: &'a [[f32; STATE_DIM]],
    },
    /// Score choices directly against the full direction bank.
    Score { choices: &'a [ChoiceId] },
}

/// Urgency tag resolved from the upstream IndicatorProbeBank OR-fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Urgency {
    Routine,
    Elevated,
    Critical,
}

#[cfg(test)]
#[path = "tests/question.rs"]
mod tests;
