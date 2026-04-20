use std::cell::Cell;
use std::rc::Rc;

use rust_clasp::clasp::constraint::{
    Constraint, ConstraintDyn, ConstraintInfo, ConstraintScore, ConstraintType, PostPropagator,
    PostPropagatorDyn, PropResult, PropagatorList, Solver,
};
use rust_clasp::clasp::literal::{LitVec, Literal, pos_lit};

struct DummyConstraint;

impl ConstraintDyn for DummyConstraint {
    fn propagate(&mut self, _s: &mut Solver, _p: Literal, _data: &mut u32) -> PropResult {
        PropResult::new(true, true)
    }

    fn reason(&mut self, _s: &mut Solver, _p: Literal, lits: &mut LitVec) {
        lits.push_back(pos_lit(1));
        lits.push_back(pos_lit(2));
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
    }
}

struct FixedPropagator {
    priority: u32,
    simplify: bool,
    drops: Rc<Cell<u32>>,
}

impl Drop for FixedPropagator {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl PostPropagatorDyn for FixedPropagator {
    fn priority(&self) -> u32 {
        self.priority
    }

    fn propagate_fixpoint(
        &mut self,
        _s: &mut Solver,
        _ctx: Option<std::ptr::NonNull<PostPropagator>>,
    ) -> bool {
        true
    }

    fn simplify(&mut self, _s: &mut Solver, _reinit: bool) -> bool {
        self.simplify
    }
}

fn collect_priorities(list: &PropagatorList) -> Vec<u32> {
    let mut current = list.head();
    let mut result = Vec::new();
    while let Some(ptr) = current {
        unsafe {
            result.push(ptr.as_ref().priority());
            current = ptr.as_ref().next;
        }
    }
    result
}

#[test]
fn constraint_defaults_match_constraint_cpp() {
    let mut solver = Solver::default();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);
    let mut constraint = Constraint::new(DummyConstraint);
    let mut reason = LitVec::default();

    assert!(constraint.valid(&mut solver));
    assert!(!constraint.simplify(&mut solver, false));
    assert_eq!(constraint.estimate_complexity(&solver), 1);
    assert_eq!(constraint.constraint_type(), ConstraintType::Static);
    assert!(constraint.locked(&solver));
    assert_eq!(constraint.activity(), ConstraintScore::default());
    assert_eq!(
        constraint.is_open(&solver, &Default::default(), &mut reason),
        0
    );
    solver.mark_seen_var(1);
    assert!(!constraint.minimize(&mut solver, pos_lit(1), std::ptr::null_mut()));
    solver.mark_seen_var(2);
    assert!(constraint.minimize(&mut solver, pos_lit(1), std::ptr::null_mut()));
}

#[test]
fn constraint_score_tracks_activity_and_lbd_bits() {
    let mut score = ConstraintScore::new(u32::MAX, u32::MAX);
    assert_eq!(score.activity(), (1 << 20) - 1);
    assert_eq!(score.lbd(), 127);

    score.reset(6, 5);
    score.bump_activity();
    score.bump_lbd(4);
    assert_eq!(score.activity(), 7);
    assert_eq!(score.lbd(), 4);
    assert!(score.bumped());

    score.reduce();
    assert_eq!(score.activity(), 3);
    assert!(!score.bumped());
}

#[test]
fn constraint_info_preserves_type_and_flags() {
    let mut info = ConstraintInfo::new(ConstraintType::Conflict);
    assert_eq!(info.constraint_type(), ConstraintType::Conflict);
    assert!(info.learnt());
    assert!(!info.tagged());
    assert!(!info.aux());

    info.set_activity(9)
        .set_lbd(3)
        .set_aux(true)
        .set_tagged(true)
        .set_type(ConstraintType::Loop);

    assert_eq!(info.constraint_type(), ConstraintType::Loop);
    assert_eq!(info.activity(), 9);
    assert_eq!(info.lbd(), 3);
    assert!(info.tagged());
    assert!(info.aux());
}

#[test]
fn propagator_list_orders_removes_and_clears() {
    let drops = Rc::new(Cell::new(0));
    let mut list = PropagatorList::new();
    list.add(Box::new(PostPropagator::new(FixedPropagator {
        priority: 10,
        simplify: false,
        drops: Rc::clone(&drops),
    })));
    list.add(Box::new(PostPropagator::new(FixedPropagator {
        priority: 0,
        simplify: false,
        drops: Rc::clone(&drops),
    })));
    list.add(Box::new(PostPropagator::new(FixedPropagator {
        priority: 5,
        simplify: true,
        drops: Rc::clone(&drops),
    })));

    assert_eq!(collect_priorities(&list), vec![0, 5, 10]);
    assert!(list.find(5).is_some());

    let mut solver = Solver::default();
    assert!(!list.simplify(&mut solver, false));
    assert_eq!(collect_priorities(&list), vec![0, 10]);
    assert_eq!(drops.get(), 1);

    let head = list.head().expect("expected head propagator");
    let removed = list.remove(head).expect("expected removed propagator");
    assert_eq!(removed.priority(), 0);
    drop(removed);
    assert_eq!(collect_priorities(&list), vec![10]);

    list.clear();
    assert!(list.head().is_none());
    assert_eq!(drops.get(), 3);
}
