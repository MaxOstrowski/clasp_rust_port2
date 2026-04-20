//! Focused Rust translation of the Bundle A `clause_creator_test.cpp` coverage.

use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::clause::{
    CLAUSE_EXPLICIT, CLAUSE_FLAG_NONE, CLAUSE_FORCE_SIMPLIFY, ClauseCreator, ClauseInfo,
    ClauseStatus, SharedLiterals,
};
use rust_clasp::clasp::constraint::{ConstraintType, DecisionHeuristic, Solver};
use rust_clasp::clasp::literal::{LitVec, lit_false, lit_true, neg_lit, pos_lit, value_true};
use rust_clasp::clasp::shared_context::SharedContext;

fn setup_context(num_vars: u32) -> (SharedContext, Vec<rust_clasp::clasp::literal::Literal>) {
    let mut ctx = SharedContext::new();
    let lits = (0..num_vars).map(|_| pos_lit(ctx.add_var())).collect();
    (ctx, lits)
}

#[test]
fn clause_creator_prepare_still_matches_upstream_simplify_helpers() {
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(4);
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

    assert_eq!(&prepared.literals()[..2], &[pos_lit(3), pos_lit(4)]);
    assert!(prepared.literals()[2..].contains(&neg_lit(1)));
    assert!(prepared.literals()[2..].contains(&neg_lit(2)));

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

    let mut taut_lits = LitVec::new();
    taut_lits.assign_from_slice(&[pos_lit(1), neg_lit(1), pos_lit(3)]);
    let taut = ClauseCreator::prepare_vec(
        &mut solver,
        &mut taut_lits,
        CLAUSE_FORCE_SIMPLIFY,
        ClauseInfo::new(ConstraintType::Static),
    );
    assert_eq!(taut.literals(), &[lit_true]);
}

#[test]
fn clause_creator_end_handles_empty_units_sat_and_problem_constraints() {
    let (mut ctx, lits) = setup_context(4);
    let mut creator = ClauseCreator::new(None);
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    unsafe {
        creator.set_solver(&mut *solver_ptr);

        let empty = creator.start(ConstraintType::Static).end_with_defaults();
        assert!(!empty.ok());
        assert!((*solver_ptr).has_conflict());
        (*solver_ptr).clear_conflict();

        let fact = creator
            .start(ConstraintType::Static)
            .add(lits[0])
            .end_with_defaults();
        assert!(fact.ok());
        assert!((*solver_ptr).is_true(lits[0]));

        let sat = creator
            .start(ConstraintType::Static)
            .add(lits[0])
            .add(lits[1])
            .end_with_defaults();
        assert!(sat.ok());

        (*solver_ptr).force(!lits[1], rust_clasp::clasp::constraint::Antecedent::new());
        let unit = creator
            .start(ConstraintType::Static)
            .add(lits[1])
            .add(lits[2])
            .end_with_defaults();
        assert!(unit.ok());
        assert!((*solver_ptr).is_true(lits[2]));

        let binary = creator
            .start(ConstraintType::Static)
            .add(lits[0])
            .add(lits[3])
            .end_with_defaults();
        assert!(binary.ok());
    }
    assert_eq!(ctx.num_binary(), 1);

    let (mut explicit_ctx, explicit_lits) = setup_context(4);
    let mut explicit_creator = ClauseCreator::new(None);
    let explicit_solver_ptr = explicit_ctx.start_add_constraints() as *mut Solver;
    unsafe {
        explicit_creator.set_solver(&mut *explicit_solver_ptr);
        let explicit = explicit_creator
            .start(ConstraintType::Static)
            .add(explicit_lits[0])
            .add(explicit_lits[1])
            .add(explicit_lits[2])
            .add(explicit_lits[3])
            .end_with_defaults();
        assert!(explicit.ok());
        assert!(!explicit.local.is_null());
    }
    assert_eq!(explicit_ctx.num_constraints(), 1);
}

