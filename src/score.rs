//! Scoring rules + stack: upstream `SectorProjection` (katgpt-sense,
//! ternary directions, sigmoid dot-products — never softmax) + upstream
//! `SalienceTriGate` (katgpt-core, two stacked sigmoids, GOAT-passed
//! Plan 303, ~9 ns).
//!
//! Rules are expressed as relations (which dim pushes which sector, how
//! each gate reads the state); matrices/vectors are derived by const fn —
//! nothing is spelled out as a full handcrafted array.

use katgpt_core::salience::{SalienceDecision, SalienceTriGate};
use katgpt_sense::sector::SectorProjection;

use crate::latent::STATE_DIM;
use crate::router::Domain;

/// Sector-count rule: the bank holds one direction per sector.
pub const SECTOR_COUNT: usize = 4;

/// Ternary-push rule: each sector is exactly two ternary pushes.
pub const PUSHES_PER_SECTOR: usize = 2;

/// Latent axis, named by its role in the gate-read rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    SpeakAxis = 0,
    DelegateAxis = 1,
    Support = 2,
    Resist = 3,
    CoinX = 4,
    CoinY = 5,
}

impl Dim {
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Push direction in ternary space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    Push = 1,
    Resist = -1,
}

impl Sign {
    pub(crate) const fn value(self) -> i8 {
        self as i8
    }
}

/// Expert sector; index must stay aligned with [`crate::router::Domain`]
/// sector offsets (Safety=0, Time=1, Resource=2, General sees all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector {
    Safety = 0,
    Time = 1,
    Resource = 2,
    General = 3,
}

impl Sector {
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// One sector rule: these axes push this sector.
pub struct SectorRule {
    pub sector: Sector,
    pub pushes: [(Dim, Sign); PUSHES_PER_SECTOR],
}

const fn rule(sector: Sector, pushes: [(Dim, Sign); PUSHES_PER_SECTOR]) -> SectorRule {
    SectorRule { sector, pushes }
}

/// Relation table: sector = [(dim, sign), (dim, sign)].
/// Safety: SpeakAxis pushes, Resist resists.
/// Time: DelegateAxis + Support push.
/// Resource: DelegateAxis + Resist push.
/// General: SpeakAxis resists, Support pushes.
/// Resource: loot geometry — the signed coin direction (CoinX, CoinY)
/// pushes Resource, so coin bearing shifts the domain score directionally.
const SECTOR_RULES: [SectorRule; SECTOR_COUNT] = [
    rule(
        Sector::Safety,
        [(Dim::SpeakAxis, Sign::Push), (Dim::Resist, Sign::Resist)],
    ),
    rule(
        Sector::Time,
        [(Dim::DelegateAxis, Sign::Push), (Dim::Support, Sign::Push)],
    ),
    rule(
        Sector::Resource,
        [(Dim::CoinX, Sign::Push), (Dim::CoinY, Sign::Push)],
    ),
    rule(
        Sector::General,
        [(Dim::SpeakAxis, Sign::Resist), (Dim::Support, Sign::Push)],
    ),
];

/// Ternary direction bank: row i = "axis i pushes the state".
/// Ternary {-1, 0, +1} per the upstream SIMD rationale (f32-cast once).
pub const DIRECTIONS: [[i8; STATE_DIM]; SECTOR_COUNT] = ternary_bank();

const fn ternary_bank() -> [[i8; STATE_DIM]; SECTOR_COUNT] {
    let mut dirs = [[0i8; STATE_DIM]; SECTOR_COUNT];
    let mut s = 0;
    while s < SECTOR_COUNT {
        let mut p = 0;
        while p < PUSHES_PER_SECTOR {
            let (dim, sign) = SECTOR_RULES[s].pushes[p];
            dirs[s][dim.index()] = sign.value();
            p += 1;
        }
        s += 1;
    }
    dirs
}

/// One gate read: this axis is read with this weight.
pub struct GateRead {
    pub dim: Dim,
    pub weight: f32,
}

const fn read(dim: Dim, weight: f32) -> GateRead {
    GateRead { dim, weight }
}

/// Gate-read rule: speak tracks SpeakAxis strongly, DelegateAxis weakly,
/// resists Resist.
const SPEAK_READS: [GateRead; 3] = [
    read(Dim::SpeakAxis, 0.9),
    read(Dim::DelegateAxis, 0.1),
    read(Dim::Resist, -0.1),
];
/// Gate-read rule: delegate tracks DelegateAxis strongly, SpeakAxis/Support
/// weakly.
const DELEGATE_READS: [GateRead; 3] = [
    read(Dim::SpeakAxis, 0.1),
    read(Dim::DelegateAxis, 0.8),
    read(Dim::Support, 0.2),
];

/// Sparse reads derived into full gate direction vectors (unlisted dims = 0).
pub const D_SPEAK: [f32; STATE_DIM] = read_vector(&SPEAK_READS);
pub const D_DELEGATE: [f32; STATE_DIM] = read_vector(&DELEGATE_READS);

pub(crate) const fn read_vector(reads: &[GateRead]) -> [f32; STATE_DIM] {
    let mut v = [0.0_f32; STATE_DIM];
    let mut i = 0;
    while i < reads.len() {
        v[reads[i].dim.index()] = reads[i].weight;
        i += 1;
    }
    v
}

/// Symmetry rule: one shared sigmoid sharpness for both stacks.
pub const BETA: f32 = 6.0;

/// Midpoint rule: thresholds sit at the sigmoid midpoint.
pub const MIDPOINT: f32 = 0.5;

/// Neutral-evidence rule: z/c probes carry no weight.
pub const W_Z: f32 = 0.0;
pub const W_C: f32 = 0.0;

/// ket's scoring stack: projection bank + emit gate. Zero-alloc after init.
pub struct KetScorer {
    projection: SectorProjection<SECTOR_COUNT, STATE_DIM>,
    gate: SalienceTriGate<ChoiceSlot, STATE_DIM>,
}

/// Delegate payload placeholder — the gate is generic over the payload type;
/// ket passes the winning [`ChoiceId`] through it at decide time.
pub type ChoiceSlot = crate::question::ChoiceId;

impl KetScorer {
    /// Default rule bank (see the rules above).
    pub fn new() -> Self {
        Self::from_parts(DIRECTIONS, D_SPEAK, D_DELEGATE)
    }

