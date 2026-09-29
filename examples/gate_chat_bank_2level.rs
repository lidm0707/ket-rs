//! Susutaku-chan: a dioxus-web customer-reply chat backed by ket.
//!
//! Two-level quest flow — every bot turn ends with a question, then waits
//! for the user's next interaction:
//!
//! - Level 1 (`Welcome` quest): greets and asks what the contact is about.
//!   Cues in the user's reply route into a Level 2 quest.
//! - Level 2 (`Billing` / `Technical` quests): handles the topic, asks its
//!   own follow-up, waits again; closing cues end the chain (`Close`).
//!
//! Cues fire evidence into latent dims; the KetDecision gates each reply
//! (choice + urgency). Grammar cues add concrete fixes as a side channel.

use dioxus::prelude::*;
use ket_rs::decision::{KetDecision, KetEngine, KetQuery};
use ket_rs::latent::{FLAG_SAFE, LatentState, STATE_DIM};
use ket_rs::question::{ChoiceId, TypedQuestion, Urgency};

/// Evidence bump per fired routing cue.
const ROUTE_HIT: f32 = 0.5;
/// Evidence bump for negative-sentiment cues.
const NEGATIVE_HIT: f32 = 1.5;
/// Evidence bump per fired grammar cue.
const GRAMMAR_HIT: f32 = 1.0;
/// Evidence ceiling per dim.
const EVIDENCE_MAX: f32 = 3.0;

const KEYWORDS: [&str; 1] = ["support"];
const CHOICES_PER_QUEST: usize = 1;

// ---------------------------------------------------------------------------
// Quests: every turn asks, then waits for user interaction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Quest {
    /// Level 1: route the customer into a topic.
    Welcome,
    /// Level 2: billing topic.
    Billing,
    /// Level 2: technical topic.
    Technical,
    /// Chain finished.
    Close,
}

impl Quest {
    const ALL: [Quest; 4] = [
        Quest::Welcome,
        Quest::Billing,
        Quest::Technical,
        Quest::Close,
    ];

    fn dim(self) -> usize {
        match self {
            Quest::Welcome => DIM_WELCOME,
            Quest::Billing => DIM_BILLING,
            Quest::Technical => DIM_TECHNICAL,
            Quest::Close => DIM_CLOSE,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Quest::Welcome => "welcome",
            Quest::Billing => "billing",
            Quest::Technical => "technical",
            Quest::Close => "close",
        }
    }

    fn choice(self) -> ChoiceId {
        ChoiceId(self as u16)
    }

    /// Susutaku-chan's line for arriving in this quest. Always ends with a
    /// question — the bot then waits for the user.
    fn prompt(self) -> &'static str {
        match self {
            Quest::Welcome => {
                "Susutaku-chan: Hello! I'm Susutaku-chan \u{1f338} Is your contact about billing or a technical problem?"
            }
            Quest::Billing => BILLING_STEPS[0],
            Quest::Technical => TECH_STEPS[0],
            Quest::Close => {
                "Susutaku-chan: Thanks! I've logged everything. Is there anything else I can help with?"
            }
        }
    }
}

/// Billing quest follow-ups: one per user reply, then the quest closes.
const BILLING_STEPS: [&str; 3] = [
    "Susutaku-chan: Got it \u{2014} billing it is. Could you tell me your order ID or the date of the charge?",
    "Susutaku-chan: Thanks! Was it a duplicate charge, a wrong amount, or something else?",
    "Susutaku-chan: I've flagged this to our billing team \u{2014} they'll reply within 24h. Does that work for you?",
];

/// Technical quest follow-ups: one per user reply, then the quest closes.
const TECH_STEPS: [&str; 3] = [
    "Susutaku-chan: Sorry about the trouble! What exactly happens \u{2014} an error message, a crash, or something slow?",
    "Susutaku-chan: Got it. Which device and app version are you using?",
    "Susutaku-chan: I've logged the details for our engineers. Could you try reinstalling and tell me if it persists?",
];

const CLASS_COUNT: usize = Quest::ALL.len();
const DIM_WELCOME: usize = 0;
const DIM_BILLING: usize = 1;
const DIM_TECHNICAL: usize = 2;
const DIM_CLOSE: usize = 3;
const DIM_URGENT: usize = CLASS_COUNT;

/// Cues that route the Level 1 question into the billing quest.
const BILLING_CUES: &[&str] = &[
    "billing",
    "price",
    "cost",
    "charge",
    "refund",
    "invoice",
    "payment",
    "money",
    "plan",
    "subscribe",
    "card",
    "how much",
];

