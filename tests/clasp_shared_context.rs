use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::cli::clasp_cli_options::ProjectMode;
use rust_clasp::clasp::cli::clasp_cli_options::context_params::ShortSimpMode;
use rust_clasp::clasp::constraint::{
    Constraint, ConstraintDyn, ConstraintInfo, ConstraintType, PropResult,
};
use rust_clasp::clasp::dependency_graph::ExtDepGraph;
use rust_clasp::clasp::literal::{Literal, WeightLiteral, neg_lit, pos_lit};
use rust_clasp::clasp::shared_context::{
    DistributorPolicy, DomainTable, EventHandler, EventObserver, LogEvent, OutputTable, ReportMode,
    SharedConflictEvent, SharedContext, SolveMode,
};
use rust_clasp::clasp::solver::Solver;
use rust_clasp::clasp::solver_strategies::{DomPref, ShortMode, UserConfiguration};
use rust_clasp::clasp::solver_types::SolverStats;
use rust_clasp::clasp::util::misc_types::{
    EnterEvent, Event, EventLike, Range32, Subsystem, Verbosity, event_cast,
};
use rust_clasp::potassco::basic_types::DomModifier;

type EnterTrace = Rc<RefCell<Vec<(u32, u32, bool)>>>;
type LogTrace = Rc<RefCell<Vec<(u32, u32, u32, String)>>>;
type ConflictTrace = Rc<RefCell<Vec<(Vec<Literal>, ConstraintType)>>>;

#[derive(Default)]
struct TraceObserver {
    seen: EnterTrace,
}

impl EventObserver for TraceObserver {
    fn on_event(&mut self, event: &dyn EventLike) {
        let base = event.event();
        self.seen.borrow_mut().push((
            base.system,
            base.verb,
            event_cast::<EnterEvent>(event).is_some(),
        ));
    }
}

#[derive(Clone, Copy, Debug)]
struct DemoEvent {
    base: Event,
}

impl DemoEvent {
    fn new(system: Subsystem, verbosity: Verbosity) -> Self {
        Self {
            base: Event::for_type::<Self>(system, verbosity),
        }
    }
}

impl EventLike for DemoEvent {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Default)]
struct LogTraceObserver {
    seen: LogTrace,
}

impl EventObserver for LogTraceObserver {
    fn on_event(&mut self, event: &dyn EventLike) {
        let base = event.event();
        if let Some(log) = event.as_any().downcast_ref::<LogEvent>() {
            self.seen
                .borrow_mut()
                .push((base.system, base.verb, base.op, log.msg.clone()));
        }
    }
}

#[derive(Default)]
struct ConflictTraceObserver {
    seen: ConflictTrace,
}

impl EventObserver for ConflictTraceObserver {
    fn on_event(&mut self, event: &dyn EventLike) {
        if let Some(conflict) = event.as_any().downcast_ref::<SharedConflictEvent>() {
            self.seen
                .borrow_mut()
                .push((conflict.learnt.clone(), conflict.info.constraint_type()));
        }
    }
}

#[derive(Default)]
struct NoopConstraint;

impl ConstraintDyn for NoopConstraint {
    fn propagate(&mut self, _s: &mut Solver, _p: Literal, _data: &mut u32) -> PropResult {
        PropResult::default()
    }

    fn reason(
        &mut self,
        _s: &mut Solver,
        _p: Literal,
        _lits: &mut rust_clasp::clasp::literal::LitVec,
    ) {
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        Some(Box::new(Constraint::new(Self)))
    }
}

