use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::clause::ClauseRep;
use rust_clasp::clasp::constraint::{Antecedent, Constraint, ConstraintDyn, PropResult, Solver};
use rust_clasp::clasp::literal::{
    LitVec, ValueVec, neg_lit, pos_lit, value_false, value_free, value_true,
};
use rust_clasp::clasp::shared_context::SharedContext;
use rust_clasp::clasp::solver_types::{
    Assignment, ClauseWatch, GenericWatch, ReasonStore64, ReasonStore64Value, ValueSet, WatchList,
    release_vec,
};
use rust_clasp::clasp::util::indexed_priority_queue::IndexedPriorityQueue;
use rust_clasp::clasp::{clause::ClauseInfo, constraint::ClauseHead, constraint::ConstraintType};

struct DummyConstraint;

impl ConstraintDyn for DummyConstraint {
    fn propagate(
        &mut self,
        _solver: &mut Solver,
        _literal: rust_clasp::clasp::literal::Literal,
        _data: &mut u32,
    ) -> PropResult {
        PropResult::default()
    }

    fn reason(
        &mut self,
        _solver: &mut Solver,
        _literal: rust_clasp::clasp::literal::Literal,
        _lits: &mut LitVec,
    ) {
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
    }
}

struct OwnedConstraint {
    ptr: *mut Constraint,
}

impl OwnedConstraint {
    fn new() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(Constraint::new(DummyConstraint))),
        }
    }

    fn ptr(&self) -> *mut Constraint {
        self.ptr
    }
}

impl Drop for OwnedConstraint {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.ptr));
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WatchTrace {
    propagated: Vec<(i32, u32)>,
    undo_calls: u32,
}

struct TrackingConstraint {
    trace: Rc<RefCell<WatchTrace>>,
    keep_watch: bool,
}

impl ConstraintDyn for TrackingConstraint {
    fn propagate(
        &mut self,
        _solver: &mut Solver,
        literal: rust_clasp::clasp::literal::Literal,
        data: &mut u32,
    ) -> PropResult {
        self.trace
            .borrow_mut()
            .propagated
            .push((rust_clasp::clasp::literal::to_int(literal), *data));
        *data += 1;
        PropResult::new(true, self.keep_watch)
    }

    fn reason(
        &mut self,
        _solver: &mut Solver,
        _literal: rust_clasp::clasp::literal::Literal,
        _lits: &mut LitVec,
    ) {
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
    }

    fn undo_level(&mut self, _solver: &mut Solver) {
        self.trace.borrow_mut().undo_calls += 1;
    }
}

fn right_watch_data(list: &WatchList) -> Vec<u32> {
    list.right_view().map(|watch| watch.data).collect()
}

