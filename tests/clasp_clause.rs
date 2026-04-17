use rust_clasp::clasp::clause::{
    CLAUSE_FORCE_SIMPLIFY, CLAUSE_NOT_CONFLICT, CLAUSE_NOT_ROOT_SAT, CLAUSE_NOT_SAT, ClauseCreator,
    ClauseInfo, ClauseRep, ClauseStatus, SharedLiterals,
};
use rust_clasp::clasp::constraint::{ConstraintType, Solver};
use rust_clasp::clasp::literal::{lit_true, neg_lit, pos_lit, value_false, value_true};

#[test]
fn shared_literals_unique_simplify_compacts_false_literals() {
    let mut shared = SharedLiterals::new_shareable(
        &[pos_lit(1), neg_lit(2), pos_lit(3), neg_lit(4)],
        ConstraintType::Conflict,
        1,
    );
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(4);
    solver.set_value(2, value_true, 1);
    solver.set_value(3, value_false, 1);

    assert_eq!(shared.simplify(&solver), 2);
    assert_eq!(shared.size(), 2);
    assert_eq!(shared.literals(), &[pos_lit(1), neg_lit(4)]);
    assert_eq!(shared.constraint_type(), ConstraintType::Conflict);
}

#[test]
fn shared_literals_shared_simplify_only_reports_remaining_literals() {
    let mut shared = SharedLiterals::new_shareable(
        &[pos_lit(1), neg_lit(2), pos_lit(3), neg_lit(4)],
        ConstraintType::Loop,
        2,
    );
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(4);
    solver.set_value(2, value_true, 1);
    solver.set_value(3, value_false, 1);

    assert_eq!(shared.ref_count(), 2);
    assert_eq!(shared.simplify(&solver), 2);
    assert_eq!(shared.size(), 4);
    assert_eq!(
        shared.literals(),
        &[pos_lit(1), neg_lit(2), pos_lit(3), neg_lit(4)]
    );
}

#[test]
fn prepare_orders_watches_and_tracks_tagged_literals() {
    let mut solver = Solver::new();
    solver.set_num_vars(3);
    solver.set_num_problem_vars(3);
    solver.set_decision_level(2);
    solver.set_value(1, value_false, 1);
    solver.set_value(3, value_true, 2);
    solver.set_tag_literal(neg_lit(2));

    let mut lits = rust_clasp::clasp::literal::LitVec::new();
    lits.push_back(pos_lit(1));
    lits.push_back(pos_lit(2));
    lits.push_back(pos_lit(3));

    let prepared = ClauseCreator::prepare_vec(
        &mut solver,
        &mut lits,
        CLAUSE_FORCE_SIMPLIFY,
        ClauseInfo::new(ConstraintType::Static),
    );

    assert_eq!(prepared.literals(), &[pos_lit(3), pos_lit(2), pos_lit(1)]);
    assert!(prepared.info.tagged());
}

#[test]
fn prepare_force_simplify_turns_tautologies_into_true_literal() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);
    let mut lits = rust_clasp::clasp::literal::LitVec::new();
    lits.push_back(pos_lit(1));
    lits.push_back(neg_lit(1));
    lits.push_back(pos_lit(2));

    let prepared = ClauseCreator::prepare_vec(
        &mut solver,
        &mut lits,
        CLAUSE_FORCE_SIMPLIFY,
        ClauseInfo::new(ConstraintType::Static),
    );

    assert_eq!(prepared.size, 1);
    assert_eq!(prepared.literals(), &[lit_true]);
}

#[test]
fn status_matches_upstream_helper_semantics() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    assert_eq!(ClauseCreator::status(&mut solver, &[]), ClauseStatus::Empty);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Open
    );

    solver.set_value(1, value_false, 0);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Unit
    );

    solver.set_value(1, value_true, 0);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Subsumed
    );

    solver.set_value(1, value_false, 0);
    solver.set_value(2, value_false, 0);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Empty
    );
}

#[test]
fn ignore_clause_follows_sat_and_conflict_flags() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);
    solver.set_decision_level(1);
    solver.set_value(1, value_true, 1);
    solver.set_value(2, value_false, 1);
    solver.set_root_level(0);

    let sat_clause = ClauseRep::prepared(
        &[pos_lit(1), pos_lit(2)],
        ClauseInfo::new(ConstraintType::Static),
    );
    let sat_status = ClauseCreator::status_clause(&mut solver, &sat_clause);
    assert_eq!(sat_status, ClauseStatus::Sat);
    assert!(ClauseCreator::ignore_clause(
        &solver,
        &sat_clause,
        sat_status,
        CLAUSE_NOT_SAT
    ));
    assert!(!ClauseCreator::ignore_clause(
        &solver,
        &sat_clause,
        sat_status,
        CLAUSE_NOT_ROOT_SAT,
    ));

    solver.set_value(1, value_false, 1);
    let unsat_clause = ClauseRep::prepared(
        &[pos_lit(1), pos_lit(2)],
        ClauseInfo::new(ConstraintType::Static),
    );
    let unsat_status = ClauseCreator::status_clause(&mut solver, &unsat_clause);
    assert_eq!(unsat_status, ClauseStatus::Unsat);
    assert!(ClauseCreator::ignore_clause(
        &solver,
        &unsat_clause,
        unsat_status,
        CLAUSE_NOT_CONFLICT,
    ));
}
