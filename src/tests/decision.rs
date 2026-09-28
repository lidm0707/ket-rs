use super::*;
use crate::latent::FLAG_SAFE;

const CHOICES: [ChoiceId; 3] = [ChoiceId(0), ChoiceId(1), ChoiceId(2)];
const SENTINEL: ChoiceId = ChoiceId(u16::MAX);

fn query<'a>(state: &'a LatentState<'a>, keywords: &'a [&'a str]) -> KetQuery<'a> {
    KetQuery {
        state,
        question: TypedQuestion::Pick {
            keywords,
            choices: &CHOICES,
        },
    }
}

#[test]
fn g1_determinism_bit_identical() {
    let dims = [0.6, 0.2, 0.1, 0.0];
    let kw = ["risk"];
    let mut e1 = KetEngine::new().unwrap();
    let mut e2 = KetEngine::new().unwrap();
    let state = LatentState {
        dims: &dims,
        flags: FLAG_SAFE,
    };
    let mut c1 = [SENTINEL; 3];
    let mut s1 = [0.0; 3];
    let mut c2 = [SENTINEL; 3];
    let mut s2 = [0.0; 3];
    let d1 = e1
        .decide_into(query(&state, &kw), &mut c1, &mut s1)
        .unwrap();
    let d2 = e2
        .decide_into(query(&state, &kw), &mut c2, &mut s2)
        .unwrap();
    assert_eq!(d1, d2);
    assert_eq!(c1, c2);
    assert_eq!(s1, s2);
}

#[test]
fn rejects_conflicted_state() {
    let dims = [f32::NAN, 0.0, 0.0, 0.0];
    let kw = ["risk"];
    let mut e = KetEngine::new().unwrap();
    let state = LatentState {
        dims: &dims,
        flags: FLAG_SAFE,
    };
    let mut c = [SENTINEL; 3];
    let mut s = [0.0; 3];
    let q = query(&state, &kw);
    assert_eq!(
        e.decide_into(q, &mut c, &mut s),
        Err(KetError::ConflictedState)
    );
}

#[test]
fn hot_path_is_buffer_only() {
    // Fixed stack buffers only — structurally zero-alloc by signature.
    let dims = [0.6, 0.2, 0.1, 0.0];
    let kw = ["deadline"];
    let mut e = KetEngine::new().unwrap();
    let state = LatentState {
        dims: &dims,
        flags: FLAG_SAFE,
    };
    let mut c = [SENTINEL; 8];
    let mut s = [0.0; 8];
    let d = e.decide_into(query(&state, &kw), &mut c, &mut s).unwrap();
    assert_eq!(d.choice, CHOICES[0]);
    assert!(s[..3].iter().all(|v| v.is_finite()));
}

#[test]
fn urgency_flows_through() {
    let dims = [5.0, 0.0, 0.0, 0.0];
    let kw = ["urgent"];
    let mut e = KetEngine::new().unwrap();
    let state = LatentState {
        dims: &dims,
        flags: FLAG_SAFE,
    };
    let mut c = [SENTINEL; 3];
    let mut s = [0.0; 3];
    let d = e.decide_into(query(&state, &kw), &mut c, &mut s).unwrap();
    assert_eq!(d.urgency, Urgency::Critical);
}

#[test]
fn score_question_uses_full_bank() {
    let dims = [0.6, 0.2, 0.1, 0.0];
    let mut e = KetEngine::new().unwrap();
    let mut c = [SENTINEL; 3];
    let mut s = [0.0; 3];
    let q = KetQuery {
        state: &LatentState {
            dims: &dims,
            flags: FLAG_SAFE,
        },
        question: TypedQuestion::Score { choices: &CHOICES },
    };
    let d = e.decide_into(q, &mut c, &mut s).unwrap();
    assert_eq!(d.choice, CHOICES[0]);
}