#[test]
fn event_handler_dispatches_enter_events_and_respects_subsystem_verbosity() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observer = TraceObserver {
        seen: Rc::clone(&seen),
    };
    let mut handler = EventHandler::with_observer(Verbosity::VerbosityQuiet, observer);

    assert_eq!(handler.verbosity(Subsystem::SubsystemFacade), 0);
    assert!(handler.set_active(Subsystem::SubsystemLoad));
    assert_eq!(seen.borrow().len(), 0);
    assert_eq!(handler.active(), Subsystem::SubsystemLoad);

    handler.set_verbosity(Subsystem::SubsystemLoad, Verbosity::VerbosityLow);
    let low = DemoEvent::new(Subsystem::SubsystemLoad, Verbosity::VerbosityLow);
    let high = DemoEvent::new(Subsystem::SubsystemLoad, Verbosity::VerbosityHigh);
    handler.dispatch(&low);
    handler.dispatch(&high);

    assert_eq!(&*seen.borrow(), &[(1, 1, false)]);

    handler.set_verbosity(Subsystem::SubsystemPrepare, Verbosity::VerbosityHigh);
    assert!(handler.set_active(Subsystem::SubsystemPrepare));
    assert_eq!(&*seen.borrow(), &[(1, 1, false), (2, 2, true)]);
    assert!(!handler.set_active(Subsystem::SubsystemPrepare));
}

#[test]
fn shared_context_short_simplification_skips_subsumed_ternary_clause() {
    let mut ctx = SharedContext::default();
    ctx.set_short_simp_mode(ShortSimpMode::SimpAll);
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    assert_eq!(
        ctx.add_imp(&[neg_lit(a), pos_lit(b)], ConstraintType::Static),
        1
    );
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 0);

    assert_eq!(
        ctx.add_imp(
            &[neg_lit(a), pos_lit(b), pos_lit(c)],
            ConstraintType::Static
        ),
        1
    );
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 0);
    assert_eq!(
        ctx.short_implications().num_edges(Literal::new(a, false)),
        1
    );
}

#[test]
fn shared_context_learnt_short_simplification_only_applies_to_learnt_clauses() {
    let mut ctx = SharedContext::default();
    ctx.set_short_simp_mode(ShortSimpMode::SimpLearnt);
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    assert_eq!(
        ctx.add_imp(&[neg_lit(a), pos_lit(b)], ConstraintType::Static),
        1
    );
    assert_eq!(
        ctx.add_imp(
            &[neg_lit(a), pos_lit(b), pos_lit(c)],
            ConstraintType::Static
        ),
        1
    );
    assert_eq!(ctx.num_ternary(), 1);

    ctx.remove_imp(&[neg_lit(a), pos_lit(b), pos_lit(c)], false);
    assert_eq!(ctx.num_ternary(), 0);

    assert_eq!(
        ctx.add_imp(&[neg_lit(a), pos_lit(b), pos_lit(c)], ConstraintType::Other),
        1
    );
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 0);
    assert_eq!(ctx.num_learnt_short(), 0);
}

#[test]
fn shared_context_short_mode_explicit_blocks_non_static_implicit_clauses() {
    let mut ctx = SharedContext::default();
    ctx.set_short_mode(ShortMode::ShortExplicit, ShortSimpMode::SimpNo);
    let a = ctx.add_var();
    let b = ctx.add_var();

    assert_eq!(
        ctx.add_imp(&[neg_lit(a), pos_lit(b)], ConstraintType::Other),
        -1
    );
    assert_eq!(
        ctx.add_imp(&[neg_lit(a), pos_lit(b)], ConstraintType::Static),
        1
    );
}

#[test]
fn shared_context_share_auto_only_enables_physical_sharing_with_multiple_solvers() {
    let mut ctx = SharedContext::default();
    ctx.set_share_mode(
        rust_clasp::clasp::cli::clasp_cli_options::context_params::ShareMode::ShareAuto,
    );
    assert!(!ctx.physical_share_problem());
    assert!(!ctx.physical_share(ConstraintType::Other));

    let _ = ctx.push_solver();
    ctx.set_share_mode(
        rust_clasp::clasp::cli::clasp_cli_options::context_params::ShareMode::ShareAuto,
    );
    assert!(ctx.physical_share_problem());
    assert!(ctx.physical_share(ConstraintType::Other));
}

