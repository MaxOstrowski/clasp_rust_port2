use rust_clasp::clasp::asp_preprocessor::{AspPreprocessor, SatPreprocessor};
use rust_clasp::clasp::literal::{LitVec, ValueVec, neg_lit, pos_lit, var_max};
use rust_clasp::clasp::shared_context::SharedContext;

#[test]
fn asp_preprocessor_constructs() {
    let _ = AspPreprocessor::new();
}

#[test]
fn asp_preprocessor_defaults_to_non_eq_mode() {
    let pre = AspPreprocessor::new();

    assert!(!pre.eq());
}

#[test]
fn asp_preprocessor_returns_var_max_for_unset_root_atom() {
    let pre = AspPreprocessor::new();

    assert_eq!(pre.get_root_atom(pos_lit(7)), var_max);
}

#[test]
fn asp_preprocessor_starts_without_a_bound_program() {
    let mut pre = AspPreprocessor::new();

    assert!(pre.program().is_none());
    assert!(pre.program_mut().is_none());
}

#[test]
fn asp_preprocessor_sets_root_atom_by_literal_id() {
    let mut pre = AspPreprocessor::new();
    let root = neg_lit(4);

    pre.set_root_atom(root, 19);

    assert_eq!(pre.get_root_atom(root), 19);
    assert_eq!(pre.get_root_atom(pos_lit(4)), var_max);
}

#[test]
fn asp_preprocessor_pop_follow_uses_stack_or_queue_order() {
    let mut pre = AspPreprocessor::new();
    let mut idx = 0;

    pre.set_follow_for_test(&[3, 7]);
    pre.set_dfs_for_test(true);
    assert_eq!(pre.pop_follow_for_test(&mut idx), 7);
    assert_eq!(idx, 0);
    assert_eq!(pre.pop_follow_for_test(&mut idx), 3);

    pre.set_follow_for_test(&[3, 7]);
    pre.set_dfs_for_test(false);
    idx = 0;
    assert_eq!(pre.pop_follow_for_test(&mut idx), 3);
    assert_eq!(idx, 1);
    assert_eq!(pre.pop_follow_for_test(&mut idx), 7);
    assert_eq!(idx, 2);
}

#[test]
fn sat_preprocessor_splits_units_from_non_units() {
    let mut pre = SatPreprocessor::new();

    assert!(!pre.add_clause(&[]));
    assert!(pre.add_clause(&[pos_lit(1)]));
    assert!(pre.add_clause(&[neg_lit(1), pos_lit(2)]));

    assert_eq!(pre.num_clauses(), 1);
}

#[test]
fn sat_preprocessor_preprocess_applies_units_and_strengthens_clauses() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let _ = ctx.start_add_constraints();

    let mut pre = SatPreprocessor::new();
    assert!(pre.add_clause(&[pos_lit(a)]));
    assert!(pre.add_clause(&[neg_lit(a), pos_lit(b)]));

    assert!(pre.preprocess(&mut ctx));
    assert!(ctx.end_init());
    assert!(ctx.master_ref().is_true(pos_lit(a)));
    assert!(ctx.master_ref().is_true(pos_lit(b)));
}

#[test]
fn sat_preprocessor_extend_model_flips_last_open_literal_once() {
    let mut pre = SatPreprocessor::new();
    let mut model = ValueVec::new();
    let mut open = LitVec::new();
    open.push_back(pos_lit(3));

    pre.extend_model(&mut model, &mut open);

    assert_eq!(open.len(), 0);
    assert!(model.is_empty());
}
