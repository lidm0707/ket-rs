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