#[test]
fn shared_context_start_add_constraints_unfreezes_existing_step_state() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    ctx.request_step_var();
    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());
    assert!(ctx.frozen());
    assert_ne!(ctx.step_literal().var(), 0);

    let _ = ctx.start_add_constraints();
    assert!(!ctx.frozen());
    assert_eq!(ctx.step_literal().var(), 0);
}

#[test]
fn shared_context_solve_multi_reserves_step_literal_capacity() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    ctx.set_solve_mode(SolveMode::SolveMulti);

    let _ = ctx.start_add_constraints();

    assert_eq!(
        ctx.short_implications().size(),
        ((ctx.num_vars() + 1) << 1) + 2
    );
}

#[test]
fn shared_context_end_init_runs_sat_preprocessor_before_solver_finalization() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    ctx.set_sat_prepro(Some(Box::new(
        rust_clasp::clasp::asp_preprocessor::SatPreprocessor::new(),
    )));
    let _ = ctx.start_add_constraints();
    let pre = ctx.sat_prepro_mut().expect("sat preprocessor");
    assert!(pre.add_clause(&[pos_lit(a)]));
    assert!(pre.add_clause(&[neg_lit(a), pos_lit(b)]));

    assert!(ctx.end_init());
    assert!(ctx.master_ref().is_true(pos_lit(a)));
    assert!(ctx.master_ref().is_true(pos_lit(b)));
}

#[test]
fn shared_context_num_unary_excludes_requested_step_literal() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    ctx.request_step_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.add_unary(pos_lit(a)));

    assert!(ctx.end_init());
    assert_eq!(ctx.num_unary(), 1);
}

#[test]
fn shared_context_preprocess_short_strengthens_ternary_implications() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert_eq!(
        ctx.add_imp(&[neg_lit(a), neg_lit(b)], ConstraintType::Static),
        1
    );
    assert_eq!(
        ctx.add_imp(
            &[neg_lit(a), pos_lit(b), pos_lit(c)],
            ConstraintType::Static
        ),
        1
    );
    assert!(ctx.end_init());
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 1);

    assert!(ctx.preprocess_short());
    assert_eq!(ctx.num_binary(), 2);
    assert_eq!(ctx.num_ternary(), 0);
}

#[test]
fn shared_context_simplify_removes_satisfied_explicit_constraints() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();

    {
        let solver = ctx.start_add_constraints();
        let mut creator = rust_clasp::clasp::clause::ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(pos_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(pos_lit(d))
                .end_with_defaults()
                .ok()
        );
    }
    assert!(ctx.end_init());
    assert_eq!(ctx.master_ref().num_constraints(), 1);

    assert!(
        ctx.master()
            .force(pos_lit(a), rust_clasp::clasp::constraint::Antecedent::new())
    );
    ctx.simplify(&[pos_lit(a)], false);
    assert_eq!(ctx.master_ref().num_constraints(), 0);
}

#[test]
fn shared_context_remove_constraint_updates_master_db() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();

    {
        let solver = ctx.start_add_constraints();
        let mut creator = rust_clasp::clasp::clause::ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(pos_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(pos_lit(d))
                .end_with_defaults()
                .ok()
        );
    }
    assert!(ctx.end_init());
    assert_eq!(ctx.master_ref().num_constraints(), 1);

    ctx.remove_constraint(0, true);
    assert_eq!(ctx.master_ref().num_constraints(), 0);
}

#[test]
fn shared_context_problem_complexity_accounts_for_explicit_constraints_when_extended() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();
    ctx.set_frozen(a, true);

    {
        let solver = ctx.start_add_constraints();
        let mut creator = rust_clasp::clasp::clause::ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(pos_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(pos_lit(d))
                .end_with_defaults()
                .ok()
        );
    }
    assert!(ctx.end_init());

    assert!(ctx.problem_complexity() >= ctx.num_constraints());
}