#[test]
fn indexed_prio_queue_matches_upstream_heap_order_and_updates() {
    let priorities = Rc::new(RefCell::new((0_i32..20).collect::<Vec<_>>()));
    let mut queue = IndexedPriorityQueue::new({
        let priorities = Rc::clone(&priorities);
        move |lhs: u32, rhs: u32| {
            let priorities = priorities.borrow();
            priorities[lhs as usize] < priorities[rhs as usize]
        }
    });

    assert!(queue.empty());
    assert_eq!(queue.size(), 0);

    for (index, value) in (0_u32..20).rev().enumerate() {
        queue.push(value);
        assert!(!queue.empty());
        assert_eq!(queue.size(), index + 1);
        assert_eq!(queue.top(), value);
        assert!(queue.contains(value));
        assert_eq!(queue.index(value), 0);
    }

    assert_eq!(queue.top(), 0);
    let expected_positions = [
        0, 1, 4, 3, 8, 18, 2, 6, 14, 5, 7, 9, 10, 16, 12, 13, 17, 19, 11, 15,
    ];
    for value in 0_u32..20 {
        assert_eq!(queue.index(value), expected_positions[value as usize]);
    }

    priorities.borrow_mut()[0] = 12;
    queue.decrease(0);
    assert_eq!(queue.size(), 20);
    assert_eq!(queue.top(), 1);
    assert_eq!(queue.index(0), 9);

    priorities.borrow_mut()[0] = 0;
    assert_eq!(queue.size(), 20);
    queue.increase(0);
    assert_eq!(queue.top(), 0);
    assert_eq!(queue.index(0), 0);

    queue.pop();
    assert_eq!(queue.size(), 19);
    assert_eq!(queue.top(), 1);
    assert!(!queue.contains(0));

    priorities.borrow_mut()[1] = 21;
    queue.update(1);
    assert_eq!(queue.top(), 2);

    priorities.borrow_mut()[7] = 22;
    queue.update(7);
    priorities.borrow_mut()[3] = 24;
    queue.update(3);
    priorities.borrow_mut()[5] = 28;
    queue.update(5);
    assert_eq!(queue.index(1), 18);
    assert_eq!(queue.index(7), 14);
    assert_eq!(queue.index(3), 17);
    assert_eq!(queue.index(5), 16);

    let mut queue_copy = queue.clone();
    let queue_copy_2 = queue.clone();
    for value in [
        2_u32, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 1, 7, 3, 5,
    ] {
        assert!(!queue.empty());
        assert!(!queue_copy.empty());
        assert_eq!(queue.top(), value);
        assert_eq!(queue_copy.top(), value);
        queue.pop();
        queue_copy.pop();
    }

    assert!(queue.empty());
    assert!(queue_copy.empty());
    assert!(!queue_copy_2.empty());
}

#[test]
fn watch_list_matches_upstream_solver_test_section() {
    let mut watch_list = WatchList::default();
    let dummy1 = 0x01usize as *mut rust_clasp::clasp::constraint::ClauseHead;
    let dummy2 = 0x02usize as *mut rust_clasp::clasp::constraint::ClauseHead;

    assert_eq!(WatchList::INLINE_RAW_CAP, 0);
    assert!(watch_list.empty());
    assert!(watch_list.left_view().is_empty());
    assert_eq!(watch_list.right_view().len(), 0);

    watch_list.push_left(ClauseWatch::new(dummy1));
    assert!(!watch_list.empty());
    assert_eq!(watch_list.left_size(), 1);
    assert_eq!(watch_list.right_size(), 0);
    assert_eq!(watch_list.left_view().len(), 1);
    assert_eq!(watch_list.right_view().len(), 0);
    assert_eq!(watch_list.left(0).head, dummy1);

    watch_list.push_right(GenericWatch::new(std::ptr::null_mut(), 0));
    assert_eq!(watch_list.right_size(), 1);
    assert_eq!(right_watch_data(&watch_list), vec![0]);

    watch_list.push_right(GenericWatch::new(std::ptr::null_mut(), 1));
    assert_eq!(watch_list.right_size(), 2);
    assert_eq!(right_watch_data(&watch_list), vec![0, 1]);
    assert_eq!(watch_list.left_size(), 1);

    watch_list.push_left(ClauseWatch::new(dummy2));
    assert_eq!(watch_list.left_size(), 2);
    assert_eq!(watch_list.left(1).head, dummy2);
    watch_list.push_right(GenericWatch::new(std::ptr::null_mut(), 3));
    watch_list.push_right(GenericWatch::new(std::ptr::null_mut(), 4));
    watch_list.push_right(GenericWatch::new(std::ptr::null_mut(), 5));
    assert_eq!(watch_list.right_size(), 5);
    assert_eq!(right_watch_data(&watch_list), vec![0, 1, 3, 4, 5]);
    assert_eq!(watch_list.right(3).data, 4);

    let mut copy = watch_list.clone();
    watch_list.pop_left();
    assert_eq!(watch_list.left_size(), 1);
    assert_eq!(watch_list.left(0).head, dummy1);
    assert_eq!(copy.left_size(), 2);

    let mut moved = std::mem::take(&mut copy);
    assert!(copy.empty());
    assert_eq!(moved.left_size(), 2);

    moved.erase_left_unordered(0);
    assert_eq!(moved.left_size(), 1);
    assert_eq!(moved.left(0).head, dummy2);

    release_vec(&mut moved);
    assert!(moved.empty());
    assert_eq!(moved.left_capacity(), 0);
    assert_eq!(moved.right_capacity(), 0);
}

