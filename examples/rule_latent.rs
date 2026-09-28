//! Rules -> latent -> agent toolcall: no training, no hand-coded dims.
//! The KetDecision is rendered as a structured tool call (name + JSON args)
//! an agent could dispatch — never prose.

use katgpt_core::salience::SalienceDecision;
use ket_rs::decision::{KetDecision, KetEngine, KetQuery};
use ket_rs::latent::{FLAG_SAFE, LatentState, STATE_DIM};
use ket_rs::question::{ChoiceId, TypedQuestion, Urgency};
use ket_rs::score::{KetScorer, SECTOR_COUNT};

/// Evidence bump per fired rule.
const RULE_HIT: f32 = 0.5;
/// Evidence bump for safety-critical rules.
const ALERT_HIT: f32 = 2.5;
/// Evidence ceiling per dim.
const EVIDENCE_MAX: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    Meal,
    Fruit,
}

impl Class {
    const ALL: [Class; 2] = [Class::Meal, Class::Fruit];

    fn dim(self) -> usize {
        match self {
            Class::Meal => DIM_MEAL,
            Class::Fruit => DIM_FRUIT,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Class::Meal => "meal",
            Class::Fruit => "fruit",
        }
    }

    fn choice(self) -> ChoiceId {
        ChoiceId(match self {
            Class::Meal => 0,
            Class::Fruit => 1,
        })
    }
}

/// Evidence dim per class (context dims follow at CLASS_COUNT + n).
const CLASS_COUNT: usize = 2;
const DIM_MEAL: usize = 0;
const DIM_FRUIT: usize = 1;
const DIM_SWEET: usize = CLASS_COUNT;
const DIM_HOT: usize = CLASS_COUNT + 1;

/// One rule: if any cue appears in the text, its dim gains evidence.
struct Rule {
    cues: &'static [&'static str],
    dim: usize,
    name: &'static str,
    hit: f32,
}

const RULES: [Rule; 7] = [
    Rule {
        cues: &["rice", "noodle", "soup", "curry", "pork", "grill"],
        dim: DIM_MEAL,
        name: "cooked-staple",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["banana", "mango", "apple", "berry"],
        dim: DIM_FRUIT,
        name: "raw-fruit",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["sweet", "ripe", "juicy"],
        dim: DIM_SWEET,
        name: "sweet-profile",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["hot", "grill", "soup", "curry"],
        dim: DIM_HOT,
        name: "served-hot",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["raw", "fresh"],
        dim: DIM_FRUIT,
        name: "eaten-raw",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["pie", "dessert"],
        dim: DIM_SWEET,
        name: "dessert-form",
        hit: RULE_HIT,
    },
    Rule {
        cues: &["spoiled", "burnt", "undercooked", "moldy"],
        dim: DIM_MEAL,
        name: "safety-risk",
        hit: ALERT_HIT,
    },
];

fn bump(dims: &mut [f32; STATE_DIM], dim: usize, hit: f32) {
    dims[dim] = (dims[dim] + hit).min(EVIDENCE_MAX);
}

/// Encode text by firing rules. Returns the latent and the fired rule
/// names — the reasons, by construction.
fn encode(text: &str) -> ([f32; STATE_DIM], Vec<&'static str>) {
    let mut dims = [0.0; STATE_DIM];
    let mut reasons = Vec::new();
    for rule in &RULES {
        if rule.cues.iter().any(|c| text.contains(c)) {
            bump(&mut dims, rule.dim, rule.hit);
            reasons.push(rule.name);
        }
    }
    (dims, reasons)
}

/// Rule verdict: argmax evidence across class dims.
fn classify(dims: &[f32; STATE_DIM]) -> Class {
    let mut best = (Class::Meal, f32::NEG_INFINITY);
    for class in Class::ALL {
        let evidence = dims[class.dim()];
        if evidence > best.1 {
            best = (class, evidence);
        }
    }
    best.0
}

/// Tool name from urgency: the action the agent dispatches.
fn tool_name(urgency: Urgency) -> &'static str {
    match urgency {
        Urgency::Routine => "serve_suggestion",
        Urgency::Elevated => "serve_suggestion_now",
        Urgency::Critical => "serve_suggestion_alert",
    }
}

/// One structured tool call: name + JSON args + reasons. No prose.
struct ToolCall {
    name: &'static str,
    args: String,
    reasons: Vec<&'static str>,
}

impl ToolCall {
    fn from_decision(d: &KetDecision, class: Class, reasons: Vec<&'static str>) -> Self {
        let args = match d.emit {
            SalienceDecision::Speak => format!(
                r#"{{"item":"{}","choice":{},"emit":"speak"}}"#,
                class.label(),
                d.choice.0
            ),
            SalienceDecision::Delegate(payload) => format!(
                r#"{{"item":"{}","choice":{},"emit":"delegate","target":{}}}"#,
                class.label(),
                d.choice.0,
                payload.0
            ),
            SalienceDecision::Silent => format!(
                r#"{{"item":"{}","choice":{},"emit":"silent"}}"#,
                class.label(),
                d.choice.0
            ),
        };
        Self {
            name: tool_name(d.urgency),
            args,
            reasons,
        }
    }
}

impl std::fmt::Display for ToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({}) because {:?}", self.name, self.args, self.reasons)
    }
}

fn main() {
    let mut engine = KetEngine::new().expect("engine");
    let mut scorer = KetScorer::new();
    const KEYWORDS: [&str; 1] = ["general"];

    let texts = [
        "hot rice and curry",
        "a raw sweet banana",
        "noodle soup with pork",
        "sweet ripe mango",
        "apple pie",
        "spoiled rice and ripe banana",
        "burnt undercooked pork",
        "burnt moldy sweet dessert",
    ];

    for text in texts {
        let (dims, reasons) = encode(text);
        let class = classify(&dims);
        let projected = scorer.project(&dims);

        let state = LatentState {
            dims: &dims,
            flags: FLAG_SAFE,
        };
        let query = KetQuery {
            state: &state,
            question: TypedQuestion::Pick {
                keywords: &KEYWORDS,
                choices: &[class.choice()],
            },
        };
        let mut choices_out = [ChoiceId(u16::MAX); 1];
        let mut scores_out = [0.0_f32; 1];
        let call = match engine.decide_into(query, &mut choices_out, &mut scores_out) {
            Ok(d) => ToolCall::from_decision(&d, class, reasons),
            Err(e) => ToolCall {
                name: "reject_suggestion",
                args: format!(r#"{{"error":"{e:?}"}}"#),
                reasons,
            },
        };
        println!("{text}: {call} (latent={dims:#?}, sectors={projected:#?} of {SECTOR_COUNT}\n)");
    }
}
