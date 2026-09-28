use super::*;

const A: ChoiceId = ChoiceId(7);

#[test]
fn choice_ids_are_comparable() {
    assert!(A == ChoiceId(7));
    assert!(A < ChoiceId(9));
}
