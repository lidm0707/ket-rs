#![allow(clippy::unwrap_used)]

use super::*;

const CHOICES: [ChoiceId; 3] = [ChoiceId(0), ChoiceId(1), ChoiceId(2)];

#[test]
fn rejects_over_depth() {
    let p = DepthConstraintPruner;
    assert!(!p.is_valid(MAX_DEPTH + 1, 0, &[]));
    assert!(p.is_valid(MAX_DEPTH, 0, &[]));
}

#[test]
fn rejects_out_of_range_token() {
    let p = DepthConstraintPruner;
    assert!(!p.is_valid(1, MAX_TOKENS_PER_DEPTH, &[]));
}

#[test]
fn rejects_parents_deeper_than_depth() {
    let p = DepthConstraintPruner;
    assert!(!p.is_valid(1, 0, &[0, 1, 2]));
}

#[test]
fn screening_adapter_is_binary() {
    let s = depth_screening();
    assert_eq!(s.relevance(1, 0, &[]), 1.0);
    assert_eq!(s.relevance(MAX_DEPTH + 1, 0, &[]), 0.0);
}

#[test]
fn prune_keeps_valid_only() {
    let s = depth_screening();
    let mut out = [ChoiceId(u16::MAX); CHOICES.len()];
    let n = prune_choices(&s, 1, &CHOICES, &[], &mut out);
    assert_eq!(n, CHOICES.len());
    assert_eq!(&out[..n], &CHOICES);
}

#[test]
fn prune_respects_out_capacity() {
    let s = depth_screening();
    let mut out = [ChoiceId(u16::MAX); 1];
    let n = prune_choices(&s, 1, &CHOICES, &[], &mut out);
    assert_eq!(n, 1);
}
