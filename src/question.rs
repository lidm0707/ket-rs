//! ket domain types: typed questions, choices, urgency. Enums, never strings.

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
