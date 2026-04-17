use std::panic::{self, AssertUnwindSafe};

use rust_clasp::clasp::clingo::ClingoAssignment;
use rust_clasp::clasp::constraint::Solver;
use rust_clasp::clasp::literal::{encode_lit, neg_lit, pos_lit, value_false, value_true};
use rust_clasp::potassco::clingo::AbstractAssignment;
use rust_clasp::potassco::error::Error;

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

#[test]
fn clingo_assignment_matches_upstream_assignment_basics() {
    let mut solver = Solver::new();
    solver.set_id(7);
    let assignment = ClingoAssignment::new(&solver);

    assert_eq!(assignment.solver_id(), 7);
    assert_eq!(assignment.size(), 1);
    assert_eq!(assignment.trail_size(), 1);
    assert_eq!(assignment.trail_begin(0), 0);
    assert_eq!(assignment.trail_at(0), encode_lit(pos_lit(0)));
    assert_eq!(assignment.trail_end(0), 1);

    solver.set_num_problem_vars(2);
    let assignment = ClingoAssignment::new(&solver);
    assert_eq!(assignment.size(), 3);
    assert_eq!(assignment.level_of(encode_lit(pos_lit(1))), u32::MAX);
    assert_eq!(assignment.level_of(encode_lit(pos_lit(2))), u32::MAX);

    solver.set_num_vars(3);
    solver.set_root_level(1);
    solver.set_decision_level(3);
    solver.set_level_start(1, 0);
    solver.set_level_start(2, 1);
    solver.set_level_start(3, 2);
    solver.set_decision(1, pos_lit(3));
    solver.set_decision(2, pos_lit(1));
    solver.set_decision(3, neg_lit(2));
    solver.push_trail_literal(pos_lit(3));
    solver.push_trail_literal(pos_lit(1));
    solver.push_trail_literal(neg_lit(2));
    solver.set_value(1, value_true, 2);
    solver.set_value(2, value_false, 3);
    solver.set_value(3, value_true, 1);

    let assignment = ClingoAssignment::new(&solver);
    assert!(assignment.is_total());
    assert_eq!(assignment.root_level(), 1);
    assert_eq!(assignment.trail_size(), 4);
    assert_eq!(assignment.trail_at(0), encode_lit(pos_lit(0)));
    assert_eq!(assignment.trail_at(1), encode_lit(pos_lit(3)));
    assert_eq!(assignment.trail_at(2), encode_lit(pos_lit(1)));
    assert_eq!(assignment.trail_at(3), encode_lit(neg_lit(2)));
    assert_eq!(assignment.level(), 3);
    assert_eq!(assignment.trail_begin(0), 0);
    assert_eq!(assignment.trail_end(0), 1);
    assert_eq!(assignment.trail_begin(1), 1);
    assert_eq!(assignment.trail_end(1), 2);
    assert_eq!(assignment.trail_begin(2), 2);
    assert_eq!(assignment.trail_end(2), 3);
    assert_eq!(assignment.trail_begin(3), 3);
    assert_eq!(assignment.trail_end(3), 4);
    assert_eq!(assignment.level_of(encode_lit(pos_lit(1))), 2);
    assert_eq!(assignment.level_of(encode_lit(pos_lit(2))), 3);
    assert_eq!(
        assignment.value(encode_lit(neg_lit(2))),
        rust_clasp::potassco::basic_types::TruthValue::True
    );
}

#[test]
fn clingo_assignment_includes_problem_vars_not_yet_committed_to_solver() {
    let mut solver = Solver::new();
    solver.set_num_problem_vars(2);
    let assignment = ClingoAssignment::new(&solver);

    assert_eq!(assignment.size(), 3);
    assert_eq!(assignment.trail_size(), 1);
    assert_eq!(assignment.trail_begin(0), 0);
    assert_eq!(assignment.trail_at(0), encode_lit(pos_lit(0)));
    assert_eq!(assignment.trail_end(0), 1);
    assert!(!assignment.is_total());
    assert_eq!(assignment.unassigned(), 2);
    assert_eq!(
        assignment.value(encode_lit(pos_lit(1))),
        rust_clasp::potassco::basic_types::TruthValue::Free
    );
    assert_eq!(
        assignment.value(encode_lit(pos_lit(2))),
        rust_clasp::potassco::basic_types::TruthValue::Free
    );
}

