use super::*;

#[test]
fn routes_safety() {
    let kw = ["risk", "harm"];
    assert_eq!(KeywordRouter::route(&kw), Domain::Safety);
}

#[test]
fn routes_time() {
    let kw = ["deadline"];
    assert_eq!(KeywordRouter::route(&kw), Domain::Time);
}

#[test]
fn routes_general_on_miss() {
    let kw = ["unrelated"];
    assert_eq!(KeywordRouter::route(&kw), Domain::General);
}

#[test]
fn expert_ranges_in_bounds() {
    for d in [
        Domain::Safety,
        Domain::Time,
        Domain::Resource,
        Domain::General,
    ] {
        assert!(d.sector_offset() + d.sector_span() <= 4);
    }
}
