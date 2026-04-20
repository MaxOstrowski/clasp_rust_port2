//! Focused Rust translation of the supported explicit-clause runtime sections
//! from `original_clasp/tests/clause_test.cpp`.

use rust_clasp::clasp::clause::{
    CLAUSE_EXPLICIT, CLAUSE_NO_ADD, CLAUSE_NO_PREPARE, ClauseCreator, ClauseInfo, SharedLiterals,
};
use rust_clasp::clasp::constraint::{Antecedent, ConstraintType, Solver};
use rust_clasp::clasp::literal::{LitVec, Literal, neg_lit, pos_lit, value_false, value_true};
use rust_clasp::clasp::pod_vector::contains;

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

fn create_explicit_clause(
    solver: &mut Solver,
    lits: &[Literal],
    info: ClauseInfo,
) -> *mut rust_clasp::clasp::constraint::ClauseHead {
    let mut owned = LitVec::new();
    owned.assign_from_slice(lits);
    ClauseCreator::create(
        solver,
        &mut owned,
        CLAUSE_EXPLICIT | CLAUSE_NO_ADD | CLAUSE_NO_PREPARE,
        info,
    )
    .local
}

#[test]
fn shared_literals_still_preserve_shared_and_unique_simplify_semantics() {
    let lits = make_lits(3, 3);
    let mut shared = SharedLiterals::new_shareable(&lits, ConstraintType::Conflict, 1);

    let mut solver = test_solver(6);
    solver.set_value(3, value_false, 0);
    solver.set_value(4, value_true, 0);

    assert_eq!(shared.simplify(&solver), 4);
    assert_eq!(shared.size(), 4);
    assert_eq!(
        shared.literals(),
        &[pos_lit(1), pos_lit(2), neg_lit(5), neg_lit(6)]
    );

    let shared = shared.share();
    assert_eq!(shared.ref_count(), 2);
}

#[test]
fn explicit_clause_attaches_watches_and_propagates_reason() {
    let mut solver = test_solver(4);
    let lits = [pos_lit(1), pos_lit(2), neg_lit(3), neg_lit(4)];
    let clause =
        create_explicit_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static));
    let head = unsafe { &mut *clause };

    assert_eq!(solver.num_clause_watches(!lits[0]), 1);
    assert_eq!(solver.num_clause_watches(!lits[1]), 1);

    solver.assume(!lits[0]);
    assert!(solver.propagate());
    solver.assume(!lits[3]);
    assert!(solver.propagate());
    solver.assume(!lits[1]);
    assert!(solver.propagate());
    assert!(solver.is_true(lits[2]));
    assert!(head.locked(&solver));

    let mut antecedent = *solver.reason(lits[2].var());
    let mut reason = LitVec::new();
    antecedent.reason(&mut solver, lits[2], &mut reason);
    assert!(contains(reason.as_slice(), &!lits[0]));
    assert!(contains(reason.as_slice(), &!lits[1]));
    assert!(contains(reason.as_slice(), &!lits[3]));

    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_reports_conflict_and_bumps_learnt_reason_activity() {
    let mut solver = test_solver(4);
    let lits = [pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)];
    let clause = create_explicit_clause(
        &mut solver,
        &lits,
        ClauseInfo::new(ConstraintType::Conflict),
    );
    let head = unsafe { &mut *clause };

    solver.force(!lits[0], Antecedent::new());
    solver.force(!lits[1], Antecedent::new());
    solver.force(!lits[2], Antecedent::new());
    solver.force(!lits[3], Antecedent::new());
    assert!(!solver.propagate());

    let before = head.activity().activity();
    let mut antecedent = solver.conflict_reason();
    let mut reason = LitVec::new();
    antecedent.reason(&mut solver, lits[0], &mut reason);
    assert!(head.activity().activity() > before);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_simplify_removes_false_literals_and_reinitializes_watches() {
    let mut solver = test_solver(6);
    let lits = make_lits(3, 3);
    let clause =
        create_explicit_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static));
    let head = unsafe { &mut *clause };

    solver.force(!lits[0], Antecedent::new());
    solver.force(!lits[1], Antecedent::new());
    assert!(solver.propagate());

    assert!(!head.simplify(&mut solver, false));
    assert_eq!(head.size(), 4);
    let new_lits = head.to_lits();
    assert_eq!(new_lits, vec![lits[2], lits[3], lits[4], lits[5]]);
    assert_eq!(solver.num_clause_watches(!new_lits[0]), 1);
    assert_eq!(solver.num_clause_watches(!new_lits[1]), 1);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_clone_attach_copies_literals_and_watches() {
    let mut solver = test_solver(4);
    let mut other = test_solver(4);
    let lits = [pos_lit(1), pos_lit(2), neg_lit(3), neg_lit(4)];
    let clause =
        create_explicit_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static));
    let head = unsafe { &mut *clause };

    let clone = head.clone_attach(&mut other);
    let clone_head = unsafe { &mut *clone };
    assert_eq!(clone_head.to_lits(), lits.to_vec());
    assert_eq!(other.num_clause_watches(!lits[0]), 1);
    assert_eq!(other.num_clause_watches(!lits[1]), 1);

    clone_head.destroy(Some(&mut other), true);
    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_strengthen_supports_simple_literal_removal() {
    let mut solver = test_solver(4);
    let lits = [pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)];
    let clause =
        create_explicit_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static));
    let head = unsafe { &mut *clause };

    let result = head.strengthen(&mut solver, lits[1], true);
    assert!(result.lit_removed);
    assert!(!result.remove_clause);
    assert_eq!(head.size(), 3);
    assert_eq!(head.to_lits(), vec![lits[0], lits[2], lits[3]]);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_creator_can_return_owned_non_added_clause() {
    let mut solver = test_solver(4);
    let mut clause = LitVec::new();
    clause.assign_from_slice(&[pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)]);

    let result = ClauseCreator::create(
        &mut solver,
        &mut clause,
        CLAUSE_EXPLICIT | CLAUSE_NO_ADD | CLAUSE_NO_PREPARE,
        ClauseInfo::new(ConstraintType::Static),
    );
    assert!(result.ok());
    assert!(!result.local.is_null());
    assert_eq!(solver.num_constraints(), 0);

    unsafe { &mut *result.local }.destroy(Some(&mut solver), true);
}