#[cfg(target_pointer_width = "32")]
#[test]
fn reason_store32_matches_upstream_solver_test_section() {
    use rust_clasp::clasp::solver_types::{ReasonStore32, ReasonStore32Value};

    let mut store = ReasonStore32::default();
    store.resize(1);
    let constraint = OwnedConstraint::new();
    let antecedent = Antecedent::from_constraint_ptr(constraint.ptr());

    store[0] = antecedent;
    store.set_data(0, 22);
    assert_eq!(store[0], antecedent);
    assert_eq!(store.data(0), 22);

    let p = pos_lit(10);
    let q = pos_lit(22);
    store[0] = Antecedent::from_literals(p, q);
    let old = store.data(0);
    store.set_data(0, 74);
    assert_eq!(store.data(0), 74);
    store.set_data(0, old);
    assert_eq!(store[0].first_literal(), p);
    assert_eq!(store[0].second_literal(), q);

    let reason = ReasonStore32Value::new(antecedent, 169);
    store[0] = reason.ante();
    if reason.data() != u32::MAX {
        store.set_data(0, reason.data());
    }
    assert_eq!(store[0], antecedent);
    assert_eq!(store.data(0), 169);

    let literal_reason = ReasonStore32Value::new(Antecedent::from_literal(p), u32::MAX);
    store[0] = literal_reason.ante();
    if literal_reason.data() != u32::MAX {
        store.set_data(0, literal_reason.data());
    }
    assert_eq!(store[0].first_literal(), p);
}

#[test]
fn reason_store64_matches_upstream_solver_test_section() {
    let mut store = ReasonStore64::default();
    store.resize(1);
    let constraint = OwnedConstraint::new();
    let antecedent = Antecedent::from_constraint_ptr(constraint.ptr());

    store[0] = antecedent;
    store.set_data(0, 22);
    assert_eq!(store[0], antecedent);
    assert_eq!(store.data(0), 22);

    let p = pos_lit(10);
    let q = pos_lit(22);
    store[0] = Antecedent::from_literals(p, q);
    let old = store.data(0);
    store.set_data(0, 74);
    assert_eq!(store.data(0), 74);
    store.set_data(0, old);
    assert_eq!(store[0].first_literal(), p);
    assert_eq!(store[0].second_literal(), q);

    let reason = ReasonStore64Value::new(antecedent, 169);
    store[0] = reason.ante();
    if reason.data() != u32::MAX {
        store.set_data(0, reason.data());
    }
    assert_eq!(store[0], antecedent);
    assert_eq!(store.data(0), 169);

    let literal_reason = ReasonStore64Value::new(Antecedent::from_literal(p), u32::MAX);
    store[0] = literal_reason.ante();
    if literal_reason.data() != u32::MAX {
        store.set_data(0, literal_reason.data());
    }
    assert_eq!(store[0].first_literal(), p);
}

#[test]
fn value_set_matches_upstream_solver_test_section() {
    let mut values = ValueSet::default();
    assert!(values.empty());

    values.set(ValueSet::pref_value, value_true);
    assert!(!values.empty());
    assert!(values.has(ValueSet::pref_value));
    assert!(!values.sign());

    values.set(ValueSet::saved_value, value_false);
    assert!(values.has(ValueSet::saved_value));
    assert!(values.sign());

    values.set(ValueSet::user_value, value_true);
    assert!(values.has(ValueSet::user_value));
    assert!(!values.sign());

    values.set(ValueSet::user_value, value_free);
    assert!(!values.has(ValueSet::user_value));
    assert!(values.sign());
}