#[test]
fn output_table_filters_and_projects_like_upstream() {
    let mut output = OutputTable::default();
    output.add_predicate("keep", pos_lit(1), 0);
    output.add_predicate("_hide", pos_lit(2), 7);
    output.add_predicate("drop", rust_clasp::clasp::literal::lit_false, 0);
    output.set_var_range(Range32::new(3, 5));
    output.set_filter('_');

    assert_eq!(output.project_mode(), ProjectMode::Output);
    assert_eq!(output.num_preds(), 3);
    assert_eq!(output.num_vars(), 2);

    let removed = output.filter(0);
    assert_eq!(removed, 2);
    assert_eq!(output.num_preds(), 1);
    assert_eq!(output.pred_range()[0].name, "keep");

    output.add_project(pos_lit(4));
    assert!(output.has_project());
    assert_eq!(output.project_mode(), ProjectMode::Project);
    output.clear_project();
    assert!(!output.has_project());
}

#[test]
fn domain_table_simplify_merges_level_and_sign_into_composite_entry() {
    let mut table = DomainTable::default();
    table.add(3, DomModifier::Level, 9, 4, pos_lit(1));
    table.add(3, DomModifier::Sign, 1, 4, pos_lit(1));
    table.add(3, DomModifier::Factor, 7, 1, pos_lit(1));

    assert_eq!(table.simplify(), 2);
    let entries = table.iter().copied().collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind(), DomModifier::True);
    assert_eq!(entries[0].bias(), 9);
    assert!(entries[0].composite());
    assert_eq!(entries[1].kind(), DomModifier::Factor);
}

#[test]
fn domain_table_apply_default_emits_show_and_minimize_preferences() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    ctx.output_mut().add_predicate("shown", pos_lit(a), 1);
    ctx.output_mut().set_var_range(Range32::new(b, b + 1));
    ctx.add_minimize(
        WeightLiteral {
            lit: pos_lit(b),
            weight: 3,
        },
        0,
    );
    let _ = ctx.minimize().expect("materialized minimize data");

    let mut seen = Vec::new();
    DomainTable::apply_default(
        &ctx,
        |lit, pref, prio| seen.push((lit, pref, prio)),
        (DomPref::PrefShow as u32) | (DomPref::PrefMin as u32),
    );

    assert!(seen.contains(&(
        pos_lit(a),
        DomPref::PrefShow as u32,
        DomPref::PrefShow as u32
    )));
    assert!(seen.contains(&(
        pos_lit(b),
        DomPref::PrefShow as u32,
        DomPref::PrefShow as u32
    )));
    assert!(seen.contains(&(
        pos_lit(b),
        DomPref::PrefMin as u32,
        (DomPref::PrefShow as u32) - 1
    )));
}

#[test]
fn distributor_policy_uses_size_lbd_and_type_filters() {
    let policy = DistributorPolicy::new(5, 3, ConstraintType::Conflict as u32);

    assert!(policy.is_candidate_type(3, 2, ConstraintType::Conflict));
    assert!(!policy.is_candidate_type(6, 2, ConstraintType::Conflict));
    assert!(!policy.is_candidate_type(3, 4, ConstraintType::Conflict));
    assert!(!policy.is_candidate_type(3, 2, ConstraintType::Static));
}

#[test]
fn shared_context_end_init_refreshes_acyc_edge_stats_from_ext_graph() {
    let mut ctx = SharedContext::default();
    let mut graph = ExtDepGraph::new(2);
    graph.add_edge(pos_lit(1), 0, 1).expect("edge added");
    assert_eq!(graph.finalize(), 1);
    ctx.set_ext_graph(Some(graph));

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());
    assert_eq!(ctx.stats().acyc_edges, 1);
}

#[test]
fn shared_context_add_stores_generic_constraint_in_master_db() {
    let mut ctx = SharedContext::default();
    let _ = ctx.start_add_constraints();

    ctx.add(Box::new(Constraint::new(NoopConstraint)));

    assert_eq!(ctx.master_ref().num_constraints(), 1);
}

#[test]
fn shared_context_event_handler_exposes_current_handler() {
    let mut ctx = SharedContext::default();
    assert!(ctx.event_handler().is_none());

    let handler = EventHandler::default();
    ctx.set_event_handler(Some(handler));

    assert!(ctx.event_handler().is_some());
}

