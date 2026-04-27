//! Focused Rust translation of the supported explicit, shared, and loop-formula
//! runtime sections from `original_clasp/tests/clause_test.cpp`.

use rust_clasp::clasp::clause::{
    CLAUSE_EXPLICIT, CLAUSE_NO_ADD, CLAUSE_NO_PREPARE, ClauseCreator, ClauseHead, ClauseInfo,
    ClauseRep, LoopFormulaHandle, SharedLiterals, new_contracted_clause, new_loop_formula,
    new_shared_clause,
};
use rust_clasp::clasp::constraint::{Constraint, ConstraintType};
use rust_clasp::clasp::literal::{
    LitVec, Literal, neg_lit, pos_lit, value_false, value_free, value_true,
};
use rust_clasp::clasp::pod_vector::contains;
use rust_clasp::clasp::shared_context::SharedContext;
use rust_clasp::clasp::solver::{Antecedent, Solver};

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
) -> *mut ClauseHead {
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

fn create_loop_formula_fixture() -> (SharedContext, LoopFormulaHandle, [Literal; 6]) {
    let mut ctx = SharedContext::new();
    let a1 = pos_lit(ctx.add_var());
    let a2 = pos_lit(ctx.add_var());
    let a3 = pos_lit(ctx.add_var());
    let b1 = pos_lit(ctx.add_var());
    let b2 = pos_lit(ctx.add_var());
    let b3 = pos_lit(ctx.add_var());
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    let _ = ctx.end_init();
    unsafe {
        let solver = &mut *solver_ptr;
        solver.assume(!b1);
        solver.assume(!b2);
        solver.assume(!b3);
        assert!(solver.propagate());
        let clause = ClauseRep::prepared(&[!a1, b3, b2, b1], ClauseInfo::new(ConstraintType::Loop));
        let atoms = [!a1, !a2, !a3];
        let loop_formula = new_loop_formula(solver, &clause, &atoms, true);
        solver.add_learnt_constraint(
            loop_formula.constraint,
            loop_formula.size(),
            ConstraintType::Loop,
        );
        let antecedent = Antecedent::from_constraint_ptr(loop_formula.constraint);
        assert!(solver.force(!a1, antecedent));
        assert!(solver.force(!a2, antecedent));
        assert!(solver.force(!a3, antecedent));
        assert!(solver.propagate());
        (ctx, loop_formula, [a1, a2, a3, b1, b2, b3])
    }
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
fn shared_literals_parity_aliases_delegate_to_existing_behavior() {
    let shared = SharedLiterals::new(&[pos_lit(1), neg_lit(2)], ConstraintType::Conflict);
    let ptr_range = shared.literals().as_ptr_range();

    assert_eq!(shared.ref_count(), 1);
    assert_eq!(shared.r#type(), ConstraintType::Conflict);
    assert_eq!(shared.begin(), ptr_range.start);
    assert_eq!(shared.end(), ptr_range.end);

    let shared = shared.share();
    assert_eq!(shared.ref_count(), 2);
    assert!(!shared.release_one());
    assert_eq!(shared.ref_count(), 1);
    assert!(shared.release_one());
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
fn learnt_clause_creation_contracts_false_tail_and_restores_on_backtrack() {
    let mut solver = test_solver(12);
    let mut clause = LitVec::new();
    clause.push_back(pos_lit(1));
    for var in 2..=12 {
        assert!(solver.assume(neg_lit(var)));
        clause.push_back(pos_lit(var));
    }
    let all_lits = clause.as_slice().to_vec();

    solver.strategies_mut().compress = 6;
    let head = unsafe {
        &mut *ClauseCreator::create(
            &mut solver,
            &mut clause,
            0,
            ClauseInfo::new(ConstraintType::Conflict),
        )
        .local
    };

    assert!(head.size() < all_lits.len() as u32);
    let clause_lits = head.to_lits();
    assert_eq!(clause_lits.len(), all_lits.len());
    for lit in &all_lits {
        assert!(clause_lits.contains(lit));
    }

    let mut antecedent = Antecedent::from_constraint_ptr(head.constraint_ptr());
    let mut reason = LitVec::new();
    antecedent.reason(&mut solver, pos_lit(1), &mut reason);
    assert_eq!(reason.len(), all_lits.len() - 1);

    solver.undo_until(0);
    assert_eq!(head.size(), all_lits.len() as u32);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn new_contracted_clause_keeps_hidden_tail_in_reason_but_not_active_size() {
    let mut solver = test_solver(12);
    let mut lits = vec![pos_lit(1), pos_lit(2), pos_lit(3)];
    for var in 4..=12 {
        assert!(solver.assume(neg_lit(var)));
        lits.push(pos_lit(var));
    }
    let rep = ClauseRep::create(&lits, ClauseInfo::new(ConstraintType::Conflict));
    let head = unsafe { &mut *new_contracted_clause(&mut solver, &rep, 3, true) };
    solver.add_learnt_constraint(head.constraint_ptr(), rep.size, ConstraintType::Conflict);

    assert!(head.size() < lits.len() as u32);
    let clause_lits = head.to_lits();
    assert_eq!(clause_lits.len(), lits.len());
    for lit in &lits {
        assert!(clause_lits.contains(lit));
    }

    assert!(solver.assume(neg_lit(1)));
    assert!(solver.propagate());
    assert!(solver.assume(neg_lit(3)));
    assert!(solver.propagate());
    assert!(solver.is_true(pos_lit(2)));

    let mut antecedent = *solver.reason(pos_lit(2).var());
    let mut reason = LitVec::new();
    antecedent.reason(&mut solver, pos_lit(2), &mut reason);
    assert_eq!(reason.len(), lits.len() - 1);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn clause_contracted_accessor_matches_explicit_and_shared_runtime_state() {
    let mut solver = test_solver(6);
    let lits = [
        pos_lit(1),
        pos_lit(2),
        pos_lit(3),
        pos_lit(4),
        pos_lit(5),
        pos_lit(6),
    ];
    for var in 4..=6 {
        assert!(solver.assume(neg_lit(var)));
    }

    let rep = ClauseRep::create(&lits, ClauseInfo::new(ConstraintType::Conflict));
    let head = unsafe { &mut *new_contracted_clause(&mut solver, &rep, 3, false) };
    assert!(head.contracted());
    assert!(!head.is_small());
    assert!(!head.strengthened());
    head.destroy(Some(&mut solver), true);

    let mut solver = test_solver(4);
    let explicit = create_explicit_clause(
        &mut solver,
        &[pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)],
        ClauseInfo::new(ConstraintType::Static),
    );
    let explicit = unsafe { &mut *explicit };
    assert!(!explicit.contracted());
    assert!(explicit.is_small());
    assert_eq!(explicit.compute_alloc_size(), 32);
    explicit.destroy(Some(&mut solver), true);

    let mut solver = test_solver(4);
    let shared = unsafe {
        &mut *new_shared_clause(
            &mut solver,
            &[pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)],
            ClauseInfo::new(ConstraintType::Static),
        )
    };
    assert!(!shared.contracted());
    assert!(shared.is_small());
    assert!(!shared.strengthened());
    shared.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_large_clause_keeps_non_small_storage_after_shrinking_to_five_literals() {
    let mut solver = test_solver(6);
    let head = unsafe {
        &mut *create_explicit_clause(
            &mut solver,
            &[
                pos_lit(1),
                pos_lit(2),
                pos_lit(3),
                pos_lit(4),
                pos_lit(5),
                pos_lit(6),
            ],
            ClauseInfo::new(ConstraintType::Static),
        )
    };

    let alloc_before = head.compute_alloc_size();
    assert!(!head.is_small());
    assert!(!head.strengthened());

    let result = head.strengthen(&mut solver, pos_lit(6), false);
    assert!(result.lit_removed);
    assert!(!result.remove_clause);
    assert_eq!(head.size(), 5);
    assert!(!head.is_small());
    assert!(!head.strengthened());
    assert!(head.compute_alloc_size() < alloc_before);
    assert_ne!(head.compute_alloc_size(), 32);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn contracted_learnt_clause_strengthening_preserves_retained_allocation_metadata() {
    let mut solver = test_solver(12);
    let mut clause = LitVec::new();
    clause.push_back(pos_lit(1));
    for var in 2..=12 {
        assert!(solver.assume(neg_lit(var)));
        clause.push_back(pos_lit(var));
    }

    solver.strategies_mut().compress = 4;
    let head = unsafe {
        &mut *ClauseCreator::create(
            &mut solver,
            &mut clause,
            0,
            ClauseInfo::new(ConstraintType::Conflict),
        )
        .local
    };

    let alloc_before = head.compute_alloc_size();
    assert!(head.contracted());
    assert!(!head.is_small());
    assert!(!head.strengthened());

    assert!(head.strengthen(&mut solver, pos_lit(12), true).lit_removed);
    assert!(head.strengthened());
    assert_eq!(head.compute_alloc_size(), alloc_before);

    solver.undo_until(solver.level(9).saturating_sub(1));
    assert!(head.size() > 3);

    let removed = [
        pos_lit(2),
        pos_lit(6),
        pos_lit(9),
        pos_lit(8),
        pos_lit(5),
        pos_lit(4),
        pos_lit(3),
    ];
    for lit in removed {
        assert!(head.strengthen(&mut solver, lit, true).lit_removed);
    }
    let clause_lits = head.to_lits();
    assert!(head.size() <= 4);
    for lit in removed {
        assert!(!clause_lits.contains(&lit));
    }
    assert!(!head.is_small());
    assert!(head.strengthened());
    assert_eq!(head.compute_alloc_size(), alloc_before);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn local_learnt_clause_memory_accounting_tracks_explicit_and_shared_lifetimes() {
    let mut solver = test_solver(8);

    let explicit = create_explicit_clause(
        &mut solver,
        &[
            pos_lit(1),
            pos_lit(2),
            pos_lit(3),
            pos_lit(4),
            pos_lit(5),
            pos_lit(6),
        ],
        ClauseInfo::new(ConstraintType::Conflict),
    );
    let explicit_bytes = unsafe { u64::from((*explicit).compute_alloc_size()) };
    assert_eq!(solver.learnt_bytes(), explicit_bytes);

    let shared = new_shared_clause(
        &mut solver,
        &[pos_lit(1), pos_lit(2), pos_lit(3), pos_lit(4)],
        ClauseInfo::new(ConstraintType::Conflict),
    );
    assert_eq!(solver.learnt_bytes(), explicit_bytes + 32);

    unsafe {
        (*shared).destroy(Some(&mut solver), true);
    }
    assert_eq!(solver.learnt_bytes(), explicit_bytes);

    unsafe {
        (*explicit).destroy(Some(&mut solver), true);
    }
    assert_eq!(solver.learnt_bytes(), 0);
}

#[test]
fn integrate_long_unshared_clause_uses_explicit_storage_and_explicit_allocation_size() {
    let mut solver = test_solver(6);
    let shared = SharedLiterals::new(
        &[
            pos_lit(1),
            pos_lit(2),
            pos_lit(3),
            pos_lit(4),
            pos_lit(5),
            pos_lit(6),
        ],
        ConstraintType::Conflict,
    );

    let result = ClauseCreator::integrate(&mut solver, &shared, CLAUSE_NO_ADD);
    assert!(result.ok());
    assert!(!result.local.is_null());

    let head = unsafe { &mut *result.local };
    assert!(!head.is_small());
    assert_eq!(solver.learnt_bytes(), u64::from(head.compute_alloc_size()));
    assert!(head.compute_alloc_size() > 32);

    head.destroy(Some(&mut solver), true);
    assert_eq!(solver.learnt_bytes(), 0);
}

#[test]
fn integrate_long_shared_clause_uses_local_shared_surrogate_when_physical_share_is_enabled() {
    let mut ctx = SharedContext::new();
    ctx.set_physical_share_learnts(true);
    for _ in 0..6 {
        let _ = ctx.add_var();
    }
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    let _ = ctx.end_init();
    let solver = unsafe { &mut *solver_ptr };

    let shared = SharedLiterals::new(
        &[
            pos_lit(1),
            pos_lit(2),
            pos_lit(3),
            pos_lit(4),
            pos_lit(5),
            pos_lit(6),
        ],
        ConstraintType::Conflict,
    );

    let result = ClauseCreator::integrate(solver, &shared, CLAUSE_NO_ADD);
    assert!(result.ok());
    assert!(!result.local.is_null());

    let head = unsafe { &mut *result.local };
    assert_eq!(head.size(), 6);
    assert!(head.is_small());
    assert_eq!(head.compute_alloc_size(), 32);
    assert_eq!(solver.learnt_bytes(), 32);

    head.destroy(Some(solver), true);
    assert_eq!(solver.learnt_bytes(), 0);
}

#[test]
fn clone_attach_preserves_integrated_shared_local_clause_runtime() {
    let mut ctx = SharedContext::new();
    ctx.set_physical_share_learnts(true);
    for _ in 0..6 {
        let _ = ctx.add_var();
    }
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    let _ = ctx.end_init();
    let solver = unsafe { &mut *solver_ptr };

    let shared = SharedLiterals::new(
        &[
            pos_lit(1),
            pos_lit(2),
            neg_lit(3),
            neg_lit(4),
            neg_lit(5),
            neg_lit(6),
        ],
        ConstraintType::Conflict,
    );
    let local = ClauseCreator::integrate(solver, &shared, CLAUSE_NO_ADD).local;
    let clause = unsafe { &mut *local };

    let mut other = test_solver(6);
    let clone = unsafe { &mut *clause.clone_attach(&mut other) };
    assert_eq!(clone.to_lits(), clause.to_lits());
    assert_eq!(clone.size(), 6);
    assert!(clone.is_small());
    assert_eq!(clone.compute_alloc_size(), 32);
    assert_eq!(other.num_clause_watches(!pos_lit(1)), 1);
    assert_eq!(other.num_clause_watches(!pos_lit(2)), 1);

    assert!(other.assume(neg_lit(1)));
    assert!(other.propagate());
    assert!(other.assume(pos_lit(4)));
    assert!(other.propagate());
    assert!(other.assume(pos_lit(5)));
    assert!(other.propagate());
    assert!(other.assume(pos_lit(6)));
    assert!(other.propagate());
    assert!(other.assume(neg_lit(2)));
    assert!(other.propagate());
    assert!(other.is_true(neg_lit(3)));

    clone.destroy(Some(&mut other), true);
    clause.destroy(Some(solver), true);
}

#[test]
fn strengthen_contracted_clause_restores_active_prefix_on_backtrack() {
    let mut solver = test_solver(12);
    let mut clause = LitVec::new();
    clause.push_back(pos_lit(1));
    for var in 2..=12 {
        assert!(solver.assume(neg_lit(var)));
        clause.push_back(pos_lit(var));
    }

    solver.strategies_mut().compress = 4;
    let head = unsafe {
        &mut *ClauseCreator::create(
            &mut solver,
            &mut clause,
            0,
            ClauseInfo::new(ConstraintType::Conflict),
        )
        .local
    };
    let active_before = head.size();

    let result = head.strengthen(&mut solver, pos_lit(12), true);
    assert!(result.lit_removed);
    assert!(!head.to_lits().contains(&pos_lit(12)));
    assert_eq!(head.size(), active_before);

    solver.undo_until(solver.level(9).saturating_sub(1));
    assert!(head.size() > active_before);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn strengthen_contracted_clause_without_extend_keeps_active_size_fixed() {
    let mut solver = test_solver(6);
    let lits = [
        pos_lit(1),
        pos_lit(2),
        pos_lit(3),
        pos_lit(4),
        pos_lit(5),
        pos_lit(6),
    ];
    for var in 2..=6 {
        assert!(solver.assume(neg_lit(var)));
    }
    let rep = ClauseRep::create(&lits, ClauseInfo::new(ConstraintType::Conflict));
    let head = unsafe { &mut *new_contracted_clause(&mut solver, &rep, 4, false) };
    solver.add_learnt_constraint(head.constraint_ptr(), 4, ConstraintType::Conflict);

    assert_eq!(head.size(), 4);
    let result = head.strengthen(&mut solver, pos_lit(2), true);
    assert!(result.lit_removed);
    assert_eq!(head.size(), 4);

    solver.undo_until(0);
    assert_eq!(head.size(), 4);

    head.destroy(Some(&mut solver), true);
}

#[test]
fn explicit_clause_strengthen_can_downgrade_to_implicit_short_clause() {
    let mut ctx = SharedContext::new();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    let _ = ctx.end_init();
    let solver = unsafe { &mut *solver_ptr };

    let head = unsafe {
        &mut *create_explicit_clause(solver, &[a, b, c], ClauseInfo::new(ConstraintType::Static))
    };
    let result = head.strengthen(solver, c, true);
    assert!(result.lit_removed);
    assert!(result.remove_clause);

    head.destroy(Some(solver), true);

    assert!(solver.assume(!a));
    assert!(solver.propagate());
    assert!(solver.is_true(b));
    assert_eq!(solver.value(c.var()), value_free);
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

#[test]
fn shared_clause_attaches_watches_and_propagates_reason() {
    let mut solver = test_solver(4);
    let lits = [pos_lit(1), pos_lit(2), neg_lit(3), neg_lit(4)];
    let clause = unsafe {
        &mut *new_shared_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static))
    };

    assert_eq!(solver.num_clause_watches(!lits[0]), 1);
    assert_eq!(solver.num_clause_watches(!lits[1]), 1);

    solver.assume(!lits[0]);
    assert!(solver.propagate());
    solver.assume(!lits[3]);
    assert!(solver.propagate());
    solver.assume(!lits[1]);
    assert!(solver.propagate());

    assert!(solver.is_true(lits[2]));
    assert!(clause.locked(&solver));
    let mut antecedent = *solver.reason(lits[2].var());
    let mut reason = LitVec::new();
    antecedent.reason(&mut solver, lits[2], &mut reason);
    assert!(contains(reason.as_slice(), &!lits[0]));
    assert!(contains(reason.as_slice(), &!lits[1]));
    assert!(contains(reason.as_slice(), &!lits[3]));

    clause.destroy(Some(&mut solver), true);
}

#[test]
fn shared_clause_simplify_unique_removes_false_literals_without_copying_watch_state() {
    let mut solver = test_solver(6);
    let lits = make_lits(3, 3);
    let clause = unsafe {
        &mut *new_shared_clause(&mut solver, &lits, ClauseInfo::new(ConstraintType::Static))
    };

    solver.force(!lits[2], Antecedent::new());
    solver.force(!lits[3], Antecedent::new());
    assert!(solver.propagate());

    assert!(!clause.simplify(&mut solver, false));
    assert_eq!(clause.size(), 4);
    assert_eq!(solver.num_clause_watches(!lits[0]), 1);
    assert_eq!(solver.num_clause_watches(!lits[1]), 1);

    clause.destroy(Some(&mut solver), true);
}

#[test]
fn loop_formula_initializes_watches_like_upstream_subset() {
    let (mut ctx, loop_formula, lits) = create_loop_formula_fixture();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        assert!(solver.has_watch(lits[0], loop_formula.constraint));
        assert!(solver.has_watch(lits[1], loop_formula.constraint));
        assert!(solver.has_watch(lits[2], loop_formula.constraint));
        assert!(solver.has_watch(!lits[5], loop_formula.constraint));
        Constraint::destroy_raw(loop_formula.constraint, Some(solver), true);
    }
}

#[test]
fn loop_formula_propagates_body_reason_from_active_atom_and_bodies() {
    let (mut ctx, loop_formula, lits) = create_loop_formula_fixture();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        solver.undo_until(0);
        solver.assume(!lits[3]);
        assert!(solver.propagate());
        solver.assume(!lits[5]);
        assert!(solver.propagate());
        solver.assume(lits[2]);
        assert!(solver.propagate());

        assert!(solver.is_true(lits[4]));
        let mut antecedent = *solver.reason(lits[4].var());
        let mut reason = LitVec::new();
        antecedent.reason(solver, lits[4], &mut reason);
        assert_eq!(reason.len(), 3);
        assert!(contains(reason.as_slice(), &lits[2]));
        assert!(contains(reason.as_slice(), &!lits[5]));
        assert!(contains(reason.as_slice(), &!lits[3]));
        assert!((*loop_formula.constraint).locked(solver));
        Constraint::destroy_raw(loop_formula.constraint, Some(solver), true);
    }
}

#[test]
fn loop_formula_propagates_all_atoms_when_all_bodies_become_false() {
    let (mut ctx, loop_formula, lits) = create_loop_formula_fixture();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        solver.undo_until(0);
        solver.assume(!lits[5]);
        assert!(solver.propagate());
        solver.assume(!lits[3]);
        assert!(solver.propagate());
        solver.assume(!lits[4]);
        assert!(solver.propagate());

        assert!(solver.is_true(!lits[0]));
        assert!(solver.is_true(!lits[1]));
        assert!(solver.is_true(!lits[2]));
        let mut antecedent = *solver.reason(lits[1].var());
        let mut reason = LitVec::new();
        antecedent.reason(solver, !lits[1], &mut reason);
        assert_eq!(reason.len(), 3);
        assert!(contains(reason.as_slice(), &!lits[3]));
        assert!(contains(reason.as_slice(), &!lits[4]));
        assert!(contains(reason.as_slice(), &!lits[5]));
        Constraint::destroy_raw(loop_formula.constraint, Some(solver), true);
    }
}

#[test]
fn loop_formula_simplify_false_bodies_collapse_into_learnt_short_clauses() {
    let (mut ctx, loop_formula, lits) = create_loop_formula_fixture();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        solver.undo_until(0);
        solver.force(!lits[3], Antecedent::new());
        assert!(solver.propagate());
        assert!((*loop_formula.constraint).simplify(solver, false));
        assert_eq!(ctx.num_learnt_short(), 3);
        Constraint::destroy_raw(loop_formula.constraint, Some(solver), true);
    }
}

#[test]
fn loop_formula_simplify_true_atom_collapses_to_single_learnt_short_clause() {
    let (mut ctx, loop_formula, lits) = create_loop_formula_fixture();
    let solver_ptr = ctx.master() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        solver.undo_until(0);
        solver.force(lits[0], Antecedent::new());
        assert!(solver.propagate());
        assert!((*loop_formula.constraint).simplify(solver, false));
        assert_eq!(ctx.num_learnt_short(), 1);
        Constraint::destroy_raw(loop_formula.constraint, Some(solver), true);
    }
}