#[test]
fn assignment_helpers_cover_assign_undo_seen_and_unit_behavior() {
    let mut assignment = Assignment::default();
    let sentinel = assignment.add_var();
    let v1 = assignment.add_var();
    let v2 = assignment.add_var();
    assert_eq!(sentinel, 0);

    let p = pos_lit(v1);
    let q = neg_lit(v2);
    assert!(assignment.assign(p, 0, Antecedent::new()));
    assert!(assignment.assign_with_data(q, 2, Antecedent::from_literal(p), 74));
    assert_eq!(assignment.num_vars(), 3);
    assert_eq!(assignment.assigned(), 2);
    assert_eq!(assignment.free(), 1);
    assert_eq!(assignment.value(v1), value_true);
    assert_eq!(assignment.value(v2), value_false);
    assert_eq!(assignment.level(v2), 2);
    assert_eq!(assignment.reason(v2).first_literal(), p);
    assert_eq!(assignment.data(v2), 74);
    assert_eq!(assignment.q_size(), 2);
    assert_eq!(assignment.q_pop(), p);
    assert_eq!(assignment.q_size(), 1);

    let mut values = ValueVec::default();
    assignment.values(&mut values);
    assert_eq!(values.as_slice(), &[value_free, value_true, value_false]);

    assert!(assignment.mark_units());
    assert_eq!(assignment.units(), 1);
    assert!(assignment.seen_var(v1));
    assert!(assignment.seen_literal(p));
    assert!(assignment.seen_literal(!p));

    assignment.set_seen_literal(q);
    assert!(assignment.seen_literal(q));
    assignment.clear_seen(v2);
    assert!(!assignment.seen_var(v2));

    assert!(!assignment.assign(!q, 3, Antecedent::new()));

    assignment.request_prefs();
    assignment.undo_trail(1, true);
    assert_eq!(assignment.assigned(), 1);
    assert_eq!(assignment.value(v2), value_free);
    assert_eq!(assignment.pref(v2).get(ValueSet::saved_value), value_false);
    assert!(assignment.q_empty());

    assignment.set_pref(v1, ValueSet::user_value, value_true);
    assert_eq!(assignment.pref(v1).get(ValueSet::user_value), value_true);
    assignment.reset_prefs();
    assert_eq!(assignment.pref(v1).get(ValueSet::user_value), value_free);
    assert_eq!(assignment.pref(v2).get(ValueSet::saved_value), value_free);

    let mut tail = Assignment::default();
    tail.add_var();
    let v = tail.add_var();
    assert!(tail.assign(pos_lit(v), 1, Antecedent::new()));
    tail.undo_last();
    assert_eq!(tail.assigned(), 0);
    assert_eq!(tail.value(v), value_free);
}

#[test]
fn shared_context_defaults_match_bundle_a_kernel_surface() {
    let mut ctx = SharedContext::default();

    assert_eq!(ctx.num_vars(), 0);
    assert_eq!(ctx.num_constraints(), 0);
    assert_eq!(ctx.num_binary(), 0);
    assert_eq!(ctx.num_ternary(), 0);
    assert!(!ctx.frozen());
    assert!(!ctx.physical_share_problem());
    assert!(!ctx.physical_share(ConstraintType::Static));
    assert!(ctx.master().shared_context().is_some());

    let a = ctx.add_var();
    let b = ctx.add_var();
    assert!(ctx.valid_var(a));
    assert!(ctx.valid_var(b));
    ctx.set_frozen(a, true);
    assert_eq!(ctx.stats().vars.num, 2);
    assert_eq!(ctx.stats().vars.frozen, 1);

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());
    assert!(ctx.frozen());
}

