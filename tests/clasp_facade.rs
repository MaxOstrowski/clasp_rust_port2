use rust_clasp::clasp::clasp_facade::{
    ClaspConfig, ClaspFacade, Configurator, Prepare, SolveHandle, SolveOptions, StepReady,
    StepStart, Summary,
};
use rust_clasp::clasp::claspfwd::ProblemType;
use rust_clasp::clasp::constraint::Solver;
use rust_clasp::clasp::logic_program::AspOptions;
use rust_clasp::clasp::mt::parallel_solve::{
    ParallelIntegrationFilter, ParallelIntegrationTopology, ParallelSearchMode,
};
use rust_clasp::clasp::parser::ParserOptions;
use rust_clasp::clasp::util::misc_types::{Subsystem, Verbosity};

struct DummyConfigurator;

impl Configurator for DummyConfigurator {
    fn add_propagators(&mut self, _solver: &mut Solver) -> bool {
        true
    }

    fn set_heuristic(&mut self, _solver: &mut Solver) {}
}

#[test]
fn solve_options_default_embeds_parallel_and_enumeration_state() {
    let options = SolveOptions::default();

    assert_eq!(options.base.algorithm.threads, 1);
    assert_eq!(options.base.algorithm.mode, ParallelSearchMode::Compete);
    assert_eq!(options.base.integrate.grace, 1024);
    assert_eq!(
        options.base.integrate.filter,
        ParallelIntegrationFilter::GuidingPath
    );
    assert_eq!(
        options.base.integrate.topology,
        ParallelIntegrationTopology::All
    );
    assert_eq!(options.num_solver(), 1);
    assert!(options.default_portfolio());
    assert!(options.models());
}

#[test]
fn clasp_config_default_layout_matches_upstream_members() {
    let config = ClaspConfig::default();

    assert_eq!(config.solve.base.algorithm.threads, 1);
    assert_eq!(
        config.solve.base.algorithm.mode,
        ParallelSearchMode::Compete
    );
    assert_eq!(config.asp, AspOptions::default());
    assert_eq!(config.parse, ParserOptions::default());
    assert!(!config.only_pre);
    assert!(!config.prepared);
    assert!(config.tester_config().is_none());
    assert!(!config.has_configurator());
    assert!(!config.notify_detach());
}

#[test]
fn clasp_config_allocates_tester_and_stores_configurator_slot() {
    let mut config = ClaspConfig::default();
    let mut configurator = DummyConfigurator;

    let tester = config.add_tester_config();
    tester.resize(2, 2);
    config.set_configurator(Some(&mut configurator), true);

    assert!(config.tester_config().is_some());
    assert!(config.has_configurator());
    assert!(config.notify_detach());
}

#[test]
fn summary_and_events_keep_facade_backlinks() {
    let mut facade = ClaspFacade::default();
    let mut summary = Summary::default();
    summary.init(&facade);

    let start = StepStart::new(&facade);
    let ready = StepReady::new(&summary);
    let prepare = Prepare::new(&mut facade);

    assert!(summary.facade.is_some());
    assert!(start.facade.is_some());
    assert!(ready.summary.is_some());
    assert!(prepare.facade.is_some());
    assert_eq!(start.base.system, Subsystem::SubsystemFacade as u32);
    assert_eq!(start.base.verb, Verbosity::VerbosityQuiet as u32);
}

#[test]
fn clasp_facade_default_layout_matches_private_member_slots() {
    let facade = ClaspFacade::default();
    let handle = SolveHandle::default();

    assert_eq!(facade.problem_type(), ProblemType::Sat);
    assert_eq!(facade.assumptions(), &[]);
    assert_eq!(facade.lower_bounds(), &[]);
    assert!(facade.program().is_none());
    assert_eq!(facade.num_propagators(), 0);
    assert!(!facade.has_heuristic());
    assert!(!facade.has_accu_summary());
    assert!(!facade.has_stats());
    assert!(!facade.has_solve_state());
    assert!(!handle.is_attached());
}