/// Cues that route the Level 1 question into the technical quest.
const TECH_CUES: &[&str] = &[
    "broken",
    "bug",
    "error",
    "crash",
    "not working",
    "doesn't work",
    "login",
    "password",
    "slow",
    "freeze",
    "api",
    "fail",
    "problem",
];

/// Cues that end the chain from any quest.
const CLOSE_CUES: &[&str] = &[
    "bye",
    "goodbye",
    "that's all",
    "no thanks",
    "done",
    "nothing else",
];

/// Cues confirming inside a Level 2 quest (advances the follow-up).
const CONFIRM_CUES: &[&str] = &["yes", "sure", "correct", "right", "ok", "okay", "confirmed"];

/// Negative cues raise urgency inside any quest.
const NEGATIVE_CUES: &[&str] = &[
    "angry",
    "terrible",
    "worst",
    "unacceptable",
    "awful",
    "urgent",
    "asap",
];

fn cue_hit(lowered: &str, cues: &[&str]) -> bool {
    cues.iter().any(|c| lowered.contains(c))
}

/// One conversation turn: where the quest chain lands and what Susutaku-chan
/// asks next. The prompt always ends with a question — the app then waits.
struct Turn {
    quest: Quest,
    step: usize,
    prompt: &'static str,
}

fn close_turn() -> Turn {
    Turn {
        quest: Quest::Close,
        step: 0,
        prompt: Quest::Close.prompt(),
    }
}

fn step_turn(quest: Quest, step: usize) -> Turn {
    let steps = match quest {
        Quest::Billing => BILLING_STEPS,
        _ => TECH_STEPS,
    };
    Turn {
        quest,
        step,
        prompt: steps[step],
    }
}

