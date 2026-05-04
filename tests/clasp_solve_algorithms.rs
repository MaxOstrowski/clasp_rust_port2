use rust_clasp::clasp::constraint::Solver;
use rust_clasp::clasp::literal::Literal;
use rust_clasp::clasp::solve_algorithms::{
    BasicSolveEvent, BasicSolveEventOp, BasicSolveOptions, Path, SequentialSolve, SolveLimits,
};
use rust_clasp::clasp::util::misc_types::{EventLike, Subsystem, Verbosity};

#[test]
fn solve_limits_match_upstream_default_and_limit_checks() {
    let unlimited = SolveLimits::default();
    assert_eq!(unlimited, SolveLimits::new(u64::MAX, u64::MAX));
    assert!(!unlimited.enabled());
    assert!(!unlimited.reached());

    let conflict_limited = SolveLimits::new(12, u64::MAX);
    assert!(conflict_limited.enabled());
    assert!(!conflict_limited.reached());

    let restart_limited = SolveLimits::new(u64::MAX, 0);
    assert!(restart_limited.enabled());
    assert!(restart_limited.reached());

    let conflict_exhausted = SolveLimits::new(0, 7);
    assert!(conflict_exhausted.reached());
}

#[test]
fn basic_solve_event_constructor_matches_upstream_shape() {
    let solver = Solver::new();
    let event = BasicSolveEvent::new(&solver, BasicSolveEventOp::Restart, 1000, 2000);

    assert_eq!(event.event().system, Subsystem::SubsystemSolve as u32);
    assert_eq!(event.event().verb, Verbosity::VerbosityMax as u32);
    assert_eq!(event.event().op, BasicSolveEventOp::Restart as u32);
    assert_eq!(event.c_limit, 1000);
    assert_eq!(event.l_limit, 2000);

    let deletion = BasicSolveEvent::new(&solver, BasicSolveEventOp::Deletion, 7, 11);
    assert_eq!(deletion.event().op, BasicSolveEventOp::Deletion as u32);
}

#[test]
fn solve_algorithm_path_acquire_copies_and_owns_literals() {
    let mut source = vec![Literal::new(1, false), Literal::new(2, true)];
    let path = Path::acquire(&source);

    assert!(path.owner());
    unsafe {
        assert_eq!(path.as_lit_view(), source.as_slice());
    }
    assert_ne!(path.begin(), source.as_ptr());
    assert_eq!(path.end(), path.begin().wrapping_add(source.len()));

    source[0] = Literal::new(3, false);
    unsafe {
        assert_eq!(path.as_lit_view()[0], Literal::new(1, false));
    }
}

#[test]
fn solve_algorithm_path_borrow_aliases_without_taking_ownership() {
    let mut source = vec![Literal::new(4, false), Literal::new(5, true)];
    let path = Path::borrow(&source);

    assert!(!path.owner());
    assert_eq!(path.begin(), source.as_ptr());
    assert_eq!(path.end(), source.as_ptr().wrapping_add(source.len()));

    source[1] = Literal::new(6, false);
    unsafe {
        assert_eq!(path.as_lit_view()[1], Literal::new(6, false));
    }
}

#[test]
fn basic_solve_options_and_sequential_solve_match_upstream_defaults() {
    let mut options = BasicSolveOptions {
        limit: SolveLimits::new(9, 4),
    };

    assert_eq!(BasicSolveOptions::supported_solvers(), 1);
    assert_eq!(BasicSolveOptions::recommended_solvers(), 1);
    assert_eq!(options.num_solver(), 1);
    assert!(!options.default_portfolio());

    options.set_solvers(8);
    assert_eq!(options.num_solver(), 1);

    let solve = options.create_solve_object();
    assert_eq!(solve, SequentialSolve::new(SolveLimits::new(9, 4)));
    assert_eq!(solve.solve_limit(), SolveLimits::new(9, 4));
    assert!(!solve.interrupted());
}
