use rust_clasp::clasp::cb_enumerator::{
    CbConsequences, ConsequenceAlgorithm, ConsequenceInitWarning, ConsequenceModelType, EnumMode,
    EnumOptions, UnsatType,
};
use rust_clasp::clasp::literal::{
    LitVec, ValueVec, neg_lit, pos_lit, value_false, value_free, value_true,
};
use rust_clasp::clasp::shared_context::SharedContext;

#[test]
fn brave_enumerator_forces_default_algorithm() {
    let consequences =
        CbConsequences::new(ConsequenceModelType::Brave, ConsequenceAlgorithm::Query);

    assert_eq!(consequences.model_type(), ConsequenceModelType::Brave);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Def);
    assert!(consequences.exhaustive());
}

#[test]
fn cautious_query_mode_is_preserved_until_init() {
    let consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    assert_eq!(consequences.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Query);
}

#[test]
fn enum_options_create_brave_and_cautious_variants_like_upstream() {
    let brave = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Brave,
        ..EnumOptions::default()
    });
    let cautious = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Cautious,
        ..EnumOptions::default()
    });
    let query = CbConsequences::from_enum_options(EnumOptions {
        enum_mode: EnumMode::Query,
        ..EnumOptions::default()
    });

    assert_eq!(brave.model_type(), ConsequenceModelType::Brave);
    assert_eq!(brave.algorithm(), ConsequenceAlgorithm::Def);

    assert_eq!(cautious.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(cautious.algorithm(), ConsequenceAlgorithm::Def);

    assert_eq!(query.model_type(), ConsequenceModelType::Cautious);
    assert_eq!(query.algorithm(), ConsequenceAlgorithm::Query);
}

#[test]
fn splitting_and_unsat_behavior_match_algorithm_selection() {
    let default_algo =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Def);
    let query_algo =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    assert!(default_algo.supports_splitting(true));
    assert!(!default_algo.supports_splitting(false));
    assert_eq!(default_algo.unsat_type(UnsatType::Stop), UnsatType::Stop);
    assert_eq!(default_algo.unsat_type(UnsatType::Cont), UnsatType::Cont);

    assert!(!query_algo.supports_splitting(true));
    assert_eq!(query_algo.unsat_type(UnsatType::Stop), UnsatType::Cont);
    assert_eq!(query_algo.unsat_type(UnsatType::Cont), UnsatType::Cont);
}

#[test]
fn query_mode_falls_back_to_default_under_optimization() {
    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    let warning = consequences.prepare_for_init(true);

    assert_eq!(
        warning,
        Some(ConsequenceInitWarning::QueryDoesNotSupportOptimization)
    );
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Def);
}

#[test]
fn default_mode_without_optimization_keeps_query_algorithm() {
    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Query);

    let warning = consequences.prepare_for_init(false);

    assert_eq!(warning, None);
    assert_eq!(consequences.algorithm(), ConsequenceAlgorithm::Query);
}

#[test]
fn add_lit_tracks_only_unmarked_non_eliminated_literals() {
    let mut ctx = SharedContext::new();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    ctx.eliminate(b.var());

    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Def);
    consequences.add_lit(&mut ctx, a);
    consequences.add_lit(&mut ctx, a);
    consequences.add_lit(&mut ctx, b);

    assert_eq!(consequences.consequence_literals(), &[a]);
    assert!(ctx.marked(a));
    assert!(ctx.var_info(a.var()).frozen());
    assert!(!ctx.marked(b));
}

#[test]
fn add_current_updates_brave_state_and_publishes_shared_clause() {
    let mut ctx = SharedContext::new();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());

    let solver = ctx.master();
    solver.set_value(a.var(), value_true, 1);
    solver.set_value(b.var(), value_free, 0);
    solver.set_value(c.var(), value_false, 3);

    let mut flagged = b;
    flagged.flag();

    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Brave, ConsequenceAlgorithm::Def);
    consequences.set_consequence_literals_for_test(&[a, flagged, c]);
    consequences.enable_shared_constraint_for_test();

    let mut clause = LitVec::new();
    let mut model = ValueVec::new();
    consequences.add_current(solver, &mut clause, &mut model, 1);

    let mut flagged_a = a;
    flagged_a.flag();
    assert_eq!(
        consequences.consequence_literals(),
        &[flagged_a, flagged, c]
    );
    assert_eq!(clause.as_slice(), &[!ctx.step_literal(), c]);
    assert_eq!(
        consequences.shared_clause_for_test(),
        Some(vec![!ctx.step_literal(), c])
    );
    assert_eq!(model.as_slice()[a.var() as usize], value_true);
    assert_eq!(model.as_slice()[b.var() as usize], value_true);
    assert_eq!(model.as_slice()[c.var() as usize], 4);
}

#[test]
fn add_current_intersects_cautious_state_and_negates_open_literals() {
    let mut ctx = SharedContext::new();
    let a = pos_lit(ctx.add_var());
    let b = neg_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());

    let solver = ctx.master();
    solver.set_value(a.var(), value_true, 2);
    solver.set_value(b.var(), value_false, 3);
    solver.set_value(c.var(), value_true, 1);

    let mut flagged_a = a;
    flagged_a.flag();
    let mut flagged_b = b;
    flagged_b.flag();

    let mut consequences =
        CbConsequences::new(ConsequenceModelType::Cautious, ConsequenceAlgorithm::Def);
    consequences.set_consequence_literals_for_test(&[flagged_a, flagged_b, c]);

    let mut clause = LitVec::new();
    let mut model = ValueVec::new();
    consequences.add_current(solver, &mut clause, &mut model, 1);

    assert_eq!(
        consequences.consequence_literals(),
        &[flagged_a, flagged_b, c]
    );
    assert_eq!(clause.as_slice(), &[!ctx.step_literal(), !a, !b]);
    assert_eq!(model.as_slice()[a.var() as usize], 5);
    assert_eq!(model.as_slice()[b.var() as usize], 10);
    assert_eq!(model.as_slice()[c.var() as usize], 0);
}
