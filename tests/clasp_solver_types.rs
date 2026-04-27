use rust_clasp::clasp::constraint::ConstraintType;
use rust_clasp::clasp::constraint::{ClauseHead, Constraint};
use rust_clasp::clasp::solver_types::{
    ClauseWatch, CoreStats, ExtendedStats, GenericWatch, JumpStats, SolverStats, StatisticType,
};

#[test]
fn core_stats_match_upstream_accumulation_and_lookup() {
    let mut lhs = CoreStats {
        choices: 2,
        conflicts: 7,
        analyzed: 5,
        restarts: 2,
        last_restart: 10,
        bl_restarts: 1,
    };
    let rhs = CoreStats {
        choices: 3,
        conflicts: 4,
        analyzed: 1,
        restarts: 3,
        last_restart: 6,
        bl_restarts: 9,
    };

    lhs.accu(&rhs);

    assert_eq!(lhs.choices, 5);
    assert_eq!(lhs.conflicts, 11);
    assert_eq!(lhs.backtracks(), 5);
    assert_eq!(lhs.backjumps(), 6);
    assert_eq!(lhs.last_restart, 10);
    assert_eq!(lhs.bl_restarts, 9);
    assert_eq!(CoreStats::size(), 6);
    assert_eq!(CoreStats::key(2), "conflicts_analyzed");
    assert_eq!(lhs.at("restarts").value(), 5.0);
    assert!((lhs.avg_restart() - 1.2).abs() < f64::EPSILON);
}

#[test]
fn jump_stats_track_bounded_and_unbounded_jumps() {
    let mut stats = JumpStats::default();
    stats.update(10, 4, 7);
    stats.update(12, 5, 5);

    assert_eq!(stats.jumps, 2);
    assert_eq!(stats.bounded, 1);
    assert_eq!(stats.jump_sum, 13);
    assert_eq!(stats.bound_sum, 3);
    assert_eq!(stats.jumped(), 10);
    assert_eq!(stats.max_jump, 7);
    assert_eq!(stats.max_jump_ex, 7);
    assert_eq!(stats.max_bound, 3);
    assert!((stats.avg_bound() - 3.0).abs() < f64::EPSILON);
    assert!((stats.avg_jump() - 6.5).abs() < f64::EPSILON);
    assert!((stats.avg_jump_ex() - 5.0).abs() < f64::EPSILON);
    assert!((stats.jumped_ratio() - (10.0 / 13.0)).abs() < f64::EPSILON);
    assert_eq!(JumpStats::key(5), "max_executed");
}

#[test]
fn watch_helper_predicates_match_pointer_identity() {
    let clause_a = 0x11usize as *mut ClauseHead;
    let clause_b = 0x12usize as *mut ClauseHead;
    let constraint_a = 0x21usize as *mut Constraint;
    let constraint_b = 0x22usize as *mut Constraint;

    let clause_match = ClauseWatch::eq_head(clause_a);
    assert!(clause_match.matches(&ClauseWatch::new(clause_a)));
    assert!(!clause_match.matches(&ClauseWatch::new(clause_b)));

    let constraint_match = GenericWatch::eq_constraint(constraint_a);
    assert!(constraint_match.matches(&GenericWatch::new(constraint_a, 7)));
    assert!(!constraint_match.matches(&GenericWatch::new(constraint_b, 7)));
}

#[test]
fn extended_stats_expose_nested_jump_map_and_lemma_totals() {
    let mut stats = ExtendedStats::default();
    stats.add_learnt(2, ConstraintType::Conflict);
    stats.add_learnt(4, ConstraintType::Loop);
    stats.add_learnt(3, ConstraintType::Other);
    stats.dom_choices = 9;
    stats.model_lits = 10;
    stats.models = 2;
    stats.distributed = 3;
    stats.sum_dist_lbd = 9;
    stats.integrated = 2;
    stats.int_imps = 2;
    stats.int_jumps = 7;
    stats.gps = 2;
    stats.gp_lits = 11;
    stats.jumps.update(8, 3, 6);

    assert_eq!(stats.lemmas(), 3);
    assert_eq!(stats.learnt_lits(), 9);
    assert_eq!(stats.lemmas_of(ConstraintType::Conflict), 1);
    assert_eq!(stats.lemmas_of(ConstraintType::Static), 0);
    assert!((stats.avg_len(ConstraintType::Loop) - 4.0).abs() < f64::EPSILON);
    assert!((stats.avg_model() - 5.0).abs() < f64::EPSILON);
    assert!((stats.avg_dist_lbd() - 3.0).abs() < f64::EPSILON);
    assert!((stats.avg_int_jump() - 3.5).abs() < f64::EPSILON);
    assert!((stats.avg_gp() - 5.5).abs() < f64::EPSILON);
    assert!((stats.int_ratio() - (2.0 / 3.0)).abs() < f64::EPSILON);
    assert_eq!(ExtendedStats::size(), 23);
    assert_eq!(ExtendedStats::key(19), "lemmas_conflict");
    assert_eq!(stats.at("lemmas_loop").value(), 1.0);

    let nested = stats.at("jumps");
    assert_eq!(nested.type_(), StatisticType::Map);
    assert_eq!(nested.size(), 7);
    assert_eq!(nested.key(0), "jumps");
    assert_eq!(nested.at("levels_bounded").value(), 3.0);
}