/// Level transition: current quest + step + user text -> next turn.
fn transition(current: Quest, step: usize, lowered: &str) -> Turn {
    match current {
        Quest::Welcome => {
            if cue_hit(lowered, BILLING_CUES) {
                step_turn(Quest::Billing, 0)
            } else if cue_hit(lowered, TECH_CUES) {
                step_turn(Quest::Technical, 0)
            } else {
                Turn {
                    quest: Quest::Welcome,
                    step: 0,
                    prompt: Quest::Welcome.prompt(),
                }
            }
        }
        Quest::Billing | Quest::Technical => {
            if cue_hit(lowered, CLOSE_CUES) || cue_hit(lowered, CONFIRM_CUES) {
                close_turn()
            } else {
                let next_step = step + 1;
                let steps_len = match current {
                    Quest::Billing => BILLING_STEPS.len(),
                    _ => TECH_STEPS.len(),
                };
                if next_step < steps_len {
                    step_turn(current, next_step)
                } else {
                    close_turn()
                }
            }
        }
        Quest::Close => {
            if cue_hit(lowered, CLOSE_CUES) {
                close_turn()
            } else {
                Turn {
                    quest: Quest::Welcome,
                    step: 0,
                    prompt: Quest::Welcome.prompt(),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Grammar side channel (level 2 details)
// ---------------------------------------------------------------------------

/// One grammar rule: a cue marks a violation; the fix is the advice shown.
struct GrammarRule {
    cues: &'static [&'static str],
    name: &'static str,
    fix: &'static str,
}

const GRAMMAR_RULES: [GrammarRule; 19] = [
    GrammarRule {
        cues: &[" i ", " i'", " i."],
        name: "capital-i",
        fix: "The pronoun \"I\" is always capitalised.",
    },
    GrammarRule {
        cues: &["a apple", "a orange", "a egg", "a hour", "a email"],
        name: "article-an",
        fix: "Use \"an\" before a vowel sound: an apple, an email, an hour.",
    },
    GrammarRule {
        cues: &["an book", "an car", "an pen", "an user", "an plan"],
        name: "article-a",
        fix: "Use \"a\" before a consonant sound: a book, a user, a plan.",
    },
    GrammarRule {
        cues: &[
            "he go", "she go", "it go", "he have", "she have", "he dont", "she dont",
        ],
        name: "third-person-s",
        fix: "Third-person singular takes -s: he goes, she has, she doesn't.",
    },
    GrammarRule {
        cues: &["i are", "you is", "we is", "they is", "he am"],
        name: "subject-verb-be",
        fix: "Match the be-verb to the subject: I am / you are / he is.",
    },
    GrammarRule {
        cues: &["very much like", "did finished", "was went", "is went"],
        name: "double-verb",
        fix: "Keep one finite verb: \"did finish\", \"went\".",
    },
    GrammarRule {
        cues: &["the the", "a a", "to to", "is is", "and and"],
        name: "repeated-word",
        fix: "Remove the duplicated word.",
    },
    GrammarRule {
        cues: &["pls", "plz", "u ", "ur ", "thx", "cus", "gonna tell"],
        name: "informal-register",
        fix: "In customer mail, prefer full words: please, you, your, thanks.",
    },
    GrammarRule {
        cues: &["no have", "not have", "no want"],
        name: "negation-do",
        fix: "Negate with do-support: \"I don't have\", \"I don't want\".",
    },
    GrammarRule {
        cues: &["more better", "more faster", "more easier"],
        name: "double-comparative",
        fix: "Use one comparative form: \"better\", \"faster\", \"easier\".",
    },
    GrammarRule {
        cues: &["?  ", "!!", "??", "...?"],
        name: "punctuation",
        fix: "One punctuation mark is enough for polite business mail.",
    },
    GrammarRule {
        cues: &["i want that you", "please you to"],
        name: "want-clause",
        fix: "Use \"I'd like you to...\" or \"Could you...\" instead.",
    },
    GrammarRule {
        cues: &[
            "since two years",
            "since 2 years",
            "from monday",
            "since last week to",
        ],
        name: "since-for",
        fix: "Use \"for\" with a length of time (for two years) and \"since\" with a point (since Monday).",
    },
    GrammarRule {
        cues: &[
            "informations",
            "advices",
            "feedbacks",
            "softwares",
            "equipments",
        ],
        name: "uncountable-plural",
        fix: "These nouns are uncountable: information, advice, feedback, software, equipment.",
    },
    GrammarRule {
        cues: &["discuss about", "enter into", "married with", "consist on"],
        name: "preposition",
        fix: "No preposition needed: discuss it, enter it; or married to, consist of.",
    },
    GrammarRule {
        cues: &[
            "i am agree",
            "i am looking forward to hear",
            "look forward to hear",
        ],
        name: "verb-pattern",
        fix: "\"I agree\" (no am); \"looking forward to hearing\" (to + -ing here).",
    },
    GrammarRule {
        cues: &[
            "yesterday i have",
            "today i have sent it yesterday",
            "last week i have",
        ],
        name: "past-simple",
        fix: "Finished past time (yesterday, last week) takes past simple, not present perfect.",
    },
    GrammarRule {
        cues: &[
            "will can",
            "will must",
            "will should",
            "can able",
            "will able",
        ],
        name: "double-modal",
        fix: "Use one modal only: \"will be able to\", \"can\".",
    },
    GrammarRule {
        cues: &[
            "every customers",
            "each users",
            "all of user",
            "one of my friend",
        ],
        name: "quantifier-number",
        fix: "Quantifier agreement: every customer, each user, one of my friends.",
    },
];

// ---------------------------------------------------------------------------
// Encoding + ket
// ---------------------------------------------------------------------------

fn bump(dims: &mut [f32; STATE_DIM], dim: usize, hit: f32) {
    dims[dim] = (dims[dim] + hit).min(EVIDENCE_MAX);
}

/// Encode the turn: the next quest gets evidence, negative cues raise the
/// urgency dim, grammar cues add detail. Returns latent + fired reasons.
fn encode(next: Quest, text: &str) -> ([f32; STATE_DIM], Vec<&'static str>) {
    let lowered = text.to_ascii_lowercase();
    let mut dims = [0.0; STATE_DIM];
    let mut reasons = vec![next.name()];

    bump(&mut dims, next.dim(), ROUTE_HIT);
    if cue_hit(&lowered, NEGATIVE_CUES) {
        bump(&mut dims, DIM_URGENT, NEGATIVE_HIT);
        reasons.push("negative-sentiment");
    }
    for rule in &GRAMMAR_RULES {
        if rule.cues.iter().any(|c| lowered.contains(c)) {
            bump(&mut dims, DIM_URGENT, GRAMMAR_HIT);
            reasons.push(rule.name);
        }
    }
    (dims, reasons)
}

/// Grammar fixes for the text, in bank order.
fn grammar_fixes(text: &str) -> Vec<&'static str> {
    let lowered = text.to_ascii_lowercase();
    GRAMMAR_RULES
        .iter()
        .filter(|r| r.cues.iter().any(|c| lowered.contains(c)))
        .map(|r| r.fix)
        .collect()
}

fn urgency_prefix(urgency: Urgency) -> &'static str {
    match urgency {
        Urgency::Routine => "",
        Urgency::Elevated => "[priority] ",
        Urgency::Critical => "[escalated] ",
    }
}

/// Compose the bot turn: ket-gated prompt + grammar notes + a decision
/// footer (fired rules, gate score) so the ket decision stays inspectable.
fn bot_turn(
    d: &KetDecision,
    prompt: &str,
    fixes: &[&'static str],
    reasons: &[&'static str],
    score: f32,
) -> String {
    let mut out = String::from(urgency_prefix(d.urgency));
    out.push_str(prompt);
    for fix in fixes {
        out.push_str("\n\u{1f4d6} Grammar tip: ");
        out.push_str(fix);
    }
    out.push_str("\n\u{1f9ed} ");
    out.push_str(&format!("rules: {:?} · score: {:.2}", reasons, score));
    out
}

// ---------------------------------------------------------------------------
// Dioxus web app
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Message {
    from_user: bool,
    text: String,
}

const INPUT_PLACEHOLDER: &str = "Type your English message to Susutaku-chan...";
const SEND_LABEL: &str = "Send";
const TITLE: &str = "Susutaku-chan — 2-level quest chat (ket)";

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    launch(app);
}

fn app() -> Element {
    let mut messages = use_signal(|| {
        vec![Message {
            from_user: false,
            text: Quest::Welcome.prompt().to_string(),
        }]
    });
    let mut draft = use_signal(String::new);
    let mut quest = use_signal(|| Quest::Welcome);
    let mut step = use_signal(|| 0_usize);
    let mut engine = use_signal(KetEngine::new);

    let mut send = move || {
        let text = draft.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        messages.write().push(Message {
            from_user: true,
            text: text.clone(),
        });
        draft.set(String::new());

        let current = *quest.read();
        let lowered = text.to_ascii_lowercase();
        let turn = transition(current, *step.read(), &lowered);
        quest.set(turn.quest);
        step.set(turn.step);

        let fixes = grammar_fixes(&text);
        let (dims, reasons) = encode(turn.quest, &text);
        let state = LatentState {
            dims: &dims,
            flags: FLAG_SAFE,
        };
        let query = KetQuery {
            state: &state,
            question: TypedQuestion::Pick {
                keywords: &KEYWORDS,
                choices: &[turn.quest.choice()],
            },
        };
        let mut choices_out = [ChoiceId(u16::MAX); CHOICES_PER_QUEST];
        let mut scores_out = [0.0_f32; CHOICES_PER_QUEST];

        let mut engine_guard = engine.write();
        let answer = match engine_guard.as_mut() {
            Some(eng) => match eng.decide_into(query, &mut choices_out, &mut scores_out) {
                Ok(d) => bot_turn(&d, turn.prompt, &fixes, &reasons, scores_out[0]),
                Err(e) => format!("Susutaku-chan: gate rejected ({e:?})"),
            },
            None => "Susutaku-chan: engine unavailable (gate offline)".to_string(),
        };
        messages.write().push(Message {
            from_user: false,
            text: answer,
        });
    };

    rsx! {
        div { style: "max-width:640px;margin:2rem auto;font-family:sans-serif;",
            h1 { "{TITLE}" }
            div {
                style: "border:1px solid #ccc;border-radius:8px;padding:1rem;height:420px;overflow-y:auto;display:flex;flex-direction:column;gap:0.5rem;",
                for msg in messages.read().iter() {
                    div {
                        style: if msg.from_user { "align-self:flex-end;background:#d7e8ff;padding:0.5rem 0.75rem;border-radius:12px;max-width:80%;" }
                               else { "align-self:flex-start;background:#e8ffe0;padding:0.5rem 0.75rem;border-radius:12px;max-width:80%;white-space:pre-wrap;" },
                        "{msg.text}"
                    }
                }
            }
            div { style: "display:flex;gap:0.5rem;margin-top:0.75rem;",
                input {
                    style: "flex:1;padding:0.5rem;",
                    value: draft,
                    placeholder: INPUT_PLACEHOLDER,
                    oninput: move |e: Event<FormData>| draft.set(e.value()),
                    onkeydown: move |e: Event<KeyboardData>| {
                        if e.key() == Key::Enter {
                            send();
                        }
                    },
                }
                button { style: "padding:0.5rem 1rem;", onclick: move |_| send(), "{SEND_LABEL}" }
            }
        }
    }
}