#[test]
fn shared_context_report_mode_reflects_configured_handler_mode() {
    let mut ctx = SharedContext::default();
    assert_eq!(ctx.report_mode(), ReportMode::Default);

    ctx.set_event_handler_with_mode(Some(EventHandler::default()), ReportMode::Conflict);

    assert_eq!(ctx.report_mode(), ReportMode::Conflict);
}

#[test]
fn shared_context_reporting_dispatches_enter_warning_and_message_events() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observer = LogTraceObserver {
        seen: Rc::clone(&seen),
    };
    let handler = EventHandler::with_observer(Verbosity::VerbosityHigh, observer);
    let mut ctx = SharedContext::default();
    ctx.set_event_handler(Some(handler));

    ctx.enter(Subsystem::SubsystemLoad);
    ctx.warn("careful");
    ctx.report("done", None);

    assert_eq!(
        &*seen.borrow(),
        &[
            (1, 0, b'W' as u32, String::from("careful")),
            (1, 2, b'M' as u32, String::from("done")),
        ]
    );
}

#[test]
fn shared_context_conflict_reporting_dispatches_owned_event_and_clamps_winner() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observer = ConflictTraceObserver {
        seen: Rc::clone(&seen),
    };
    let handler = EventHandler::with_observer(Verbosity::VerbosityHigh, observer);
    let mut ctx = SharedContext::default();
    ctx.set_concurrency(3);
    ctx.set_event_handler_with_mode(Some(handler), ReportMode::Conflict);
    ctx.set_winner(99);

    let solver = Solver::default();
    let info = ConstraintInfo::new(ConstraintType::Conflict);
    ctx.report_conflict(&solver, &[pos_lit(1), neg_lit(2)], &info);

    assert_eq!(ctx.winner(), 3);
    assert_eq!(
        &*seen.borrow(),
        &[(vec![pos_lit(1), neg_lit(2)], ConstraintType::Conflict)]
    );
}

#[test]
fn shared_context_default_dom_pref_follows_domain_heuristic_configuration() {
    let mut ctx = SharedContext::default();
    assert_eq!(ctx.default_dom_pref(), 1u32 << 31);

    let solver = ctx.configuration_mut().add_solver(0);
    solver.heu_id = rust_clasp::clasp::cli::clasp_cli_options::HeuristicType::Domain as u32;
    solver.heuristic.dom_mod = 1;
    solver.heuristic.dom_pref = 23;

    assert_eq!(ctx.default_dom_pref(), 23);
}

#[test]
fn shared_context_is_shared_requires_frozen_context_and_multiple_solvers() {
    let mut ctx = SharedContext::default();
    assert!(!ctx.is_shared());

    let _ = ctx.push_solver();
    assert!(!ctx.is_shared());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());
    assert!(ctx.is_shared());
}

#[test]
fn shared_context_set_toggles_requested_var_flag_only_on_change() {
    let mut ctx = SharedContext::default();
    let var = ctx.add_var();

    assert!(ctx.var_info(var).input());
    ctx.set(
        var,
        rust_clasp::clasp::shared_context::VarInfo::FLAG_INPUT,
        false,
    );
    assert!(!ctx.var_info(var).input());

    ctx.set(
        var,
        rust_clasp::clasp::shared_context::VarInfo::FLAG_INPUT,
        false,
    );
    assert!(!ctx.var_info(var).input());

    ctx.set(
        var,
        rust_clasp::clasp::shared_context::VarInfo::FLAG_INPUT,
        true,
    );
    assert!(ctx.var_info(var).input());
}

#[test]
fn shared_context_set_concurrency_resizes_local_solver_pool() {
    let mut ctx = SharedContext::default();
    assert_eq!(ctx.concurrency(), 1);

    ctx.set_concurrency(3);
    assert_eq!(ctx.concurrency(), 3);

    ctx.set_concurrency(1);
    assert_eq!(ctx.concurrency(), 1);
}

