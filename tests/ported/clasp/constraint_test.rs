use rust_clasp::clasp::constraint::{ConstraintInfo, ConstraintScore, ConstraintType};

#[test]
fn upstream_constraint_score_layout_cases() {
    let mut score = ConstraintScore::new(1, 7);
    assert_eq!(score.activity(), 1);
    assert_eq!(score.lbd(), 7);
    score.bump_activity();
    score.bump_lbd(4);
    assert_eq!(score.activity(), 2);
    assert_eq!(score.lbd(), 4);
}

#[test]
fn upstream_constraint_info_type_cases() {
    let mut info = ConstraintInfo::new(ConstraintType::Static);
    assert_eq!(info.constraint_type(), ConstraintType::Static);
    assert!(!info.learnt());

    info.set_type(ConstraintType::Other)
        .set_activity(12)
        .set_lbd(2);
    assert_eq!(info.constraint_type(), ConstraintType::Other);
    assert_eq!(info.activity(), 12);
    assert_eq!(info.lbd(), 2);
    assert!(info.learnt());
}