#[test]
fn solver_stats_enable_accumulate_flush_and_swap() {
    let mut source = SolverStats::default();
    source.core.choices = 4;
    source.core.conflicts = 3;
    source.core.analyzed = 2;
    assert!(source.enable_extended());
    source.add_learnt(2, ConstraintType::Conflict);
    source.add_distributed(5, ConstraintType::Conflict);
    source.add_test(true);
    source.add_model(6);
    source.add_cpu_time(1.25);
    source.add_split(3);
    source.add_dom_choice(2);
    source.add_integrated_asserting(9, 4);
    source.add_integrated(5);
    source.remove_integrated(2);
    source.add_path(7);
    source.add_conflict(9, 4, 7, 0);
    source.add_deleted(8);

    let extra = source.at("extra");
    assert_eq!(extra.type_(), StatisticType::Map);
    assert_eq!(extra.at("distributed").value(), 1.0);
    assert_eq!(extra.at("distributed_sum_lbd").value(), 5.0);
    assert_eq!(extra.at("hcc_partial").value(), 1.0);
    assert_eq!(extra.at("models").value(), 1.0);
    assert_eq!(extra.at("splits").value(), 3.0);
    assert_eq!(extra.at("domain_choices").value(), 2.0);
    assert_eq!(extra.at("integrated_imps").value(), 1.0);
    assert_eq!(extra.at("integrated_jumps").value(), 5.0);
    assert_eq!(extra.at("integrated").value(), 3.0);
    assert_eq!(extra.at("guiding_paths").value(), 1.0);
    assert_eq!(extra.at("guiding_paths_lits").value(), 7.0);
    assert_eq!(extra.at("lemmas_deleted").value(), 8.0);
    assert_eq!(extra.at("lemmas_conflict").value(), 1.0);
    assert_eq!(extra.at("lemmas_binary").value(), 1.0);

    let jump_map = extra.at("jumps");
    assert_eq!(jump_map.type_(), StatisticType::Map);
    assert_eq!(jump_map.at("jumps").value(), 1.0);

    let mut sink = SolverStats::default();
    source.set_multi(&mut sink);
    source.flush();
    assert_eq!(sink.core.choices, 4);
    assert_eq!(sink.core.conflicts, 3);
    assert_eq!(sink.core.analyzed, 3);
    assert_eq!(sink.size(), 7);
    assert_eq!(sink.key(6), "extra");

    let mut swap_a = SolverStats::default();
    swap_a.core.choices = 1;
    let mut swap_b = SolverStats::default();
    swap_b.core.choices = 9;
    assert!(swap_b.enable_extended());
    swap_a.swap_stats(&mut swap_b);
    assert_eq!(swap_a.core.choices, 9);
    assert!(swap_a.extra.is_some());
    assert_eq!(swap_b.core.choices, 1);
    assert!(swap_b.extra.is_none());
}

#[test]
fn solver_stats_camel_case_wrappers_delegate_to_existing_behavior() {
    let mut stats = SolverStats::default();
    assert!(stats.enableExtended());

    stats.addLearnt(3, ConstraintType::Other);
    stats.addDistributed(4, ConstraintType::Conflict);
    stats.addTest(false);
    stats.addModel(5);
    stats.addCpuTime(0.5);
    stats.addSplit(2);
    stats.addDomChoice(7);
    stats.addIntegratedAsserting(9, 6);
    stats.addIntegrated(3);
    stats.removeIntegrated(1);
    stats.addPath(8);
    stats.addConflict(10, 4, 6, 99);
    stats.addDeleted(11);

    let extra = stats.at("extra");
    assert_eq!(extra.at("lemmas_other").value(), 1.0);
    assert_eq!(extra.at("distributed_sum_lbd").value(), 4.0);
    assert_eq!(extra.at("hcc_partial").value(), 0.0);
    assert_eq!(extra.at("models_level").value(), 5.0);
    assert_eq!(extra.at("cpu_time").value(), 0.5);
    assert_eq!(extra.at("splits").value(), 2.0);
    assert_eq!(extra.at("domain_choices").value(), 7.0);
    assert_eq!(extra.at("integrated_imps").value(), 1.0);
    assert_eq!(extra.at("integrated_jumps").value(), 3.0);
    assert_eq!(extra.at("integrated").value(), 2.0);
    assert_eq!(extra.at("guiding_paths").value(), 1.0);
    assert_eq!(extra.at("guiding_paths_lits").value(), 8.0);
    assert_eq!(extra.at("lemmas_deleted").value(), 11.0);

    let mut other = SolverStats::default();
    other.core.choices = 42;
    stats.swapStats(&mut other);
    assert_eq!(stats.core.choices, 42);
    assert_eq!(other.at("extra").at("lemmas_deleted").value(), 11.0);
}
