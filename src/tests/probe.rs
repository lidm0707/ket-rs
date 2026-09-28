use super::*;

#[test]
fn constructs_bank() {
    assert!(UrgencyProbe::new().is_some());
}

#[test]
fn calm_state_is_routine() {
    let dims = [0.0_f32; STATE_DIM];
    assert_eq!(UrgencyProbe::new().unwrap().tag(&dims), Urgency::Routine);
}

#[test]
fn hot_state_is_critical() {
    let dims = [5.0_f32, 0.0, 0.0, 0.0];
    assert_eq!(UrgencyProbe::new().unwrap().tag(&dims), Urgency::Critical);
}

#[test]
fn warm_state_is_elevated() {
    let dims = [0.0_f32, 3.0, 0.0, 0.0];
    assert_eq!(UrgencyProbe::new().unwrap().tag(&dims), Urgency::Elevated);
}

#[test]
fn label_roundtrip() {
    for d in 0..3u8 {
        let l = UrgencyLabel::from_u8(d).unwrap();
        assert_eq!(l.as_u8(), d);
    }
    assert!(UrgencyLabel::from_u8(3).is_none());
}