    /// Builder: start from the default rules, override what you need.
    pub fn builder() -> KetScorerBuilder {
        KetScorerBuilder {
            directions: DIRECTIONS,
            speak: D_SPEAK,
            delegate: D_DELEGATE,
        }
    }

    fn from_parts(
        directions: [[i8; STATE_DIM]; SECTOR_COUNT],
        d_speak: [f32; STATE_DIM],
        d_delegate: [f32; STATE_DIM],
    ) -> Self {
        Self {
            projection: SectorProjection::new(directions),
            gate: SalienceTriGate::new(
                d_speak, d_delegate, W_Z, W_C, BETA, BETA, MIDPOINT, MIDPOINT, MIDPOINT, MIDPOINT,
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

/// Builder over the default rules: rebind sector pushes and gate reads
/// without touching the lib. Unset fields keep the rule default.
pub struct KetScorerBuilder {
    directions: [[i8; STATE_DIM]; SECTOR_COUNT],
    speak: [f32; STATE_DIM],
    delegate: [f32; STATE_DIM],
}

impl KetScorerBuilder {
    /// Replaces one sector's ternary pushes.
    pub fn sector(mut self, sector: Sector, pushes: [(Dim, Sign); PUSHES_PER_SECTOR]) -> Self {
        let row = &mut self.directions[sector.index()];
        *row = [0; STATE_DIM];
        for (dim, sign) in pushes {
            row[dim.index()] = sign.value();
        }
        self
    }

    /// Replaces the speak gate's sparse reads (unlisted dims = 0).
    pub fn speak_reads(mut self, reads: &[GateRead]) -> Self {
        self.speak = read_vector(reads);
        self
    }

    /// Replaces the delegate gate's sparse reads (unlisted dims = 0).
    pub fn delegate_reads(mut self, reads: &[GateRead]) -> Self {
        self.delegate = read_vector(reads);
        self
    }

    /// One-axis tweak on the speak gate.
    pub fn speak(mut self, dim: Dim, weight: f32) -> Self {
        self.speak[dim.index()] = weight;
        self
    }

    /// One-axis tweak on the delegate gate.
    pub fn delegate(mut self, dim: Dim, weight: f32) -> Self {
        self.delegate[dim.index()] = weight;
        self
    }

    pub fn build(self) -> KetScorer {
        KetScorer::from_parts(self.directions, self.speak, self.delegate)
    }
}

#[cfg(test)]
#[path = "tests/score.rs"]
mod tests;
