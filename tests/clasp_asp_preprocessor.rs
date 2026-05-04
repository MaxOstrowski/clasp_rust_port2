use rust_clasp::clasp::asp_preprocessor::SatPreprocessor;
use rust_clasp::clasp::literal::{LitVec, ValueVec, neg_lit, pos_lit};
use rust_clasp::clasp::shared_context::SharedContext;

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