#[test]
fn clingo_assignment_matches_upstream_assignment_queries() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);
    solver.set_decision_level(2);
    solver.set_level_start(1, 0);
    solver.set_level_start(2, 1);
    solver.set_decision(1, pos_lit(1));
    solver.set_decision(2, neg_lit(2));
    solver.push_trail_literal(pos_lit(1));
    solver.push_trail_literal(neg_lit(2));
    solver.set_value(1, value_true, 1);
    solver.set_value(2, value_false, 2);

    let assignment = ClingoAssignment::new(&solver);
    let lit1 = encode_lit(pos_lit(1));
    let lit2 = encode_lit(pos_lit(2));

    assert!(!assignment.has_conflict());
    assert_eq!(assignment.level(), 2);
    assert_eq!(
        assignment.value(lit1),
        rust_clasp::potassco::basic_types::TruthValue::True
    );
    assert_eq!(
        assignment.value(lit2),
        rust_clasp::potassco::basic_types::TruthValue::False
    );
    assert!(assignment.is_true(lit1));
    assert!(assignment.is_false(lit2));
    assert!(assignment.is_true(encode_lit(neg_lit(2))));
    assert_eq!(assignment.level_of(lit1), 1);
    assert_eq!(assignment.level_of(lit2), 2);
    assert!(!assignment.has_lit(encode_lit(pos_lit(3))));
    assert_eq!(assignment.decision(0), encode_lit(pos_lit(0)));
    assert_eq!(assignment.decision(1), lit1);
    assert_eq!(assignment.decision(2), encode_lit(neg_lit(2)));
    assert_eq!(assignment.trail_size(), 3);
    assert_eq!(assignment.trail_at(0), encode_lit(pos_lit(0)));
    assert_eq!(assignment.trail_at(1), lit1);
    assert_eq!(assignment.trail_at(2), encode_lit(neg_lit(2)));
    assert_eq!(assignment.trail_begin(0), 0);
    assert_eq!(assignment.trail_end(0), 1);
    assert_eq!(assignment.trail_begin(1), 1);
    assert_eq!(assignment.trail_end(1), 2);
    assert_eq!(assignment.trail_begin(2), 2);
    assert_eq!(assignment.trail_end(2), 3);
}

#[test]
fn clingo_assignment_reports_root_fixed_literals_and_conflicts() {
    let mut solver = Solver::new();
    solver.set_num_vars(1);
    solver.set_num_problem_vars(1);
    solver.set_root_level(0);
    solver.set_value(1, value_true, 0);
    solver.push_trail_literal(pos_lit(1));

    let assignment = ClingoAssignment::new(&solver);
    let lit1 = encode_lit(pos_lit(1));
    assert!(assignment.is_fixed(lit1));
    assert!(!assignment.has_conflict());

    solver.set_has_conflict(true);
    let assignment = ClingoAssignment::new(&solver);
    assert!(assignment.has_conflict());
}

#[test]
fn clingo_assignment_reports_upstream_precondition_errors() {
    let solver = Solver::new();
    let assignment = ClingoAssignment::new(&solver);

    let invalid_lit = catch_error(|| {
        let _ = assignment.value(3);
    });
    assert_eq!(
        invalid_lit,
        Error::InvalidArgument("Invalid literal".to_owned())
    );

    let invalid_level = catch_error(|| {
        let _ = assignment.decision(1);
    });
    assert_eq!(
        invalid_level,
        Error::InvalidArgument("Invalid decision level".to_owned())
    );

    let invalid_trail = catch_error(|| {
        let _ = assignment.trail_at(1);
    });
    assert_eq!(
        invalid_trail,
        Error::InvalidArgument("Invalid trail position".to_owned())
    );
}