#[test]
fn shared_context_reset_restores_default_state() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    ctx.set_preserve_models(true);
    ctx.set_concurrency(2);

    ctx.reset();

    assert_eq!(ctx.num_vars(), 0);
    assert_eq!(ctx.concurrency(), 1);
    assert!(!ctx.preserve_models());
}

#[test]
fn shared_context_init_stats_resets_target_and_preserves_extended_layout() {
    let mut ctx = SharedContext::default();
    let _ = ctx.master().stats_mut().enable_extended();
    ctx.master().stats_mut().core.choices = 9;

    let mut other = Solver::new();
    other.stats_mut().core.choices = 4;

    ctx.init_stats(&mut other);

    assert_eq!(other.stats().core.choices, 0);
    assert!(other.stats().extra.is_some());
}

#[test]
fn shared_context_solver_stats_and_accu_stats_include_master_and_locals() {
    let mut ctx = SharedContext::default();
    ctx.master().stats_mut().core.choices = 3;
    let solver = ctx.push_solver();
    solver.stats_mut().core.choices = 5;

    assert_eq!(ctx.solver_stats(0).core.choices, 3);
    assert_eq!(ctx.solver_stats(1).core.choices, 5);

    let mut out = SolverStats::default();
    let accum = ctx.accu_stats(&mut out);
    assert_eq!(accum.core.choices, 8);
}

#[test]
fn shared_context_end_init_updates_cached_problem_complexity() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();
    ctx.set_frozen(a, true);

    {
        let solver = ctx.start_add_constraints();
        let mut creator = rust_clasp::clasp::clause::ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(pos_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(pos_lit(d))
                .end_with_defaults()
                .ok()
        );
    }

    assert_eq!(ctx.stats().complexity, 0);
    assert!(ctx.end_init());
    assert_eq!(ctx.stats().complexity, ctx.problem_complexity());
    assert!(ctx.stats().complexity > 0);
}

#[test]
fn shared_context_minimize_is_lazily_materialized_and_can_be_removed() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();

    ctx.add_minimize(
        WeightLiteral {
            lit: pos_lit(a),
            weight: 1,
        },
        0,
    );
    ctx.add_minimize(
        WeightLiteral {
            lit: pos_lit(b),
            weight: 2,
        },
        0,
    );

    assert!(ctx.has_minimize());
    assert!(ctx.minimize_no_create().is_none());

    let first_ptr = {
        let data = ctx.minimize().expect("minimize data");
        let lits = data.iter().copied().collect::<Vec<_>>();
        assert_eq!(
            lits,
            vec![
                WeightLiteral {
                    lit: pos_lit(b),
                    weight: 2,
                },
                WeightLiteral {
                    lit: pos_lit(a),
                    weight: 1,
                },
            ]
        );
        data as *const _
    };

    assert_eq!(
        ctx.minimize_no_create().map(|data| data as *const _),
        Some(first_ptr)
    );

    ctx.remove_minimize();
    assert!(!ctx.has_minimize());
    assert!(ctx.minimize_no_create().is_none());
}

#[test]
fn shared_context_minimize_rebuilds_existing_product_when_new_terms_are_added() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();

    ctx.add_minimize(
        WeightLiteral {
            lit: pos_lit(a),
            weight: 1,
        },
        1,
    );
    let first = ctx.minimize().expect("initial minimize data");
    assert_eq!(first.num_rules(), 1);

    ctx.add_minimize(
        WeightLiteral {
            lit: pos_lit(a),
            weight: 2,
        },
        0,
    );
    let rebuilt = ctx.minimize().expect("rebuilt minimize data");
    assert_eq!(rebuilt.num_rules(), 2);
    assert_eq!(rebuilt.adjust(0), 0);
    assert_eq!(rebuilt.adjust(1), 0);
    assert_eq!(rebuilt.weight_at_level(rebuilt.literals()[0], 0), 1);
    assert_eq!(rebuilt.weight_at_level(rebuilt.literals()[0], 1), 2);
}
