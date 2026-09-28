use super::*;

#[test]
fn thompson_is_deterministic_per_seed() {
    let a = ExplorationArms::<2>::new();
    assert_eq!(a.thompson(0), a.thompson(0));
}

#[test]
fn thompson_draws_in_unit_interval() {
    let a = ExplorationArms::<4>::new();
    for i in 0..4 {
        let s = a.thompson(i);
        assert!((0.0..=1.0).contains(&s));
    }
}

#[test]
fn observe_shifts_posterior_up() {
    let mut a = ExplorationArms::<2>::new();
    let before = a.best_belief(0);
    a.observe(0, 1.0, 1);
    let after = a.best_belief(0);
    assert!(after > before);
}

#[test]
fn conservative_selection_prefers_proven_arm() {
    let mut a = ExplorationArms::<3>::new();
    for t in 1..20 {
        a.observe(1, 1.0, t);
    }
    assert_eq!(a.select_conservative(), 1);
}