#[test]
fn clause_creator_creates_and_orders_learnt_explicit_clauses() {
    let (mut ctx, lits) = setup_context(4);
    let _ = ctx.start_add_constraints();
    let _ = ctx.end_init();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        (*solver_ptr).assume(!lits[1]);
        (*solver_ptr).propagate();
        (*solver_ptr).assume(!lits[2]);
        (*solver_ptr).propagate();
        (*solver_ptr).assume(!lits[3]);
        (*solver_ptr).propagate();

        let mut creator = ClauseCreator::new(Some(&mut *solver_ptr));
        let learnt = creator
            .start(ConstraintType::Conflict)
            .add(lits[0])
            .add(lits[1])
            .add(lits[2])
            .add(lits[3])
            .end(CLAUSE_FLAG_NONE);
        assert!(learnt.ok());
        assert!((*solver_ptr).is_true(lits[0]));
        assert_eq!((*solver_ptr).num_learnt_constraints(), 1);
        let head = &*learnt.local;
        assert_eq!(head.to_lits()[..2], [lits[0], lits[3]]);
    }
}

#[derive(Clone, Default)]
struct RecordingHeuristic {
    seen: Rc<RefCell<Vec<(usize, ConstraintType)>>>,
}

impl DecisionHeuristic for RecordingHeuristic {
    fn new_constraint(
        &mut self,
        _solver: &Solver,
        lits: &[rust_clasp::clasp::literal::Literal],
        ty: ConstraintType,
    ) {
        self.seen.borrow_mut().push((lits.len(), ty));
    }
}

#[test]
fn clause_creator_notifies_minimal_heuristic_interface() {
    let (mut ctx, lits) = setup_context(4);
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    let seen = Rc::new(RefCell::new(Vec::new()));
    unsafe {
        (*solver_ptr).set_heuristic(RecordingHeuristic {
            seen: Rc::clone(&seen),
        });
        let mut creator = ClauseCreator::new(Some(&mut *solver_ptr));

        let _ = creator
            .start(ConstraintType::Static)
            .add(lits[0])
            .add(lits[1])
            .add(lits[2])
            .add(lits[3])
            .end_with_defaults();
    }
    let _ = ctx.end_init();
    unsafe {
        let mut creator = ClauseCreator::new(Some(&mut *solver_ptr));
        let _ = creator
            .start(ConstraintType::Loop)
            .add(lits[0])
            .add(!lits[1])
            .add(!lits[2])
            .end(CLAUSE_FLAG_NONE);
    }

    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], (4, ConstraintType::Static));
    assert_eq!(seen[1], (3, ConstraintType::Loop));
}

#[test]
fn clause_creator_integrate_handles_unit_and_conflict_shared_literals() {
    let (mut ctx, lits) = setup_context(5);
    let _ = ctx.start_add_constraints();
    let _ = ctx.end_init();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        (*solver_ptr).assume(!lits[0]);
        (*solver_ptr).propagate();
        (*solver_ptr).assume(!lits[1]);
        (*solver_ptr).propagate();
    }

    let unit_shared =
        SharedLiterals::new_shareable(&[lits[4], lits[0], lits[1]], ConstraintType::Other, 1);
    let unit =
        unsafe { ClauseCreator::integrate(&mut *solver_ptr, &unit_shared, CLAUSE_FLAG_NONE) };
    assert!(unit.ok());
    assert!(unsafe { (*solver_ptr).is_true(lits[4]) });

    let conflict_shared = SharedLiterals::new_shareable(&[], ConstraintType::Other, 1);
    let conflict =
        unsafe { ClauseCreator::integrate(&mut *solver_ptr, &conflict_shared, CLAUSE_EXPLICIT) };
    assert!(!conflict.ok());
    assert!(unsafe { (*solver_ptr).has_conflict() });
}

#[test]
fn clause_creator_status_matches_upstream_helper_cases() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    assert_eq!(ClauseCreator::status(&mut solver, &[]), ClauseStatus::Empty);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Open
    );
    solver.set_value(1, value_true, 0);
    assert_eq!(
        ClauseCreator::status(&mut solver, &[pos_lit(1), pos_lit(2)]),
        ClauseStatus::Subsumed
    );
}
