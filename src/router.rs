//! Keyword routing built on the upstream smooth-min primitive.
//!
//! `katgpt_core::smooth_min_similarity` (default-on, GOAT-passed Issue 041)
//! aggregates per-token match cosines into one domain score; the argmax
//! domain wins. Domain taxonomy and expert registry are ket-level domain
//! logic, kept here (never in katgpt-core, per BOUNDARY.md).

use katgpt_core::smooth_min_similarity;

/// Smooth-min sharpness. Upstream's empirically-best operating point.
const SMOOTH_MIN_BETA: f32 = 1.0e4;

/// Match cosine for an exact / substring hit.
const MATCH_HIT: f32 = 1.0;
/// Match cosine for a total miss. Smooth-min then punishes partial banks.
const MATCH_MISS: f32 = 0.0;

/// Question domain. Routing target; selects the expert sector range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Safety,
    Time,
    Resource,
    General,
}

impl Domain {
    pub const COUNT: usize = 4;
    /// Expert sector offset into the projection bank.
    pub const SECTOR_OFFSET: [usize; Domain::COUNT] = [0, 1, 2, 0];
    /// Expert sector span (General sees the whole bank).
    pub const SECTOR_SPAN: [usize; Domain::COUNT] = [1, 1, 1, 4];

    pub fn sector_offset(self) -> usize {
        Self::SECTOR_OFFSET[self as usize]
    }

    pub fn sector_span(self) -> usize {
        Self::SECTOR_SPAN[self as usize]
    }
}

pub const KEYWORDS_SAFETY: [&str; 3] = ["safe", "risk", "harm"];
pub const KEYWORDS_TIME: [&str; 3] = ["now", "urgent", "deadline"];
pub const KEYWORDS_RESOURCE: [&str; 3] = ["cost", "budget", "memory"];
pub const KEYWORDS_GENERAL: [&str; 2] = ["general", "misc"];

const DOMAIN_BANKS: [(Domain, &[&str]); Domain::COUNT] = [
    (Domain::Safety, &KEYWORDS_SAFETY),
    (Domain::Time, &KEYWORDS_TIME),
    (Domain::Resource, &KEYWORDS_RESOURCE),
    (Domain::General, &KEYWORDS_GENERAL),
];

/// Routes query keywords to a domain via smooth-min similarity over the
/// keyword banks. Ties resolve to the lower bank index (deterministic).
pub struct KeywordRouter;

impl KeywordRouter {
    pub fn route(keywords: &[&str]) -> Domain {
        let mut best = (Domain::General, f32::NEG_INFINITY);
        for (domain, bank) in DOMAIN_BANKS {
            // Single scan builds the cosine vector and hit flag; the
            // smooth-min exp chain runs only for hit banks (a miss bank's
            // score is a losing constant). Total miss => General.
            let mut cosines = [MATCH_MISS; KEYWORDS_SAFETY.len()];
            let mut any_hit = false;
            for (i, keyword) in bank.iter().enumerate().take(cosines.len()) {
                if keywords.iter().any(|k| k.contains(keyword)) {
                    cosines[i] = MATCH_HIT;
                    any_hit = true;
                }
            }
            if !any_hit {
                continue;
            }
            let score = smooth_min_similarity(&cosines, SMOOTH_MIN_BETA);
            // `>=` so later banks win exact ties; General still wins when no
            // bank is hit at all.
            if score >= best.1 {
                best = (domain, score);
            }
        }
        best.0
    }
}

#[cfg(test)]
#[path = "tests/router.rs"]
mod tests;
