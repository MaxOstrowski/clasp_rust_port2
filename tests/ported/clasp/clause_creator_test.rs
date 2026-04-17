//! Partial port of `original_clasp/tests/clause_creator_test.cpp`.

use rust_clasp::clasp::clause::{CLAUSE_FORCE_SIMPLIFY, ClauseCreator, ClauseInfo};
use rust_clasp::clasp::constraint::{ConstraintType, Solver};
use rust_clasp::clasp::literal::{LitVec, lit_false, lit_true, neg_lit, pos_lit, value_true};

fn test_solver(num_vars: u32) -> Solver {
    let mut solver = Solver::new();
    solver.set_num_vars(num_vars);
    solver.set_num_problem_vars(num_vars);
    solver
}

#[test]
fn clause_creator_prepare_removes_duplicate_literals() {
    let mut solver = test_solver(4);
    let a = pos_lit(1);
    let b = pos_lit(2);
    let c = pos_lit(3);
    let d = pos_lit(4);

    let mut creator = ClauseCreator::new(Some(&mut solver));
    let prepared = creator
        .start(ConstraintType::Static)
        .add(a)
        .add(b)
        .add(c)
        .add(a)
        .add(b)
        .add(d)
        .prepare(true);

    assert_eq!(prepared.literals(), &[a, b, c, d]);
    assert_eq!(creator.lits(), &[a, b, c, d]);
}

#[test]
fn clause_creator_prepare_detects_tautologies() {
    let mut solver = test_solver(3);
    let a = pos_lit(1);
    let b = pos_lit(2);
    let c = pos_lit(3);

    let mut creator = ClauseCreator::new(Some(&mut solver));
    let prepared = creator
        .start(ConstraintType::Static)
        .add(a)
        .add(b)
        .add(c)
        .add(a)
        .add(b)
        .add(neg_lit(1))
        .prepare(true);

    assert_eq!(prepared.size, 1);
    assert_eq!(prepared.literals(), &[lit_true]);
    assert_eq!(creator.lits(), &[lit_true]);
}

#[test]
fn clause_creator_prepare_moves_free_watch_literals_to_front() {
    let mut solver = test_solver(4);
    solver.set_decision_level(2);
    solver.set_value(1, value_true, 1);
    solver.set_value(2, value_true, 2);

    let mut creator = ClauseCreator::new(Some(&mut solver));
    let prepared = creator
        .start(ConstraintType::Loop)
        .add(neg_lit(1))
        .add(neg_lit(2))
        .add(neg_lit(2))
        .add(neg_lit(1))
        .add(pos_lit(3))
        .add(pos_lit(4))
        .prepare(true);

    assert_eq!(prepared.literals().len(), 4);
    assert_eq!(&prepared.literals()[..2], &[pos_lit(3), pos_lit(4)]);
    assert_eq!(creator.lits().len(), 4);
    assert_eq!(&creator.lits()[..2], &[pos_lit(3), pos_lit(4)]);
}

#[test]
fn clause_creator_prepare_drops_false_literal_regression_case() {
    let mut solver = test_solver(2);
    let mut clause = LitVec::new();
    clause.push_back(lit_false);
    clause.push_back(pos_lit(1));
    clause.push_back(pos_lit(2));

    let prepared = ClauseCreator::prepare_vec(
        &mut solver,
        &mut clause,
        CLAUSE_FORCE_SIMPLIFY,
        ClauseInfo::new(ConstraintType::Static),
    );

    assert_eq!(prepared.literals(), &[pos_lit(1), pos_lit(2)]);
    assert_eq!(clause.as_slice(), &[pos_lit(1), pos_lit(2)]);
}
