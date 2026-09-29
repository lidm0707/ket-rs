use super::*;
use crate::question::ChoiceId;

#[test]
fn projection_is_deterministic() {
    let dims = [0.6, 0.2, 0.1, 0.0];
    let mut s = KetScorer::new();
    let a = s.project(&dims);
    let b = s.project(&dims);
    assert_eq!(a, b);
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn domain_score_averages_span() {
    let projected = [0.0, 1.0, 3.0, 7.0];
    assert_eq!(KetScorer::domain_score(&projected, Domain::Safety), 0.0);
    assert_eq!(KetScorer::domain_score(&projected, Domain::Time), 1.0);
    assert_eq!(KetScorer::domain_score(&projected, Domain::General), 2.75);
}

#[test]
fn gate_is_ternary() {
    let s = KetScorer::new();
    let loud = [5.0, 5.0, 5.0, 5.0];
    let quiet = [-5.0, -5.0, -5.0, -5.0];
    let speakish = [1.0, 0.0, 0.0, 0.0];
    assert!(matches!(
        s.gate(&speakish, ChoiceId(0)),
        SalienceDecision::Speak
    ));
    assert!(matches!(
        s.gate(&quiet, ChoiceId(0)),
        SalienceDecision::Silent
    ));
    assert!(matches!(
        s.gate(&loud, ChoiceId(3)),
        SalienceDecision::Delegate(ChoiceId(3))
    ));
}

#[test]
fn builder_overrides_default_rules() {
    let dims = [0.0, 5.0, 0.0, 0.0];
    let mut custom = KetScorer::builder()
        .sector(
            Sector::Safety,
            [(Dim::DelegateAxis, Sign::Push), (Dim::Support, Sign::Push)],
        )
        .build();
    // Default Safety reads SpeakAxis/Resist — dim1 is neutral there (~0.5);
    // the override makes DelegateAxis push Safety hard.
    assert!(custom.project(&dims)[0] > 0.9);
    assert!(KetScorer::new().project(&dims)[0] < 0.6);
}
