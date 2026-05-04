use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::cli::clasp_cli_options::context_params::ShortSimpMode;
use rust_clasp::clasp::constraint::ConstraintType;
use rust_clasp::clasp::literal::{Literal, neg_lit, pos_lit};
use rust_clasp::clasp::shared_context::{EventHandler, EventObserver, LogEvent, SharedContext};
use rust_clasp::clasp::solver_strategies::{ShortMode, UserConfiguration};
use rust_clasp::clasp::util::misc_types::{
    EnterEvent, Event, EventLike, Subsystem, Verbosity, event_cast,
};

type EnterTrace = Rc<RefCell<Vec<(u32, u32, bool)>>>;
type LogTrace = Rc<RefCell<Vec<(u32, u32, u32, String)>>>;

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