#[test]
fn shared_context_tracks_implicit_short_clauses_and_solver_propagates_them() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let solver = ctx.start_add_constraints();
    let binary = ClauseRep::prepared(
        &[neg_lit(a), pos_lit(b)],
        ClauseInfo::new(ConstraintType::Static),
    );
    let ternary = ClauseRep::prepared(
        &[neg_lit(a), neg_lit(b), pos_lit(c)],
        ClauseInfo::new(ConstraintType::Static),
    );
    assert!(solver.add(&binary, true));
    assert!(solver.add(&ternary, true));
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 1);
    assert_eq!(ctx.num_constraints(), 2);
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(pos_lit(a)));
    assert!(solver.propagate());
    assert!(solver.is_true(pos_lit(b)));
    assert_eq!(solver.reason(b).first_literal(), pos_lit(a));

    assert!(solver.assume(pos_lit(b)));
    assert!(solver.propagate());
    assert!(solver.is_true(pos_lit(c)));
    let reason = *solver.reason(c);
    assert_eq!(reason.first_literal(), pos_lit(a));
    assert_eq!(reason.second_literal(), pos_lit(b));
}

#[test]
fn solver_watch_management_and_backtrack_follow_upstream_kernel_rules() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    let trace = Rc::new(RefCell::new(WatchTrace::default()));
    let tracked = Box::into_raw(Box::new(Constraint::new(TrackingConstraint {
        trace: Rc::clone(&trace),
        keep_watch: true,
    })));
    let clause_head = 0x42usize as *mut ClauseHead;

    solver.add_watch(pos_lit(1), tracked, 7);
    solver.add_clause_watch(pos_lit(2), clause_head);
    assert!(solver.has_watch(pos_lit(1), tracked));
    assert!(solver.has_clause_watch(pos_lit(2), clause_head));
    assert_eq!(solver.num_watches(pos_lit(1)), 1);
    assert_eq!(solver.num_clause_watches(pos_lit(2)), 1);
    assert_eq!(solver.get_watch(pos_lit(1), 0).unwrap().data, 7);
    assert_eq!(
        solver.get_clause_watch(pos_lit(2), 0).unwrap(),
        ClauseWatch::new(clause_head)
    );

    assert!(solver.assume(pos_lit(1)));
    solver.add_undo_watch(1, tracked);
    assert_eq!(solver.queue_size(), 1);
    assert!(solver.propagate());
    assert!(solver.q_empty());
    assert_eq!(trace.borrow().propagated, vec![(1, 7)]);
    assert_eq!(solver.get_watch(pos_lit(1), 0).unwrap().data, 8);

    assert!(solver.backtrack(0));
    assert_eq!(solver.decision_level(), 0);
    assert_eq!(solver.num_assigned_vars(), 0);
    assert_eq!(trace.borrow().undo_calls, 1);

    assert!(solver.remove_watch(pos_lit(1), tracked));
    assert!(solver.remove_clause_watch(pos_lit(2), clause_head));
    assert!(!solver.has_watch(pos_lit(1), tracked));
    assert!(!solver.has_clause_watch(pos_lit(2), clause_head));

    unsafe {
        Constraint::destroy_raw(tracked, None, false);
    }
}

#[test]
fn solver_conflict_reason_and_clear_assumptions_cover_bundle_a_kernel_subset() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let solver = ctx.start_add_constraints();
    let imp = ClauseRep::prepared(
        &[neg_lit(a), pos_lit(b)],
        ClauseInfo::new(ConstraintType::Static),
    );
    assert!(solver.add(&imp, true));
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(pos_lit(a)));
    assert!(solver.propagate());
    assert!(solver.is_true(pos_lit(b)));
    assert!(!solver.force(neg_lit(b), Antecedent::new()));
    assert!(solver.has_conflict());
    assert_eq!(solver.conflict_literal(), neg_lit(b));

    assert!(solver.clear_assumptions());
    assert_eq!(solver.root_level(), 0);
    assert_eq!(solver.decision_level(), 0);
    assert_eq!(solver.num_assigned_vars(), 0);
    assert!(!solver.has_conflict());
}
