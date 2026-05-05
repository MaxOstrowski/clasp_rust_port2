use rust_clasp::clasp::constraint::priority_reserved_ufs;
use rust_clasp::clasp::solver_strategies::FwdCheck;
use rust_clasp::clasp::unfounded_check::{
    AtomData, BodyData, BodyPtr, DEFAULT_UNFOUNDED_CHECK_PRIO, DefaultUnfoundedCheck, ExtData,
    ExtWatch, MinimalityCheck, ReasonStrategy, UfsType, WatchType,
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
    let body = BodyData::new();
    assert_eq!(body.watches, 0);
    assert!(!body.picked);
    assert_eq!(body.lower_or_ext, 0);
    assert_eq!(body, BodyData::default());
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

#[test]
fn default_unfounded_check_starts_with_empty_structural_state() {
    let mut check = DefaultUnfoundedCheck::default();

    assert_eq!(check.priority(), priority_reserved_ufs);
    assert_eq!(check.reason_strategy(), ReasonStrategy::CommonReason);
    assert!(check.graph().is_none());
    assert!(!check.solver_bound());
    assert!(!check.has_minimality_check());
    assert_eq!(check.nodes(), 0);
    assert_eq!(check.todo_count(), 0);
    assert_eq!(check.ufs_count(), 0);
    assert_eq!(check.source_queue_count(), 0);
    assert_eq!(check.extended_len(), 0);
    assert_eq!(check.watch_count(), 0);
    assert_eq!(check.picked_ext_len(), 0);
    assert_eq!(check.loop_atom_count(), 0);
    assert_eq!(check.active_clause_len(), 0);
    assert_eq!(check.reason_slots(), 0);
    assert_eq!(check.invalid_len(), 0);
    assert!(!check.info().tagged());

    check.set_reason_strategy(ReasonStrategy::SharedReason);
    assert_eq!(check.reason_strategy(), ReasonStrategy::SharedReason);
}

#[test]
fn minimality_check_ctor_normalizes_forward_check_parameters_like_upstream() {
    let fwd = FwdCheck {
        high_step: 3,
        high_pct: 25,
        sign_def: 1,
        disable: 0,
    };
    let minimality = MinimalityCheck::new(fwd);

    assert_eq!(minimality.fwd, fwd);
    assert_eq!(minimality.high, 3);
    assert_eq!(minimality.low, 0);
    assert_eq!(minimality.next, 0);
    assert_eq!(minimality.scc, 0);

    let clamped = MinimalityCheck::new(FwdCheck {
        high_step: 7,
        high_pct: 125,
        sign_def: 0,
        disable: 0,
    });
    assert_eq!(clamped.fwd.high_pct, 100);
    assert_eq!(clamped.high, 7);

    let unlimited = MinimalityCheck::new(FwdCheck {
        high_step: 0,
        high_pct: 10,
        sign_def: 0,
        disable: 0,
    });
    assert_eq!(unlimited.fwd.high_step, u32::MAX);
    assert_eq!(unlimited.high, u32::MAX);

    let disabled = MinimalityCheck::new(FwdCheck {
        high_step: 9,
        high_pct: 20,
        sign_def: 0,
        disable: 1,
    });
    assert_eq!(disabled.next, u32::MAX);
}

#[test]
fn body_ptr_and_ext_watch_preserve_simple_storage_state() {
    let body_ptr = BodyPtr::new(None, 17);
    let watch = ExtWatch {
        body_id: 12,
        data: 9,
    };

    assert!(body_ptr.node.is_none());
    assert_eq!(body_ptr.id, 17);

    assert_eq!(watch.body_id, 12);
    assert_eq!(watch.data, 9);
}
