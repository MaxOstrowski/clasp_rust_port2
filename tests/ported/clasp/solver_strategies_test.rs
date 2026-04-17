use rust_clasp::clasp::constraint::{ConstraintInfo, ConstraintType, Solver};
use rust_clasp::clasp::literal::{LitVec, pos_lit};
use rust_clasp::clasp::solver_strategies::{
    BasicSatConfig, CCMinAntes, Configuration, ConflictEvent, HeuParams, HeuristicType, LbdMode,
    RestartKeep, ScheduleStrategy, SearchLimits, SearchStrategy, SignHeu, SolverParams, WatchInit,
};
use rust_clasp::clasp::util::misc_types::{Subsystem, Verbosity};

#[test]
fn solver_params_defaults_match_upstream() {
    let config = BasicSatConfig::new();
    let params = config.solver(0);

    assert_eq!(params.heu_id, HeuristicType::Def as u32);
    assert_eq!(params.cc_min_rec, 0);
    assert_eq!(params.cc_min_antes, CCMinAntes::AllAntes as u32);
    assert_eq!(params.search, SearchStrategy::UseLearning as u32);
    assert_eq!(params.compress, 0);
    assert_eq!(params.init_watches, WatchInit::WatchRand as u32);
    assert_eq!(params.sign_def, SignHeu::SignAtom as u32);

    let heuristic = HeuParams::default();
    assert_eq!(params.heuristic.score, heuristic.score);
    assert_eq!(params.heuristic.other, heuristic.other);
    assert_eq!(params.heuristic.moms, 1);
}

#[test]
fn schedule_advance_matches_iterated_geom_sequence() {
    let mut stepped = ScheduleStrategy::geom(100, 1.5, 13);
    for i in 0..((1u32 << 12) - 1) {
        let mut advanced = ScheduleStrategy::geom(100, 1.5, 13);
        advanced.advance_to(i);
        assert_eq!(stepped.idx, advanced.idx, "idx mismatch at {i}");
        assert_eq!(stepped.len, advanced.len, "len mismatch at {i}");
        stepped.next();
    }
}

#[test]
fn schedule_advance_matches_iterated_luby_sequence() {
    let mut stepped = ScheduleStrategy::luby(64, 10);
    for i in 0..((1u32 << 12) - 1) {
        let mut advanced = ScheduleStrategy::luby(64, 10);
        advanced.advance_to(i);
        assert_eq!(stepped.idx, advanced.idx, "idx mismatch at {i}");
        assert_eq!(stepped.len, advanced.len, "len mismatch at {i}");
        stepped.next();
    }
}

#[test]
fn schedule_overflow_and_clamping_match_upstream() {
    let mut sched = ScheduleStrategy::geom(100, 0.0, 0);
    assert_eq!(sched.grow, 1.0);
    assert_eq!(sched.current(), 100);

    sched = ScheduleStrategy::geom(1, 2.0, 0);
    assert_eq!(sched.current(), 1);
    assert_eq!(sched.next(), 2);
    assert_eq!(sched.next(), 4);
    sched.advance_to(12);
    assert_eq!(sched.current(), 4096);
    sched.advance_to(63);
    assert_eq!(sched.current(), 1u64 << 63);
    sched.advance_to(64);
    assert_eq!(sched.current(), u64::MAX);

    sched.reset();
    assert_eq!(sched.idx, 0);
    assert_eq!(sched.current(), 1);
}

#[test]
fn solver_prepare_applies_upstream_adjustments() {
    let mut no_learning = SolverParams {
        search: SearchStrategy::NoLearning as u32,
        heu_id: HeuristicType::Berkmin as u32,
        compress: 12,
        save_progress: 7,
        reverse_arcs: 1,
        otfs: 1,
        update_lbd: LbdMode::LbdUpdateGlucose as u32,
        bump_var_act: 1,
        ..SolverParams::default()
    };
    assert_eq!(no_learning.prepare(), 1);
    assert_eq!(no_learning.heu_id, HeuristicType::None as u32);
    assert_eq!(no_learning.compress, 0);
    assert_eq!(no_learning.save_progress, 0);
    assert_eq!(no_learning.reverse_arcs, 0);
    assert_eq!(no_learning.otfs, 0);
    assert_eq!(no_learning.update_lbd, 0);
    assert_eq!(no_learning.bump_var_act, 0);
    assert_eq!(no_learning.cc_min_antes, CCMinAntes::NoAntes as u32);

    let mut unit = SolverParams {
        heu_id: HeuristicType::Unit as u32,
        look_type: 0,
        look_ops: 99,
        ..SolverParams::default()
    };
    assert_eq!(unit.prepare(), 2);
    assert_eq!(
        unit.look_type,
        rust_clasp::clasp::literal::VarType::Atom as u32
    );
    assert_eq!(unit.look_ops, 0);

    let mut domain = SolverParams::default();
    domain.heuristic.dom_pref = 16;
    domain.heuristic.dom_mod = 3;
    assert_eq!(domain.prepare(), 4);
    assert_eq!(domain.heuristic.dom_pref, 0);
    assert_eq!(domain.heuristic.dom_mod, 0);
}

#[test]
fn restart_schedule_dynamic_encoding_roundtrips() {
    let schedule = rust_clasp::clasp::solver_strategies::RestartSchedule::dynamic(
        128,
        1.2,
        10,
        rust_clasp::clasp::util::misc_types::MovingAvgType::AvgEma,
        RestartKeep::Block,
        rust_clasp::clasp::util::misc_types::MovingAvgType::AvgEmaLog,
        20,
    );
    assert!(schedule.is_dynamic());
    assert_eq!(schedule.keep_avg(), RestartKeep::Block);
    assert_eq!(schedule.slow_win(), 20);
    assert_eq!(schedule.adjust_lim(), 16_000);
}

#[test]
fn search_limits_default_to_upstream_unbounded_values() {
    let limits = SearchLimits::default();

    assert_eq!(limits.used, 0);
    assert_eq!(limits.restart_conflicts, u64::MAX);
    assert!(limits.dynamic.is_none());
    assert!(limits.block.is_none());
    assert!(!limits.local);
    assert_eq!(limits.conflicts, u64::MAX);
    assert_eq!(limits.memory, u64::MAX);
    assert_eq!(limits.learnts, u32::MAX);
}

#[test]
fn conflict_event_exposes_upstream_event_identity_and_payload() {
    let solver = Solver::new();
    let learnt = LitVec::from_slice(&[pos_lit(1), pos_lit(2)]);
    let info = ConstraintInfo::new(ConstraintType::Conflict);
    let event = ConflictEvent::new(&solver, learnt.as_slice(), info);

    assert_eq!(event.base.base.system, Subsystem::SubsystemSolve as u32);
    assert_eq!(event.base.base.verb, Verbosity::VerbosityQuiet as u32);
    assert_eq!(event.base.solver, &solver as *const Solver);
    assert_eq!(event.learnt, learnt.as_slice());
    assert_eq!(event.info.constraint_type(), ConstraintType::Conflict);
}
