use rust_clasp::clasp::constraint::priority_reserved_ufs;
use rust_clasp::clasp::unfounded_check::{
    AtomData, BodyData, DEFAULT_UNFOUNDED_CHECK_PRIO, ExtData, ReasonStrategy, UfsType, WatchType,
};

#[test]
fn unfounded_check_enums_match_upstream_discriminants() {
    assert_eq!(ReasonStrategy::CommonReason as u32, 0);
    assert_eq!(ReasonStrategy::OnlyReason as u32, 1);
    assert_eq!(ReasonStrategy::DistinctReason as u32, 2);
    assert_eq!(ReasonStrategy::SharedReason as u32, 3);
    assert_eq!(ReasonStrategy::NoReason as u32, 4);

    assert_eq!(UfsType::None as u32, 0);
    assert_eq!(UfsType::Poly as u32, 1);
    assert_eq!(UfsType::NonPoly as u32, 2);

    assert_eq!(WatchType::SourceFalse as u32, 0);
    assert_eq!(WatchType::HeadFalse as u32, 1);
    assert_eq!(WatchType::HeadTrue as u32, 2);
    assert_eq!(WatchType::SubgoalFalse as u32, 3);
    assert_eq!(DEFAULT_UNFOUNDED_CHECK_PRIO, priority_reserved_ufs);
}

#[test]
fn atom_data_tracks_source_validity_like_upstream_bitfields() {
    let mut atom = AtomData::default();
    assert_eq!(atom.watch(), AtomData::NIL_SOURCE);
    assert!(!atom.has_source());
    assert!(!atom.todo);
    assert!(!atom.ufs);

    atom.resurrect_source();
    assert!(atom.has_source());
    assert_eq!(atom.watch(), AtomData::NIL_SOURCE);

    atom.set_source(17);
    assert!(atom.has_source());
    assert_eq!(atom.watch(), 17);

    atom.mark_source_invalid();
    assert!(!atom.has_source());
    assert_eq!(atom.watch(), 17);
}

#[test]
fn body_data_defaults_match_zero_initialized_upstream_state() {
    let body = BodyData::default();
    assert_eq!(body.watches, 0);
    assert!(!body.picked);
    assert_eq!(body.lower_or_ext, 0);
}

#[test]
fn ext_data_tracks_workspace_membership_and_lower_bound() {
    let mut ext = ExtData::new(70, 5);
    assert_eq!(ext.word_count(), 3);
    assert_eq!(ext.lower, 5);
    assert_eq!(ext.slack, -5);
    assert!(!ext.in_ws(0));
    assert!(!ext.in_ws(37));
    assert!(!ext.in_ws(69));

    assert!(!ext.add_to_ws(0, 2));
    assert!(ext.in_ws(0));
    assert_eq!(ext.lower, 3);

    assert!(!ext.add_to_ws(37, 1));
    assert!(ext.in_ws(37));
    assert_eq!(ext.lower, 2);

    assert!(ext.add_to_ws(69, 2));
    assert!(ext.in_ws(69));
    assert_eq!(ext.lower, 0);

    assert!(ext.add_to_ws(69, 2));
    assert_eq!(ext.lower, 0);

    ext.remove_from_ws(37, 1);
    assert!(!ext.in_ws(37));
    assert_eq!(ext.lower, 1);

    ext.remove_from_ws(37, 1);
    assert_eq!(ext.lower, 1);

    ext.remove_from_ws(69, 2);
    assert!(!ext.in_ws(69));
    assert_eq!(ext.lower, 3);
}

#[test]
fn ext_data_word_and_pos_follow_upstream_packing() {
    assert_eq!(ExtData::word(0), 0);
    assert_eq!(ExtData::pos(0), 0);
    assert_eq!(ExtData::word(31), 0);
    assert_eq!(ExtData::pos(31), 31);
    assert_eq!(ExtData::word(32), 1);
    assert_eq!(ExtData::pos(32), 0);
    assert_eq!(ExtData::word(63), 1);
    assert_eq!(ExtData::pos(63), 31);
}
