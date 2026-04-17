//! Partial port of `original_clasp/tests/clause_test.cpp`.

use rust_clasp::clasp::clause::SharedLiterals;
use rust_clasp::clasp::constraint::{ConstraintType, Solver};
use rust_clasp::clasp::literal::{Literal, neg_lit, pos_lit, value_false, value_true};

fn test_solver(num_vars: u32) -> Solver {
    let mut solver = Solver::new();
    solver.set_num_vars(num_vars);
    solver.set_num_problem_vars(num_vars);
    solver
}

fn make_lits(pos: u32, neg: u32) -> Vec<Literal> {
    let mut lits = Vec::with_capacity((pos + neg) as usize);
    for var in 1..=pos {
        lits.push(pos_lit(var));
    }
    for var in (pos + 1)..=(pos + neg) {
        lits.push(neg_lit(var));
    }
    lits
}

#[test]
fn shared_literals_simplify_shared_keeps_storage_and_refcounts() {
    let lits = make_lits(3, 3);
    let mut shared = SharedLiterals::new_shareable(&lits, ConstraintType::Conflict, 1);

    assert!(shared.unique());
    assert_eq!(shared.constraint_type(), ConstraintType::Conflict);
    assert_eq!(shared.size(), 6);

    assert_eq!(shared.share().ref_count(), 2);
    assert!(!shared.unique());

    let mut solver = test_solver(6);
    solver.set_value(3, value_false, 0);
    solver.set_value(4, value_true, 0);

    assert_eq!(shared.simplify(&solver), 4);
    assert_eq!(shared.size(), 6);
    assert_eq!(shared.literals(), lits.as_slice());
    assert!(!shared.release(1));
    assert!(shared.release(1));
}
