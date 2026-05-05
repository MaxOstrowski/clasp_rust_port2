use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;

use rust_clasp::clasp::asp_preprocessor::SatPreprocessor;
use rust_clasp::clasp::clause::{CLAUSE_WATCH_FIRST, CLAUSE_WATCH_LEAST, ClauseCreator, ClauseRep};
use rust_clasp::clasp::cli::clasp_cli_options::solver_strategies::SignHeu;
use rust_clasp::clasp::constraint::{
    Antecedent, Constraint, ConstraintDyn, PropResult, priority_reserved_look,
};
use rust_clasp::clasp::literal::{
    LitVec, Literal, ValueVec, WeightLiteral, lit_false, neg_lit, pos_lit, value_false, value_free,
    value_true,
};
use rust_clasp::clasp::pod_vector::contains;
use rust_clasp::clasp::shared_context::{SharedContext, VarInfo};
use rust_clasp::clasp::solver::{
    CCMinRecursive, DecisionHeuristic, PostPropagator, PostPropagatorDyn, SelectFirst, Solver,
    UndoMode, priority_class_general,
};
use rust_clasp::clasp::solver_strategies::{
    CCMinAntes, CCRepMode, Forget, HeuParams, ReduceStrategy, SearchLimits, SearchStrategy,
    SolverParams, UpdateMode, UserConfiguration,
};
use rust_clasp::clasp::solver_types::{
    Assignment, ClauseWatch, ExtendedStats, GenericWatch, ImpliedList, ImpliedLiteral,
    ReasonStore64, ReasonStore64Value, ValueSet, WatchList, release_vec,
};
use rust_clasp::clasp::util::indexed_priority_queue::IndexedPriorityQueue;
use rust_clasp::clasp::{
    clause::ClauseInfo, constraint::ClauseHead, constraint::ConstraintType, literal::VarType,
};

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

struct ConflictReasonConstraint {
    ante: Vec<Literal>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LearntTrace {
    decreases: u32,
    destroyed: u32,
}

struct LearntConstraint {
    trace: Rc<RefCell<LearntTrace>>,
    locked: bool,
    score: rust_clasp::clasp::constraint::ConstraintScore,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PostTrace {
    inits: u32,
    props: u32,
    resets: u32,
    undos: u32,
}

struct TrackingPostProp {
    trace: Rc<RefCell<PostTrace>>,
    priority: u32,
    fail: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct HeuristicInitTrace {
    start_inits: u32,
    end_inits: u32,
    detached: u32,
    config_params: Vec<u32>,
    updates: Vec<(u32, u32)>,
    simplified: Vec<Vec<i32>>,
    undone: Vec<Vec<i32>>,
    reasons: Vec<(Vec<i32>, i32)>,
}

struct InitTrackingHeuristic {
    trace: Rc<RefCell<HeuristicInitTrace>>,
    negative_sign: bool,
}

struct DefaultDecisionHeuristic;

impl DecisionHeuristic for InitTrackingHeuristic {
    fn start_init(&mut self, _solver: &mut Solver) {
        self.trace.borrow_mut().start_inits += 1;
    }

    fn end_init(&mut self, _solver: &mut Solver) {
        self.trace.borrow_mut().end_inits += 1;
    }

    fn detach(&mut self, _solver: &mut Solver) {
        self.trace.borrow_mut().detached += 1;
    }

    fn set_config(&mut self, params: HeuParams) {
        self.trace.borrow_mut().config_params.push(params.param);
    }

    fn update_var(&mut self, _solver: &mut Solver, var: u32, num: u32) {
        self.trace.borrow_mut().updates.push((var, num));
    }

    fn simplify(&mut self, _solver: &Solver, new_facts: &[Literal]) {
        self.trace.borrow_mut().simplified.push(
            new_facts
                .iter()
                .copied()
                .map(rust_clasp::clasp::literal::to_int)
                .collect(),
        );
    }

    fn undo(&mut self, _solver: &Solver, undo: &[Literal]) {
        self.trace.borrow_mut().undone.push(
            undo.iter()
                .copied()
                .map(rust_clasp::clasp::literal::to_int)
                .collect(),
        );
    }

    fn update_reason(&mut self, _solver: &Solver, lits: &[Literal], resolve_lit: Literal) {
        self.trace.borrow_mut().reasons.push((
            lits.iter()
                .copied()
                .map(rust_clasp::clasp::literal::to_int)
                .collect(),
            rust_clasp::clasp::literal::to_int(resolve_lit),
        ));
    }

    fn select_literal(&self, _solver: &Solver, var: u32, _idx: i32) -> Literal {
        Literal::new(var, self.negative_sign)
    }
}

impl DecisionHeuristic for DefaultDecisionHeuristic {}

impl PostPropagatorDyn for TrackingPostProp {
    fn priority(&self) -> u32 {
        self.priority
    }

    fn propagate_fixpoint(
        &mut self,
        _s: &mut Solver,
        _ctx: Option<NonNull<PostPropagator>>,
    ) -> bool {
        self.trace.borrow_mut().props += 1;
        !self.fail
    }

    fn init(&mut self, _s: &mut Solver) -> bool {
        self.trace.borrow_mut().inits += 1;
        true
    }

    fn reset(&mut self) {
        self.trace.borrow_mut().resets += 1;
    }

    fn undo_level(&mut self, _solver: &mut Solver) {
        self.trace.borrow_mut().undos += 1;
    }
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

impl ConstraintDyn for ConflictReasonConstraint {
    fn propagate(
        &mut self,
        _solver: &mut Solver,
        _literal: Literal,
        _data: &mut u32,
    ) -> PropResult {
        PropResult::default()
    }

    fn reason(&mut self, _solver: &mut Solver, _literal: Literal, lits: &mut LitVec) {
        for literal in self.ante.iter().copied() {
            lits.push_back(literal);
        }
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
    }
}

impl ConstraintDyn for LearntConstraint {
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

    fn locked(&self, _solver: &Solver) -> bool {
        self.locked
    }

    fn activity(&self) -> rust_clasp::clasp::constraint::ConstraintScore {
        self.score
    }

    fn decrease_activity(&mut self) {
        self.trace.borrow_mut().decreases += 1;
        self.score.reduce();
    }

    fn destroy(&mut self, _solver: Option<&mut Solver>, _detach: bool) {
        self.trace.borrow_mut().destroyed += 1;
    }
}

fn right_watch_data(list: &WatchList) -> Vec<u32> {
    list.right_view().map(|watch| watch.data).collect()
}

fn add_tracking_post(
    solver: &mut Solver,
    trace: Rc<RefCell<PostTrace>>,
    priority: u32,
    fail: bool,
) -> bool {
    solver.add_post(Box::new(PostPropagator::new(TrackingPostProp {
        trace,
        priority,
        fail,
    })))
}

fn learnt_constraint(
    trace: Rc<RefCell<LearntTrace>>,
    locked: bool,
    activity: u32,
    lbd: u32,
) -> *mut Constraint {
    Box::into_raw(Box::new(Constraint::new(LearntConstraint {
        trace,
        locked,
        score: rust_clasp::clasp::constraint::ConstraintScore::new(activity, lbd),
    })))
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
fn implied_list_begin_end_match_upstream_iteration_endpoints() {
    let p = pos_lit(1);
    let entry = ImpliedLiteral::new(p, 2, Antecedent::from_literal(!p));
    let mut implied = ImpliedList::default();
    implied.add(2, entry);

    let mut begin = implied.begin();
    assert_eq!(begin.next(), Some(&entry));
    assert_eq!(begin.next(), None);

    let mut end = implied.end();
    assert_eq!(end.next(), None);
}

#[test]
fn implied_list_find_returns_matching_entry_only() {
    let p = pos_lit(1);
    let q = neg_lit(2);
    let mut implied = ImpliedList::default();
    implied.add(2, ImpliedLiteral::new(p, 2, Antecedent::from_literal(!p)));
    implied.add(3, ImpliedLiteral::new(q, 3, Antecedent::from_literal(!q)));

    let found = implied.find(q).expect("expected matching implied literal");
    assert_eq!(found.lit, q);
    assert_eq!(found.level, 3);
    assert!(implied.find(pos_lit(3)).is_none());
}

#[test]
fn implied_list_active_matches_level_and_front_rules() {
    let p = pos_lit(1);
    let mut implied = ImpliedList::default();
    implied.add(3, ImpliedLiteral::new(p, 3, Antecedent::from_literal(!p)));

    assert!(implied.active(2));
    assert!(!implied.active(3));

    implied.front = implied.len() as u32;
    assert!(!implied.active(2));
}

#[test]
fn implied_list_assign_replays_earlier_implied_literal_on_backtrack() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let implied = OwnedConstraint::new();

    assert!(solver.assume(a));
    assert!(solver.assume(b));
    assert!(solver.assume(c));
    solver.set_backtrack_level(solver.decision_level());
    assert!(solver.force_at_level(!d, 2, Antecedent::from_constraint_ptr(implied.ptr())));
    assert_eq!(solver.level(d.var()), 3);

    solver.set_backtrack_level(0);
    assert!(solver.backtrack(2));
    assert!(solver.is_true(!d));
    assert_eq!(solver.level(d.var()), 2);
}

#[test]
fn implied_literal_copy_assignment_preserves_all_fields() {
    let p = pos_lit(1);
    let source = ImpliedLiteral::with_data(p, 4, Antecedent::from_literal(!p), 23);
    let mut target = ImpliedLiteral::new(!p, 1, Antecedent::new());
    assert_ne!(target, source);

    target = source;

    assert_eq!(target, source);
    assert_eq!(target.ante.data(), 23);
}

#[test]
fn shared_context_defaults_match_bundle_a_kernel_surface() {
    let mut ctx = SharedContext::default();

    assert!(ctx.ok());
    assert!(!ctx.valid_var(0));
    assert_eq!(ctx.num_vars(), 0);
    assert_eq!(ctx.vars().collect::<Vec<_>>(), Vec::<u32>::new());
    assert_eq!(ctx.num_eliminated_vars(), 0);
    assert_eq!(ctx.num_constraints(), 0);
    assert_eq!(ctx.num_binary(), 0);
    assert_eq!(ctx.num_ternary(), 0);
    assert!(!ctx.frozen());
    assert!(!ctx.is_extended());
    assert!(!ctx.physical_share_problem());
    assert!(!ctx.physical_share(ConstraintType::Static));
    assert_eq!(ctx.short_implications().num_binary(), 0);
    assert_eq!(ctx.short_implications().num_ternary(), 0);
    assert!(ctx.master().shared_context().is_some());

    let a = ctx.add_var();
    let b = ctx.add_var();
    assert!(ctx.valid_var(a));
    assert!(ctx.valid_var(b));
    assert!(!ctx.valid_var(0));
    assert_eq!(ctx.vars().collect::<Vec<_>>(), vec![a, b]);
    ctx.set_frozen(a, true);
    assert_eq!(ctx.stats().vars.num, 2);
    assert_eq!(ctx.stats().vars.frozen, 1);
    assert!(ctx.is_extended());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());
    assert!(ctx.frozen());
}

#[test]
fn shared_context_valid_var_keeps_zero_as_sentinel_even_with_step_var_support() {
    let mut ctx = SharedContext::default();

    assert!(!ctx.valid_var(0));
    assert_eq!(ctx.step_literal().var(), 0);

    let step = ctx.require_step_var();
    assert_eq!(step.var(), 1);
    assert!(ctx.valid_var(step.var()));
    assert!(!ctx.valid_var(0));
}

#[test]
fn shared_context_start_add_constraints_with_guess_preserves_bundle_a_behavior() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();

    let solver_ptr = ctx.start_add_constraints_with_guess(1) as *mut Solver;
    assert_eq!(solver_ptr, ctx.master() as *mut Solver);

    let clause = ClauseCreator::new(Some(unsafe { &mut *solver_ptr }))
        .start(ConstraintType::Static)
        .add(pos_lit(a))
        .add(pos_lit(b))
        .end_with_defaults();

    assert!(clause.ok());
    assert_eq!(ctx.num_constraints(), 1);
    assert_eq!(ctx.num_binary(), 1);
}

#[test]
fn shared_context_parity_aliases_use_existing_bundle_a_state_only() {
    let mut info = VarInfo::new(VarInfo::FLAG_INPUT | VarInfo::FLAG_OUTPUT | VarInfo::FLAG_BODY);
    assert_eq!(info.r#type(), info.type_());
    assert!(info.has_all(VarInfo::FLAG_INPUT | VarInfo::FLAG_OUTPUT));
    assert!(info.has_any(VarInfo::FLAG_INPUT | VarInfo::FLAG_FROZEN));
    assert!(!info.has_all(VarInfo::FLAG_INPUT | VarInfo::FLAG_FROZEN));

    info.set(VarInfo::FLAG_FROZEN);
    assert!(info.has_all(VarInfo::FLAG_INPUT | VarInfo::FLAG_FROZEN));

    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    assert!(
        ctx.start_add_constraints()
            .force(pos_lit(a), Antecedent::new())
    );
    assert!(
        !ctx.start_add_constraints()
            .force(neg_lit(a), Antecedent::new())
    );
    assert!(!ctx.ok());
}

#[test]
fn shared_context_eliminate_var_matches_upstream_solver_test_section() {
    let mut ctx = SharedContext::default();
    let v1 = ctx.add_typed_var(VarType::Atom, VarInfo::FLAG_NANT | VarInfo::FLAG_INPUT);
    let _ = ctx.add_typed_var(VarType::Body, VarInfo::FLAG_NANT | VarInfo::FLAG_INPUT);

    {
        let solver = ctx.start_add_constraints();
        assert_eq!(solver.num_vars(), 2);
    }
    ctx.eliminate(v1);

    assert_eq!(ctx.num_eliminated_vars(), 1);
    assert!(ctx.eliminated(v1));
    {
        let solver = ctx.master_ref();
        assert_eq!(solver.num_vars(), 2);
        assert_eq!(solver.num_free_vars(), 1);
        assert_eq!(solver.num_assigned_vars(), 0);
        assert_ne!(solver.value(v1), value_free);
    }

    ctx.eliminate(v1);
    assert_eq!(ctx.num_eliminated_vars(), 1);
    assert!(ctx.end_init());
}

#[test]
fn shared_context_mark_ignores_add_flags_and_tracks_literal_marks() {
    let mut ctx = SharedContext::default();
    let v1 = ctx.add_typed_var(
        VarType::Atom,
        VarInfo::FLAG_NANT | VarInfo::FLAG_INPUT | VarInfo::FLAG_POS,
    );
    let v2 = ctx.add_typed_var(VarType::Body, VarInfo::FLAG_NEG);
    let v3 = ctx.add_typed_var(VarType::Hybrid, 0);

    assert_eq!(ctx.var_info(v1).type_(), VarType::Atom);
    assert_eq!(ctx.var_info(v2).type_(), VarType::Body);
    assert_eq!(ctx.var_info(v3).type_(), VarType::Hybrid);
    assert!(!ctx.marked(pos_lit(v1)) && !ctx.marked(neg_lit(v1)));
    assert!(!ctx.marked(pos_lit(v2)) && !ctx.marked(neg_lit(v2)));
    assert!(!ctx.marked(pos_lit(v3)) && !ctx.marked(neg_lit(v3)));

    ctx.mark(pos_lit(v1));
    ctx.mark(neg_lit(v3));
    ctx.mark(pos_lit(v2));

    assert!(ctx.marked(pos_lit(v1)));
    assert!(!ctx.marked(neg_lit(v1)));
    assert!(ctx.marked(pos_lit(v2)));
    assert!(!ctx.marked(neg_lit(v2)));
    assert!(ctx.marked(neg_lit(v3)));
    assert!(!ctx.marked(pos_lit(v3)));

    ctx.unmark_literal(pos_lit(v1));
    ctx.unmark_var(v3);

    assert!(!ctx.marked(pos_lit(v1)));
    assert!(ctx.marked(pos_lit(v2)));
    assert!(!ctx.marked(neg_lit(v3)));
}

#[test]
fn solver_bundle_a_header_accessors_match_upstream_surface() {
    let mut ctx = SharedContext::default();
    let atom = ctx.add_typed_var(VarType::Atom, VarInfo::FLAG_INPUT);
    let body = ctx.add_typed_var(VarType::Body, VarInfo::FLAG_OUTPUT);

    {
        let solver = ctx.start_add_constraints();
        solver.strategies_mut().search = SearchStrategy::NoLearning as u32;
        solver.strategies_mut().up_mode = UpdateMode::UpdateOnConflict as u32;
        solver.strategies_mut().compress = 7;
        solver.strategies_mut().restart_on_model = 1;
        let tag = solver.push_tag_var(false);

        assert_eq!(solver.search_mode(), SearchStrategy::NoLearning);
        assert_eq!(solver.update_mode(), UpdateMode::UpdateOnConflict);
        assert_eq!(solver.compress_limit(), 7);
        assert!(solver.restart_on_model());
        assert!(solver.is_master());
        assert_eq!(solver.var_info(atom).type_(), VarType::Atom);
        assert_eq!(solver.var_info(body).type_(), VarType::Body);
        assert_eq!(solver.var_info(body + 10), VarInfo::default());
        assert_eq!(solver.pref(atom), ValueSet::default());
        assert_eq!(solver.pref(tag).get(ValueSet::def_value), value_false);
    }

    let attached_id = {
        let attached = ctx.push_solver();
        attached.id()
    };
    assert!(
        !ctx.solver_ref(attached_id)
            .expect("attached solver")
            .is_master()
    );
}

#[test]
fn solver_start_init_reads_shared_configuration_and_sat_prepro() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    let _ = ctx.add_var();
    {
        let config = ctx.configuration_mut();
        let solver_params = config.add_solver(0);
        solver_params.search = SearchStrategy::NoLearning as u32;
        solver_params.up_mode = UpdateMode::UpdateOnConflict as u32;
        solver_params.cc_min_rec = 1;
        let search_params = config.add_search(0);
        search_params.rand_runs = 7;
        search_params.rand_prob = 0.25;
    }
    ctx.set_sat_prepro(Some(Box::new(SatPreprocessor::new())));

    let solver = ctx.start_add_constraints_with_guess(12);
    let search = solver.search_config().expect("search config");

    assert_eq!(solver.search_mode(), SearchStrategy::NoLearning);
    assert_eq!(solver.update_mode(), UpdateMode::UpdateOnConflict);
    assert_eq!(solver.strategies().cc_min_rec, 1);
    assert_eq!(search.rand_runs, 7);
    assert!((search.rand_prob - 0.25).abs() < f32::EPSILON);
    assert!(solver.sat_prepro().is_some());
}

#[test]
fn solver_init_calls_heuristic_hooks_and_applies_sign_fix_preferences() {
    let mut solver = Solver::new();
    solver.set_num_vars(3);
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: true,
    });

    let mut params = SolverParams {
        heu_id: solver.strategies().heu_id,
        sign_fix: 1,
        ..SolverParams::default()
    };
    params.heuristic.param = 17;

    solver.start_init(6, params);
    assert_eq!(trace.borrow().start_inits, 1);
    assert_eq!(trace.borrow().config_params, vec![17]);

    assert!(solver.end_init());
    assert_eq!(trace.borrow().end_inits, 1);
    for var in 1..=3 {
        assert_eq!(solver.pref(var).get(ValueSet::user_value), value_false);
    }
}

#[test]
fn decision_heuristic_default_construction_keeps_bump_as_a_noop() {
    let mut heuristic = DefaultDecisionHeuristic;
    let mut solver = Solver::new();
    solver.set_num_vars(1);

    let before = heuristic.select_literal(&solver, 1, 0);
    let weighted = [WeightLiteral {
        lit: pos_lit(1),
        weight: 3,
    }];

    assert!(!heuristic.bump(&solver, &weighted, 1.0));
    assert_eq!(heuristic.select_literal(&solver, 1, 0), before);
}

#[test]
fn decision_heuristic_select_literal_prefers_explicit_values_then_default_sign() {
    let heuristic = SelectFirst;

    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.strategies_mut().sign_def = SignHeu::SignNeg as u32;

    assert_eq!(heuristic.select_literal(&solver, 1, 0), neg_lit(1));
    assert_eq!(heuristic.select_literal(&solver, 1, 1), pos_lit(1));

    solver.assignment_mut().request_prefs();
    solver
        .assignment_mut()
        .set_pref(1, ValueSet::pref_value, value_true);
    solver
        .assignment_mut()
        .set_pref(2, ValueSet::user_value, value_false);

    assert_eq!(heuristic.select_literal(&solver, 1, -1), pos_lit(1));
    assert_eq!(heuristic.select_literal(&solver, 2, 1), neg_lit(2));

    let mut ctx = SharedContext::default();
    let atom = ctx.add_typed_var(VarType::Atom, VarInfo::FLAG_INPUT);
    let body = ctx.add_typed_var(VarType::Body, 0);
    let shared_solver = ctx.start_add_constraints();

    assert_eq!(
        heuristic.select_literal(shared_solver, atom, 0),
        neg_lit(atom)
    );
    assert_eq!(
        heuristic.select_literal(shared_solver, body, 0),
        pos_lit(body)
    );
}

#[test]
fn solver_end_init_short_circuits_when_conflict_is_already_present() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: false,
    });
    solver.set_has_conflict(true);

    assert!(!solver.end_init());
    assert_eq!(trace.borrow().end_inits, 0);
    assert_eq!(solver.pref(1).get(ValueSet::user_value), value_free);
}

#[test]
fn solver_reset_config_removes_reserved_look_post_and_detaches_previous_heuristic() {
    let mut solver = Solver::new();
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: false,
    });
    solver.set_default_heuristic();
    assert_eq!(trace.borrow().detached, 1);

    solver.strategies_mut().has_config = 1;
    let post_trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        &mut solver,
        Rc::clone(&post_trace),
        priority_reserved_look,
        false,
    ));
    assert!(solver.get_post(priority_reserved_look).is_some());

    solver.reset_config();

    assert_eq!(solver.strategies().has_config, 0);
    assert!(solver.get_post(priority_reserved_look).is_none());
}

#[test]
fn solver_reset_restores_fresh_solver_state_and_destroys_owned_runtime_objects() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    let solver = ctx.start_add_constraints();
    solver.set_id(7);
    solver.set_num_vars(3);
    solver.set_num_problem_vars(2);
    solver.strategies_mut().has_config = 1;
    solver.add_learnt_bytes(19);
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::new(RefCell::new(HeuristicInitTrace::default())),
        negative_sign: true,
    });

    let static_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let learnt_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let enum_trace = Rc::new(RefCell::new(LearntTrace::default()));
    solver.add_constraint(learnt_constraint(Rc::clone(&static_trace), false, 2, 3));
    solver.add_learnt_constraint(
        learnt_constraint(Rc::clone(&learnt_trace), false, 4, 5),
        2,
        ConstraintType::Conflict,
    );
    solver.set_enumeration_constraint(Some(learnt_constraint(Rc::clone(&enum_trace), false, 6, 7)));
    let post_trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&post_trace),
        priority_class_general,
        false,
    ));

    solver.reset();

    assert_eq!(static_trace.borrow().destroyed, 1);
    assert_eq!(learnt_trace.borrow().destroyed, 1);
    assert_eq!(enum_trace.borrow().destroyed, 1);
    assert_eq!(solver.id(), 7);
    assert!(solver.shared_context().is_some());
    assert_eq!(solver.num_vars(), 0);
    assert_eq!(solver.num_problem_vars(), 0);
    assert_eq!(solver.learnt_bytes(), 0);
    assert_eq!(solver.num_constraints(), 0);
    assert!(solver.enumeration_constraint().is_none());
    assert!(solver.get_post(priority_class_general).is_none());
    assert_eq!(solver.strategies().has_config, 0);
    assert_eq!(solver.pref(1).get(ValueSet::user_value), value_free);
}

#[test]
fn solver_prepare_post_initializes_existing_and_new_post_propagators() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    let solver = ctx.start_add_constraints();

    let first_trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&first_trace),
        priority_class_general,
        false,
    ));
    assert_eq!(first_trace.borrow().inits, 0);

    assert!(solver.prepare_post());
    assert_eq!(first_trace.borrow().inits, 1);

    let second_trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&second_trace),
        priority_class_general + 1,
        false,
    ));
    assert_eq!(second_trace.borrow().inits, 1);
}

#[test]
fn solver_add_falls_back_to_explicit_clause_creation_when_clause_is_not_implicit() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();
    let solver = ctx.start_add_constraints();

    let clause = ClauseRep::create(
        &[pos_lit(a), pos_lit(b), pos_lit(c), pos_lit(d)],
        ClauseInfo::new(ConstraintType::Static),
    );

    assert!(solver.add(&clause, true));
    assert_eq!(solver.num_constraints(), 1);
    assert_eq!(ctx.num_binary(), 0);
    assert_eq!(ctx.num_ternary(), 0);
}

#[test]
fn solver_set_enumeration_constraint_replaces_and_destroys_previous_constraint() {
    let mut solver = Solver::new();
    let first_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let second_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let first = learnt_constraint(Rc::clone(&first_trace), false, 2, 3);
    let second = learnt_constraint(Rc::clone(&second_trace), false, 4, 5);

    solver.set_enumeration_constraint(Some(first));
    assert_eq!(solver.enumeration_constraint(), Some(first));

    solver.set_enumeration_constraint(Some(second));
    assert_eq!(first_trace.borrow().destroyed, 1);
    assert_eq!(solver.enumeration_constraint(), Some(second));

    solver.set_enumeration_constraint(None);
    assert_eq!(second_trace.borrow().destroyed, 1);
    assert!(solver.enumeration_constraint().is_none());
}

#[test]
fn solver_pop_aux_var_removes_tail_auxiliary_variables_and_reports_heuristic_updates() {
    let mut solver = Solver::new();
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: false,
    });

    let aux = solver.push_aux_var();
    assert_eq!(aux, 1);
    assert_eq!(solver.num_vars(), 1);
    assert_eq!(trace.borrow().updates, vec![(1, 1)]);

    solver.pop_aux_var(1);

    assert_eq!(solver.num_vars(), 0);
    assert_eq!(trace.borrow().updates, vec![(1, 1), (1, 1)]);
}

#[test]
fn solver_update_vars_matches_shared_problem_var_growth_and_shrink() {
    let mut solver = Solver::new();
    solver.set_num_vars(1);
    solver.set_num_problem_vars(1);
    let first_aux = solver.push_aux_var();
    let second_aux = solver.push_aux_var();
    solver.set_tag_literal(pos_lit(second_aux));

    solver.update_vars(2);

    assert_eq!(first_aux, 2);
    assert_eq!(solver.num_vars(), 2);
    assert_eq!(solver.num_problem_vars(), 2);
    assert_eq!(solver.tag_literal(), Literal::default());
    assert!(!solver.valid_var(3));

    solver.update_vars(4);

    assert_eq!(solver.num_vars(), 4);
    assert_eq!(solver.num_problem_vars(), 4);
    assert!(solver.valid_var(4));
    assert!(solver.get_watch(pos_lit(4), 0).is_none());
}

#[test]
fn solver_problem_vars_iterates_only_problem_variable_range() {
    let mut solver = Solver::new();
    solver.set_num_problem_vars(4);
    solver.set_num_vars(6);

    assert_eq!(solver.problem_vars(1).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    assert_eq!(solver.problem_vars(3).collect::<Vec<_>>(), vec![3, 4]);
    assert!(solver.problem_vars(5).collect::<Vec<_>>().is_empty());
}

#[test]
fn solver_end_step_forgets_signs_and_heuristics_and_removes_reserved_look_post() {
    let mut ctx = SharedContext::default();
    let atom = ctx.add_var();
    let solver = ctx.start_add_constraints();
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: true,
    });
    solver.strategies_mut().has_config = 0;
    let init_params = SolverParams {
        heu_id: solver.strategies().heu_id,
        sign_fix: 1,
        ..SolverParams::default()
    };
    solver.start_init(1, init_params);
    assert!(solver.end_init());
    assert_eq!(solver.pref(atom).get(ValueSet::user_value), value_false);
    let post_trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&post_trace),
        priority_reserved_look,
        false,
    ));

    let params = SolverParams {
        forget_set: (Forget::ForgetHeuristic as u32) | (Forget::ForgetSigns as u32),
        ..SolverParams::default()
    };

    assert!(solver.end_step(0, params));
    assert_eq!(trace.borrow().detached, 1);
    assert_eq!(solver.pref(atom).get(ValueSet::user_value), value_free);
    assert!(solver.get_post(priority_reserved_look).is_none());
}

#[test]
fn solver_split_uses_guiding_path_and_clears_pending_split_request() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    assert!(solver.assume(pos_lit(1)));
    solver.push_root_level(1);
    assert!(solver.assume(pos_lit(2)));

    assert!(solver.request_split());
    let mut path = LitVec::new();
    assert!(solver.split(&mut path));
    assert_eq!(path.as_slice(), &[pos_lit(1), neg_lit(2)]);
    assert!(!solver.clear_split_request());
}

#[test]
fn solver_num_watches_counts_shared_implicit_edges_for_problem_literals() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let solver = ctx.start_add_constraints();
    let binary = ClauseRep::prepared(
        &[neg_lit(a), pos_lit(b)],
        ClauseInfo::new(ConstraintType::Static),
    );

    assert!(solver.add(&binary, true));
    assert_eq!(solver.num_watches(pos_lit(a)), 1);
    assert_eq!(solver.num_watches(neg_lit(a)), 0);
}

#[test]
fn solver_test_probe_undoes_successful_probe_levels_and_notifies_post_propagators() {
    let mut solver = Solver::new();
    solver.set_num_vars(1);
    let trace = Rc::new(RefCell::new(PostTrace::default()));
    assert!(add_tracking_post(
        &mut solver,
        Rc::clone(&trace),
        priority_class_general,
        false,
    ));
    let post = solver.get_post(priority_class_general);

    assert!(solver.test(pos_lit(1), post));
    assert_eq!(solver.decision_level(), 0);
    assert!(!solver.frozen_level(1));
    assert_eq!(solver.stats().core.choices, 0);
    assert_eq!(trace.borrow().undos, 1);
}

#[test]
fn solver_invokes_heuristic_simplify_and_undo_hooks() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);
    let trace = Rc::new(RefCell::new(HeuristicInitTrace::default()));
    solver.set_heuristic(InitTrackingHeuristic {
        trace: Rc::clone(&trace),
        negative_sign: false,
    });

    assert!(solver.force(pos_lit(1), Antecedent::new()));
    assert!(solver.simplify());
    assert_eq!(trace.borrow().simplified, vec![vec![1]]);

    assert!(solver.assume(pos_lit(2)));
    assert!(solver.backtrack(0));
    assert_eq!(trace.borrow().undone, vec![vec![2]]);
}

#[test]
fn solver_resolve_to_flagged_collects_flagged_literals_and_lbd() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_typed_var(VarType::Atom, VarInfo::FLAG_INPUT);
    let b = ctx.add_typed_var(VarType::Atom, VarInfo::FLAG_INPUT);
    let solver = ctx.start_add_constraints();

    assert!(solver.assume(pos_lit(a)));
    assert!(solver.assume(pos_lit(b)));

    let mut out = LitVec::new();
    let mut out_lbd = 0;
    assert!(solver.resolve_to_flagged(
        &[pos_lit(b), pos_lit(a)],
        VarInfo::FLAG_INPUT,
        &mut out,
        &mut out_lbd,
    ));
    assert_eq!(out.as_slice(), &[pos_lit(b), pos_lit(a)]);
    assert_eq!(out_lbd, 2);
    assert!(!solver.seen_var(a));
    assert!(!solver.seen_var(b));
}

#[test]
fn solver_resolve_to_core_projects_conflict_to_decision_literals() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let solver = ctx.start_add_constraints();

    assert!(solver.assume(pos_lit(a)));
    assert!(solver.assume(pos_lit(b)));
    assert!(!solver.force(neg_lit(b), Antecedent::from_literal(pos_lit(a))));

    let mut core = LitVec::new();
    solver.resolve_to_core(&mut core);

    assert_eq!(core.as_slice(), &[pos_lit(b), pos_lit(a)]);
}

#[test]
fn shared_context_step_var_creation_matches_upstream_step_var_sections() {
    let mut requested = SharedContext::default();
    assert_eq!(
        requested.step_literal(),
        rust_clasp::clasp::literal::lit_true
    );
    requested.request_step_var();
    assert_eq!(requested.num_vars(), 0);
    requested.start_add_constraints();
    assert_eq!(requested.num_vars(), 0);
    assert!(requested.end_init());
    assert_eq!(requested.num_vars(), 1);
    assert_eq!(requested.stats().vars.num, 0);
    assert_eq!(requested.step_literal(), pos_lit(1));

    let mut missing_start = SharedContext::default();
    missing_start.add_typed_var(VarType::Atom, 0);
    missing_start.add_typed_var(VarType::Atom, 0);
    missing_start.request_step_var();
    assert_eq!(missing_start.num_vars(), 2);
    assert!(missing_start.end_init());
    assert_eq!(missing_start.num_vars(), 3);
    assert_eq!(missing_start.stats().vars.num, 2);
    assert_eq!(missing_start.step_literal(), pos_lit(3));
    assert_eq!(
        missing_start.master_ref().num_vars(),
        missing_start.num_vars()
    );

    let mut required = SharedContext::default();
    assert_eq!(required.require_step_var(), pos_lit(1));
    assert_eq!(required.num_vars(), 1);
    assert_eq!(required.step_literal(), pos_lit(1));
    assert!(required.var_info(1).frozen());
    assert!(required.end_init());
    assert_eq!(required.num_vars(), 1);
    assert_eq!(required.stats().vars.num, 0);
}

#[test]
fn shared_context_pop_vars_after_commit_shrinks_master_assignment() {
    let mut ctx = SharedContext::default();
    ctx.add_typed_var(VarType::Atom, 0);
    ctx.add_typed_var(VarType::Atom, 0);
    ctx.add_typed_var(VarType::Atom, 0);

    {
        let solver = ctx.start_add_constraints();
        assert_eq!(solver.num_vars(), 3);
        assert_eq!(solver.num_free_vars(), 3);
    }

    ctx.pop_vars(2);
    assert_eq!(ctx.num_vars(), 1);
    assert!(ctx.end_init());
    assert_eq!(ctx.master_ref().num_vars(), 1);
    assert_eq!(ctx.master_ref().num_free_vars(), 1);
}

#[test]
fn shared_context_unfreeze_simplifies_step_literal_when_it_is_last_var() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_typed_var(VarType::Atom, 0);
    let b = ctx.add_typed_var(VarType::Atom, 0);
    assert_eq!(ctx.num_vars(), 2);

    let step = ctx.require_step_var();
    assert!(ctx.add_ternary(pos_lit(a), pos_lit(b), !step));
    assert_eq!(ctx.num_ternary(), 1);
    assert_eq!(ctx.num_vars(), 3);
    assert_eq!(ctx.stats().vars.num, 2);

    assert!(ctx.end_init());
    assert!(ctx.unfreeze());
    assert_eq!(ctx.num_ternary(), 0);
    assert_eq!(ctx.step_literal().var(), 0);
    assert_eq!(ctx.num_vars(), 2);
    assert_eq!(ctx.stats().vars.num, 2);
}

#[test]
fn shared_context_unfreeze_keeps_step_var_when_it_is_not_last() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_typed_var(VarType::Atom, 0);
    let b = ctx.add_typed_var(VarType::Atom, 0);
    let step = ctx.require_step_var();
    assert!(ctx.add_ternary(pos_lit(a), pos_lit(b), !step));
    let extra = ctx.add_typed_var(VarType::Atom, 0);
    assert!(ctx.add_binary(pos_lit(a), pos_lit(extra)));

    assert_eq!(extra, 4);
    assert_eq!(ctx.num_vars(), 4);
    assert_eq!(ctx.stats().vars.num, 3);

    assert!(ctx.end_init());
    assert!(ctx.unfreeze());
    assert_eq!(ctx.num_ternary(), 0);
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.step_literal().var(), 0);
    assert_eq!(ctx.num_vars(), 4);
    assert_eq!(ctx.stats().vars.num, 3);
    assert!(ctx.master_ref().is_false(pos_lit(3)));
    assert_eq!(ctx.master_ref().value(4), value_free);
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
fn shared_context_attach_clones_explicit_db_and_detach_clears_local_runtime() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();
    let e = ctx.add_var();
    let f = ctx.add_var();

    {
        let solver = ctx.start_add_constraints();
        assert!(solver.force(pos_lit(a), Antecedent::new()));
        let mut creator = ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(neg_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(pos_lit(d))
                .end_with_defaults()
                .ok()
        );
        assert!(solver.add(
            &ClauseRep::prepared(
                &[neg_lit(e), pos_lit(f)],
                ClauseInfo::new(ConstraintType::Static),
            ),
            true,
        ));
    }
    assert!(ctx.end_init());
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.master().num_constraints(), 1);

    let attached_ptr = {
        let solver = ctx.push_solver();
        solver as *mut Solver
    };
    let attached_id = unsafe { (*attached_ptr).id() };

    assert!(unsafe { ctx.attach_solver(attached_ptr) });
    {
        let solver = ctx.solver(attached_id).unwrap();
        assert_eq!(solver.num_constraints(), 1);
        assert!(solver.is_true(pos_lit(a)));
        assert!(solver.assume(neg_lit(c)));
        assert!(solver.assume(neg_lit(d)));
        assert!(solver.propagate());
        assert!(solver.is_true(pos_lit(b)));
        assert!(solver.assume(pos_lit(e)));
        assert!(solver.propagate());
        assert!(solver.is_true(pos_lit(f)));
    }

    unsafe { ctx.detach_solver(attached_ptr, true) };
    {
        let solver = ctx.solver(attached_id).unwrap();
        assert_eq!(solver.num_constraints(), 0);
        assert_eq!(solver.value(a), value_free);
        assert_eq!(solver.value(b), value_free);
    }

    assert!(ctx.attach(attached_id));
    {
        let solver = ctx.solver(attached_id).unwrap();
        assert_eq!(solver.num_constraints(), 1);
        assert!(solver.is_true(pos_lit(a)));
        assert_eq!(solver.value(b), value_free);
        assert!(solver.assume(pos_lit(e)));
        assert!(solver.propagate());
        assert!(solver.is_true(pos_lit(f)));
    }
}

#[test]
fn shared_context_end_init_with_attach_all_attaches_existing_local_solvers() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    {
        let solver = ctx.start_add_constraints();
        let mut creator = ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(a)
                .add(b)
                .add(c)
                .add(d)
                .end_with_defaults()
                .ok()
        );
    }

    let attached_id = {
        let solver = ctx.push_solver();
        solver.id()
    };

    assert!(ctx.end_init_with_attach_all(true));
    assert_eq!(ctx.master_ref().num_constraints(), 1);
    assert_eq!(ctx.solver_ref(attached_id).unwrap().num_constraints(), 1);

    assert!(ctx.unfreeze());
    {
        let solver = ctx.start_add_constraints();
        let mut creator = ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(!a)
                .add(!b)
                .add(c)
                .add(d)
                .end_with_defaults()
                .ok()
        );
    }

    assert!(ctx.end_init());
    assert_eq!(ctx.master_ref().num_constraints(), 2);
    assert_eq!(ctx.solver_ref(attached_id).unwrap().num_constraints(), 1);

    assert!(ctx.unfreeze());
    {
        let solver = ctx.start_add_constraints();
        let mut creator = ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Static)
                .add(a)
                .add(b)
                .add(!c)
                .add(!d)
                .end_with_defaults()
                .ok()
        );
    }

    assert!(ctx.end_init_with_attach_all(true));
    assert_eq!(ctx.master_ref().num_constraints(), 3);
    assert_eq!(ctx.solver_ref(attached_id).unwrap().num_constraints(), 3);
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
fn solver_post_propagators_follow_init_priority_and_incremental_attachment_rules() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();

    let simple_trace = Rc::new(RefCell::new(PostTrace::default()));
    let medium_trace = Rc::new(RefCell::new(PostTrace::default()));
    let general_trace = Rc::new(RefCell::new(PostTrace::default()));
    let late_trace = Rc::new(RefCell::new(PostTrace::default()));

    let solver = ctx.start_add_constraints();
    assert!(add_tracking_post(
        solver,
        Rc::clone(&general_trace),
        priority_class_general,
        false,
    ));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&simple_trace),
        10,
        false
    ));
    assert!(add_tracking_post(
        solver,
        Rc::clone(&medium_trace),
        20,
        false
    ));

    let simple = solver.get_post(10).expect("missing priority 10 post");
    let medium = solver.get_post(20).expect("missing priority 20 post");
    let general = solver
        .get_post(priority_class_general)
        .expect("missing general post");
    unsafe {
        assert_eq!(simple.as_ref().next, Some(medium));
        assert_eq!(medium.as_ref().next, Some(general));
        assert!(general.as_ref().next.is_none());
    }

    assert!(solver.propagate());
    assert_eq!(simple_trace.borrow().props, 0);
    assert_eq!(medium_trace.borrow().props, 0);
    assert_eq!(general_trace.borrow().props, 0);

    assert!(ctx.end_init());
    assert_eq!(simple_trace.borrow().inits, 1);
    assert_eq!(medium_trace.borrow().inits, 1);
    assert_eq!(general_trace.borrow().inits, 1);
    assert_eq!(simple_trace.borrow().props, 1);
    assert_eq!(medium_trace.borrow().props, 1);
    assert_eq!(general_trace.borrow().props, 1);

    let solver = ctx.master();
    let removed = solver.remove_post(medium).expect("expected removed post");
    assert!(solver.propagate());
    assert_eq!(simple_trace.borrow().props, 2);
    assert_eq!(medium_trace.borrow().props, 1);
    assert_eq!(general_trace.borrow().props, 2);

    assert!(solver.add_post(removed));
    assert_eq!(medium_trace.borrow().inits, 2);
    assert!(add_tracking_post(
        solver,
        Rc::clone(&late_trace),
        priority_class_general,
        false,
    ));
    assert_eq!(late_trace.borrow().inits, 1);

    let medium = solver.get_post(20).expect("missing reattached medium post");
    let general = solver
        .get_post(priority_class_general)
        .expect("missing general post after reattach");
    unsafe {
        assert_eq!(medium.as_ref().next, Some(general));
        assert!(general.as_ref().next.is_some());
    }
}

#[test]
fn solver_post_propagation_helpers_match_upstream_subset() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();

    let p1_trace = Rc::new(RefCell::new(PostTrace::default()));
    let p2_trace = Rc::new(RefCell::new(PostTrace::default()));
    let p3_trace = Rc::new(RefCell::new(PostTrace::default()));

    let solver = ctx.start_add_constraints();
    assert!(add_tracking_post(solver, Rc::clone(&p1_trace), 10, false));
    assert!(add_tracking_post(solver, Rc::clone(&p2_trace), 20, false));
    assert!(add_tracking_post(solver, Rc::clone(&p3_trace), 30, false));
    assert!(ctx.end_init());

    let solver = ctx.master();
    p1_trace.borrow_mut().props = 0;
    p2_trace.borrow_mut().props = 0;
    p3_trace.borrow_mut().props = 0;

    let p3 = solver.get_post(30).expect("missing priority 30 post");
    assert_eq!(solver.propagate_until(None, Some(20)), value_free);
    assert_eq!(p1_trace.borrow().props, 1);
    assert_eq!(p2_trace.borrow().props, 1);
    assert_eq!(p3_trace.borrow().props, 0);

    assert!(solver.propagate_from(p3));
    assert_eq!(p1_trace.borrow().props, 1);
    assert_eq!(p2_trace.borrow().props, 1);
    assert_eq!(p3_trace.borrow().props, 1);

    p1_trace.borrow_mut().props = 0;
    p2_trace.borrow_mut().props = 0;
    p3_trace.borrow_mut().props = 0;
    assert_eq!(solver.propagate_until(None, None), value_true);
    assert_eq!(p1_trace.borrow().props, 1);
    assert_eq!(p2_trace.borrow().props, 1);
    assert_eq!(p3_trace.borrow().props, 1);
}

#[test]
fn solver_push_root_and_stop_conflict_cover_current_control_surface() {
    let mut solver = Solver::new();
    solver.set_num_vars(3);
    solver.set_num_problem_vars(3);

    assert!(solver.assume(pos_lit(1)));
    assert!(solver.propagate());
    assert!(solver.assume(pos_lit(2)));
    assert_eq!(solver.decision_level(), 2);
    assert_eq!(solver.queue_size(), 1);

    solver.push_root_level(1);
    assert_eq!(solver.root_level(), 1);
    assert_eq!(solver.backtrack_level(), 1);

    assert!(solver.push_root(pos_lit(3)));
    assert_eq!(solver.root_level(), 2);
    assert_eq!(solver.decision_level(), 2);
    assert_eq!(solver.stats().core.choices, 2);
    assert!(solver.is_true(pos_lit(1)));
    assert!(solver.is_true(pos_lit(3)));
    assert_eq!(solver.value(2), value_free);

    solver.set_backtrack_level(1);
    let saved_bt = solver.backtrack_level();
    solver.set_backtrack_level_with_mode(0, UndoMode::Default);
    assert_eq!(solver.backtrack_level(), saved_bt);
    solver.set_backtrack_level_with_mode(0, UndoMode::PopProjLevel);
    assert_eq!(solver.backtrack_level(), solver.root_level());
    assert_eq!(solver.jump_level(), solver.decision_level());
    assert_eq!(solver.queue_size(), 0);

    assert!(solver.assume(neg_lit(2)));
    assert_eq!(solver.queue_size(), 1);
    solver.set_stop_conflict();
    assert!(solver.has_stop_conflict());
    assert!(solver.has_conflict());
    assert_eq!(solver.root_level(), solver.decision_level());
    assert_eq!(solver.backtrack_level(), solver.decision_level());

    solver.clear_stop_conflict();
    assert!(!solver.has_stop_conflict());
    assert!(!solver.has_conflict());
    assert_eq!(solver.backtrack_level(), saved_bt);
    assert_eq!(solver.queue_size(), 1);
    assert!(solver.propagate());

    assert!(solver.pop_root_level(1));
    assert_eq!(solver.root_level(), 1);
    assert_eq!(solver.decision_level(), 1);
    assert!(solver.is_true(pos_lit(1)));
    assert_eq!(solver.trail_view(0), &[pos_lit(1)]);
    assert_eq!(solver.value(3), value_free);
}

#[test]
fn solver_push_tag_var_creates_one_aux_var_and_can_push_it_to_root() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    let tag = solver.push_tag_var(false);
    assert_eq!(tag, 3);
    assert_eq!(solver.tag_literal(), pos_lit(tag));
    assert_eq!(solver.num_aux_vars(), 1);
    assert_eq!(solver.root_level(), 0);
    assert_eq!(solver.decision_level(), 0);

    let same = solver.push_tag_var(true);
    assert_eq!(same, tag);
    assert_eq!(solver.num_aux_vars(), 1);
    assert_eq!(solver.root_level(), 1);
    assert_eq!(solver.decision_level(), 1);
    assert!(solver.is_true(pos_lit(tag)));
}

#[test]
fn solver_remove_conditional_deletes_tagged_learnt_clause() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let tag = pos_lit(solver.push_tag_var(false));
    let mut creator = ClauseCreator::new(Some(solver));
    assert!(
        creator
            .start(ConstraintType::Conflict)
            .add(pos_lit(a))
            .add(pos_lit(b))
            .add(pos_lit(c))
            .add(!tag)
            .end_with_defaults()
            .ok()
    );
    assert_eq!(solver.num_learnt_constraints(), 1);

    solver.remove_conditional();
    assert_eq!(solver.num_learnt_constraints(), 0);
}

#[test]
fn solver_pop_root_level_removes_tagged_learnts_when_tag_literal_stops_holding() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let tag = pos_lit(solver.push_tag_var(true));
    let mut creator = ClauseCreator::new(Some(solver));
    assert!(
        creator
            .start(ConstraintType::Conflict)
            .add(pos_lit(a))
            .add(pos_lit(b))
            .add(pos_lit(c))
            .add(!tag)
            .end_with_defaults()
            .ok()
    );
    assert_eq!(solver.num_learnt_constraints(), 1);

    assert!(solver.pop_root_level(1));

    assert_eq!(solver.root_level(), 0);
    assert_eq!(solver.num_learnt_constraints(), 0);
}

#[test]
fn solver_strengthen_conditional_moves_tagged_learnt_clause_to_short_db() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    {
        let solver = ctx.master();
        let tag = pos_lit(solver.push_tag_var(false));
        let mut creator = ClauseCreator::new(Some(solver));
        assert!(
            creator
                .start(ConstraintType::Conflict)
                .add(pos_lit(a))
                .add(pos_lit(b))
                .add(pos_lit(c))
                .add(!tag)
                .end_with_defaults()
                .ok()
        );
        assert_eq!(solver.num_learnt_constraints(), 1);

        solver.strengthen_conditional();
    }

    assert!(ctx.num_learnt_short() == 1 || ctx.num_ternary() == 1);
}

#[test]
fn solver_learns_conditional_clause_and_strengthens_it_to_unconditional_knowledge() {
    let mut ctx = SharedContext::default();
    let b = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let tag = pos_lit(solver.push_tag_var(true));
    assert!(solver.assume(pos_lit(b)));
    assert!(solver.propagate());

    let conflict = Box::into_raw(Box::new(Constraint::new(ConflictReasonConstraint {
        ante: vec![tag, pos_lit(b)],
    })));
    assert!(!solver.force(lit_false, Antecedent::from_constraint_ptr(conflict)));
    unsafe {
        Constraint::destroy_raw(conflict, None, false);
    }

    assert!(solver.resolve_conflict());
    let shared = solver
        .shared_context()
        .expect("master solver must have context");
    assert_eq!(shared.num_learnt_short(), 0);
    assert_eq!(shared.num_binary(), 0);
    assert_eq!(solver.num_learnt_constraints(), 1);
    assert_eq!(solver.decision_level(), 1);

    solver.strengthen_conditional();
    assert!(solver.clear_assumptions());
    assert!(solver.is_true(neg_lit(b)));
}

#[test]
fn solver_state_accessors_match_existing_assignment_storage() {
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(3);

    assert_eq!(solver.num_aux_vars(), 1);
    assert_eq!(solver.num_free_vars(), 4);
    assert_eq!(solver.top_value(1), value_free);
    assert!(!solver.valid_level(0));
    assert!(!solver.valid_level(1));
    assert_eq!(solver.level_lits(1), &[]);

    assert!(solver.force(pos_lit(3), Antecedent::new()));
    assert_eq!(solver.top_value(3), value_true);
    assert_eq!(solver.num_free_vars(), 3);

    assert!(solver.assume(pos_lit(1)));
    assert!(solver.propagate());
    assert!(solver.assume(neg_lit(2)));

    assert!(solver.valid_level(1));
    assert!(solver.valid_level(2));
    assert!(!solver.valid_level(3));
    assert_eq!(solver.level_lits(1), &[pos_lit(1)]);
    assert_eq!(solver.level_lits(2), &[neg_lit(2)]);
    assert_eq!(solver.level_lits(3), &[]);
    assert_eq!(solver.reason_literal(pos_lit(1)), &Antecedent::new());
    assert_eq!(solver.reason_data_literal(pos_lit(1)), u32::MAX);
    assert_eq!(solver.top_value(1), value_free);
    assert_eq!(solver.num_free_vars(), 1);
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

#[test]
fn short_implication_graph_remove_tracks_static_and_learnt_edges() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let _ = ctx.start_add_constraints();
    assert!(ctx.add_binary(pos_lit(a), pos_lit(b)));
    assert!(ctx.add_ternary(pos_lit(a), pos_lit(b), pos_lit(c)));
    assert_eq!(ctx.num_binary(), 1);
    assert_eq!(ctx.num_ternary(), 1);

    assert_eq!(
        ctx.add_imp(&[neg_lit(a), neg_lit(b)], ConstraintType::Conflict),
        1
    );
    assert_eq!(ctx.num_learnt_short(), 1);

    ctx.remove_imp(&[pos_lit(a), pos_lit(b)], false);
    ctx.remove_imp(&[pos_lit(a), pos_lit(b), pos_lit(c)], false);
    ctx.remove_imp(&[neg_lit(a), neg_lit(b)], true);

    assert_eq!(ctx.num_binary(), 0);
    assert_eq!(ctx.num_ternary(), 0);
    assert_eq!(ctx.num_learnt_short(), 0);
}

#[test]
fn solver_simplify_removes_satisfied_binary_short_clauses() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[a, c], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!a, d], ClauseInfo::new(ConstraintType::Static)),
        true
    ));

    assert!(solver.force(a, Antecedent::new()));
    assert!(solver.simplify());
    assert_eq!(ctx.num_binary(), 0);
}

#[test]
fn solver_simplify_removes_satisfied_ternary_and_keeps_binary_residual() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b, d], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!a, b, c], ClauseInfo::new(ConstraintType::Static)),
        true
    ));

    assert!(solver.force(a, Antecedent::new()));
    assert!(solver.simplify());
    assert!(solver.assume(!b));
    let shared = solver.shared_context().unwrap();
    assert_eq!(shared.num_ternary(), 0);
    assert_eq!(shared.num_binary(), 1);
    assert!(solver.propagate());
    assert!(solver.is_true(c));
}

#[test]
fn solver_estimate_bcp_matches_upstream_binary_walk_and_loop_behavior() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let e = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!b, c], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!c, d], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!d, e], ClauseInfo::new(ConstraintType::Static)),
        true
    ));

    for depth in 0..4 {
        assert_eq!(solver.estimate_bcp(!a, depth), depth as u32 + 2);
    }

    let mut loop_ctx = SharedContext::default();
    let a = pos_lit(loop_ctx.add_var());
    let b = pos_lit(loop_ctx.add_var());
    let c = pos_lit(loop_ctx.add_var());
    let solver = loop_ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!b, c], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(solver.add(
        &ClauseRep::prepared(&[!c, !a], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert_eq!(solver.estimate_bcp(!a, -1), 3);
}

#[test]
fn solver_assert_immediate_matches_upstream_reason_shapes() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let q = pos_lit(ctx.add_var());
    let f = pos_lit(ctx.add_var());
    let x = pos_lit(ctx.add_var());
    let z = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    let mut creator = ClauseCreator::new(Some(solver));
    creator.add_default_flags(CLAUSE_WATCH_FIRST);
    creator
        .start(ConstraintType::Static)
        .add(!z)
        .add(d)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(b)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(!b)
        .add(z)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(!b)
        .add(!z)
        .add(d)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(!b)
        .add(!z)
        .add(!d)
        .add(q)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(!q)
        .add(f)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(!f)
        .add(!z)
        .add(x)
        .end_with_defaults();

    assert!(solver.assume(!a));
    assert!(solver.propagate());

    assert_eq!(solver.num_assigned_vars(), 7);

    let why_b = *solver.reason(b.var());
    let why_z = *solver.reason(z.var());
    let why_d = *solver.reason(d.var());
    let why_q = *solver.reason(q.var());
    let why_f = *solver.reason(f.var());
    let why_x = *solver.reason(x.var());

    assert_eq!(why_b.type_(), Antecedent::BINARY);
    assert_eq!(why_b.first_literal(), !a);

    assert_eq!(why_z.type_(), Antecedent::TERNARY);
    assert_eq!(why_z.first_literal(), !a);
    assert_eq!(why_z.second_literal(), b);

    assert_eq!(why_d.type_(), Antecedent::GENERIC);
    assert_eq!(why_q.type_(), Antecedent::GENERIC);

    assert_eq!(why_f.type_(), Antecedent::BINARY);
    assert_eq!(why_f.first_literal(), q);

    assert_eq!(why_x.type_(), Antecedent::TERNARY);
    assert_eq!(why_x.first_literal(), f);
    assert_eq!(why_x.second_literal(), z);
}

#[test]
fn solver_prefer_short_bfs_matches_upstream_reason_selection() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let p = pos_lit(ctx.add_var());
    let q = pos_lit(ctx.add_var());
    let x = pos_lit(ctx.add_var());
    let y = pos_lit(ctx.add_var());
    let z = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    let mut creator = ClauseCreator::new(Some(solver));
    creator.add_default_flags(CLAUSE_WATCH_LEAST);
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(x)
        .add(y)
        .add(p)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(x)
        .add(y)
        .add(z)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(p)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(!p)
        .add(z)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(!z)
        .add(b)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(x)
        .add(q)
        .add(!b)
        .end_with_defaults();
    creator
        .start(ConstraintType::Static)
        .add(a)
        .add(!b)
        .add(!p)
        .add(!q)
        .end_with_defaults();

    let shared = solver.shared_context().expect("shared context");
    assert_eq!(shared.num_binary(), 2);
    assert_eq!(shared.num_ternary(), 1);

    assert!(solver.assume(!x));
    assert!(solver.propagate());
    assert!(solver.assume(!y));
    assert!(solver.propagate());
    assert_eq!(solver.num_assigned_vars(), 2);
    assert!(solver.assume(!a));

    assert!(!solver.propagate());
    assert_eq!(solver.num_assigned_vars(), 7);

    assert_eq!(solver.reason(b.var()).type_(), Antecedent::BINARY);
    assert_eq!(solver.reason(p.var()).type_(), Antecedent::BINARY);
    assert_eq!(solver.reason(z.var()).type_(), Antecedent::TERNARY);
    assert_eq!(solver.reason(q.var()).type_(), Antecedent::GENERIC);
}

#[test]
fn solver_reverse_arc_reports_binary_and_ternary_short_reasons() {
    let mut binary_ctx = SharedContext::default();
    let a = pos_lit(binary_ctx.add_var());
    let b = pos_lit(binary_ctx.add_var());

    let solver = binary_ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(binary_ctx.end_init());

    let solver = binary_ctx.master();
    assert!(solver.force(!b, Antecedent::new()));
    solver.mark_seen_literal(!b);

    let binary = solver.reverse_arc(!a, 1).unwrap();
    assert_eq!(binary.type_(), Antecedent::BINARY);
    assert_eq!(binary.first_literal(), !b);

    let mut ternary_ctx = SharedContext::default();
    let a = pos_lit(ternary_ctx.add_var());
    let b = pos_lit(ternary_ctx.add_var());
    let c = pos_lit(ternary_ctx.add_var());

    let solver = ternary_ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(&[a, b, c], ClauseInfo::new(ConstraintType::Static)),
        true
    ));
    assert!(ternary_ctx.end_init());

    let solver = ternary_ctx.master();
    assert!(solver.force(!b, Antecedent::new()));
    assert!(solver.force(!c, Antecedent::new()));
    solver.mark_seen_literal(!b);
    solver.mark_seen_literal(!c);

    let ternary = solver.reverse_arc(!a, 1).unwrap();
    assert_eq!(ternary.type_(), Antecedent::TERNARY);
    assert_eq!(ternary.first_literal(), !b);
    assert_eq!(ternary.second_literal(), !c);
}

#[test]
fn solver_resolve_conflict_uses_reverse_arc_to_strengthen_learnt_clause() {
    let mut ctx = SharedContext::default();
    let q = pos_lit(ctx.add_var());
    let a = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let x = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    solver.strategies_mut().reverse_arcs = 1;
    assert!(solver.add(
        &ClauseRep::prepared(&[!q, c], ClauseInfo::new(ConstraintType::Static)),
        true,
    ));
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(q));
    assert!(solver.assume(a) && solver.propagate());
    assert!(solver.is_true(c));
    assert!(solver.assume(d) && solver.propagate());
    assert!(solver.force(x, Antecedent::from_literal(d)));

    let conflict = Box::into_raw(Box::new(Constraint::new(ConflictReasonConstraint {
        ante: vec![c, d, x],
    })));
    assert!(!solver.force(lit_false, Antecedent::from_constraint_ptr(conflict)));
    unsafe {
        Constraint::destroy_raw(conflict, None, false);
    }

    assert!(solver.resolve_conflict());
    assert!(solver.is_true(!d));
    assert_eq!(solver.decision_level(), 1);

    let mut antecedent = *solver.reason(d.var());
    assert_eq!(antecedent.type_(), Antecedent::BINARY);
    let mut reason = LitVec::new();
    antecedent.reason(solver, !d, &mut reason);
    reason.push_back(!d);

    assert_eq!(reason.len(), 2);
    assert!(contains(reason.as_slice(), &q));
    assert!(contains(reason.as_slice(), &!d));
    assert!(!contains(reason.as_slice(), &a));
    assert!(!contains(reason.as_slice(), &c));
}

#[test]
fn solver_resolve_conflict_strengthens_subsumed_rhs_clause() {
    let mut ctx = SharedContext::default();
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let r = pos_lit(ctx.add_var());
    let x = pos_lit(ctx.add_var());

    let solver = ctx.start_add_constraints();
    let mut creator = ClauseCreator::new(Some(solver));
    let rhs = creator
        .start(ConstraintType::Static)
        .add(!c)
        .add(!d)
        .add(!r)
        .add(x)
        .end_with_defaults();
    assert!(rhs.ok());
    assert!(!rhs.local.is_null());

    assert!(solver.assume(c));
    assert!(solver.propagate());
    assert!(solver.assume(d));
    assert!(solver.propagate());
    assert!(solver.assume(r));
    assert!(solver.propagate());
    assert!(solver.is_true(x));

    let conflict = Box::into_raw(Box::new(Constraint::new(ConflictReasonConstraint {
        ante: vec![x, r],
    })));
    assert!(!solver.force(lit_false, Antecedent::from_constraint_ptr(conflict)));
    unsafe {
        Constraint::destroy_raw(conflict, None, false);
    }

    assert!(solver.resolve_conflict());

    let rhs_lits = unsafe { (*rhs.local).to_lits() };
    assert_eq!(rhs_lits.len(), 3);
    assert!(contains(&rhs_lits, &!c));
    assert!(contains(&rhs_lits, &!d));
    assert!(contains(&rhs_lits, &!r));
    assert!(!contains(&rhs_lits, &x));
}

#[test]
fn solver_cc_minimize_matches_seen_and_marked_level_recursion() {
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(4);
    solver.set_decision_level(3);

    let a = pos_lit(1);
    let b = pos_lit(2);
    let d = pos_lit(4);

    solver.set_value(a.var(), value_true, 1);
    solver.set_value(b.var(), value_true, 2);
    solver.set_value(d.var(), value_true, 3);
    solver.set_reason(b.var(), Antecedent::from_literal(a));
    solver.set_reason(d.var(), Antecedent::from_literal(b));

    assert!(!solver.cc_minimize(d, std::ptr::null_mut()));

    solver.mark_level(3);
    let mut rec = CCMinRecursive::default();
    solver.prepare_cc_min_recursive(&mut rec);
    assert!(!solver.cc_minimize(d, &mut rec));

    solver.mark_level(2);
    solver.mark_seen_var(a.var());
    solver.prepare_cc_min_recursive(&mut rec);
    assert!(solver.cc_minimize(d, &mut rec));
}

#[test]
fn solver_cc_minimize_honors_recursive_antecedent_limits() {
    let mut solver = Solver::new();
    solver.set_num_vars(4);
    solver.set_num_problem_vars(4);
    solver.set_decision_level(3);

    let a = pos_lit(1);
    let b = pos_lit(2);
    let c = pos_lit(3);
    let d = pos_lit(4);

    solver.set_value(a.var(), value_true, 1);
    solver.set_value(b.var(), value_true, 2);
    solver.set_value(c.var(), value_true, 2);
    solver.set_value(d.var(), value_true, 3);
    solver.set_reason(b.var(), Antecedent::from_literal(a));
    solver.set_reason(d.var(), Antecedent::from_literals(b, c));

    solver.mark_level(3);
    solver.mark_level(2);
    solver.mark_seen_var(a.var());
    solver.mark_seen_var(c.var());

    let mut rec = CCMinRecursive::default();
    solver.strategies_mut().cc_min_antes = CCMinAntes::AllAntes as u32;
    solver.prepare_cc_min_recursive(&mut rec);
    assert!(solver.cc_minimize(d, &mut rec));

    solver.strategies_mut().cc_min_antes = CCMinAntes::BinaryAntes as u32;
    solver.prepare_cc_min_recursive(&mut rec);
    assert!(!solver.cc_minimize(d, &mut rec));
}

#[test]
fn solver_resolve_conflict_learns_unit_uip_from_short_implications() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();

    let solver = ctx.start_add_constraints();
    solver.stats_mut().enable_extended();
    assert!(solver.add(
        &ClauseRep::prepared(
            &[pos_lit(a), pos_lit(b)],
            ClauseInfo::new(ConstraintType::Static)
        ),
        true,
    ));
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(b), pos_lit(c)],
            ClauseInfo::new(ConstraintType::Static)
        ),
        true,
    ));
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(a), pos_lit(c)],
            ClauseInfo::new(ConstraintType::Static)
        ),
        true,
    ));
    assert!(!solver.assume(neg_lit(c)) || !solver.propagate());

    assert!(solver.resolve_conflict());
    assert!(!solver.has_conflict());
    assert!(solver.is_true(pos_lit(c)));
    assert_eq!(solver.decision_level(), 0);
    let conflict_index = ExtendedStats::index(ConstraintType::Conflict).unwrap();
    assert_eq!(
        solver.stats().extra.as_ref().unwrap().learnts[conflict_index],
        1
    );
}

#[test]
fn solver_resolve_conflict_builds_first_uip_learnt_clause() {
    let mut ctx = SharedContext::default();
    let x1 = pos_lit(ctx.add_var());
    let x2 = pos_lit(ctx.add_var());
    let x3 = pos_lit(ctx.add_var());
    let x4 = pos_lit(ctx.add_var());
    let x5 = pos_lit(ctx.add_var());
    let x6 = pos_lit(ctx.add_var());
    let x7 = pos_lit(ctx.add_var());
    let x8 = pos_lit(ctx.add_var());
    let x9 = pos_lit(ctx.add_var());
    let x10 = pos_lit(ctx.add_var());
    let x11 = pos_lit(ctx.add_var());
    let x12 = pos_lit(ctx.add_var());
    let x13 = pos_lit(ctx.add_var());
    let x14 = pos_lit(ctx.add_var());
    let x15 = pos_lit(ctx.add_var());
    let x16 = pos_lit(ctx.add_var());
    let x17 = pos_lit(ctx.add_var());

    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        let mut creator = ClauseCreator::new(Some(solver));
        creator
            .start(ConstraintType::Static)
            .add(!x11)
            .add(x12)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x1)
            .add(!x12)
            .add(!x13)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x4)
            .add(!x12)
            .add(x14)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x13)
            .add(!x14)
            .add(!x15)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x2)
            .add(x15)
            .add(x16)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x3)
            .add(x15)
            .add(!x17)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x6)
            .add(!x16)
            .add(x17)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x2)
            .add(x9)
            .add(x10)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x4)
            .add(!x7)
            .add(!x8)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x5)
            .add(x6)
            .end_with_defaults();
    }
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(!x1) && solver.propagate());
    assert!(solver.assume(x2) && solver.propagate());
    assert!(solver.assume(!x3) && solver.propagate());
    assert!(solver.assume(x4) && solver.propagate());
    assert!(solver.assume(!x5) && solver.propagate());
    assert!(solver.assume(x7) && solver.propagate());
    assert!(solver.assume(!x9) && solver.propagate());
    assert!(!solver.assume(x11) || !solver.propagate());

    assert!(solver.resolve_conflict());
    assert!(solver.is_true(x15));
    assert_eq!(solver.decision_level(), 5);
    let mut antecedent = *solver.reason(x15.var());
    assert_eq!(antecedent.type_(), Antecedent::GENERIC);

    let mut reason = LitVec::new();
    antecedent.reason(solver, x15, &mut reason);
    reason.push_back(x15);
    assert_eq!(reason.len(), 4);
    assert!(contains(reason.as_slice(), &x2));
    assert!(contains(reason.as_slice(), &!x3));
    assert!(contains(reason.as_slice(), &x6));
    assert!(contains(reason.as_slice(), &x15));
}

#[test]
fn solver_resolve_conflict_can_replace_learnt_clause_with_decision_sequence() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let e = pos_lit(ctx.add_var());
    let x = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    solver.strategies_mut().compress = 0;
    solver.strategies_mut().cc_rep_mode = CCRepMode::CcRepDecision as u32;

    assert!(solver.assume(a));
    assert!(solver.assume(b));
    assert!(solver.force(e, Antecedent::from_literal(b)));
    assert!(solver.assume(c));
    assert!(solver.assume(d));
    assert!(solver.force(x, Antecedent::from_literal(d)));

    let conflict = Box::into_raw(Box::new(Constraint::new(ConflictReasonConstraint {
        ante: vec![a, b, e, c, d, x],
    })));
    assert!(!solver.force(lit_false, Antecedent::from_constraint_ptr(conflict)));
    unsafe {
        Constraint::destroy_raw(conflict, None, false);
    }

    assert!(solver.resolve_conflict());
    assert!(solver.is_true(!d));
    assert_eq!(solver.decision_level(), 3);

    let mut antecedent = *solver.reason(d.var());
    assert_eq!(antecedent.type_(), Antecedent::GENERIC);
    let mut reason = LitVec::new();
    antecedent.reason(solver, !d, &mut reason);
    reason.push_back(!d);

    assert_eq!(reason.len(), 4);
    assert!(contains(reason.as_slice(), &a));
    assert!(contains(reason.as_slice(), &b));
    assert!(contains(reason.as_slice(), &c));
    assert!(contains(reason.as_slice(), &!d));
    assert!(!contains(reason.as_slice(), &e));
}

#[test]
fn solver_resolve_conflict_respects_bounded_backjump_level() {
    let mut ctx = SharedContext::default();
    let x1 = pos_lit(ctx.add_var());
    let x2 = pos_lit(ctx.add_var());
    let x3 = pos_lit(ctx.add_var());
    let x4 = pos_lit(ctx.add_var());
    let x5 = pos_lit(ctx.add_var());
    let x6 = pos_lit(ctx.add_var());
    let x7 = pos_lit(ctx.add_var());
    let x8 = pos_lit(ctx.add_var());
    let x9 = pos_lit(ctx.add_var());
    let x10 = pos_lit(ctx.add_var());
    let x11 = pos_lit(ctx.add_var());
    let x12 = pos_lit(ctx.add_var());
    let x13 = pos_lit(ctx.add_var());
    let x14 = pos_lit(ctx.add_var());
    let x15 = pos_lit(ctx.add_var());
    let x16 = pos_lit(ctx.add_var());
    let x17 = pos_lit(ctx.add_var());

    let solver_ptr = ctx.start_add_constraints() as *mut Solver;
    unsafe {
        let solver = &mut *solver_ptr;
        let mut creator = ClauseCreator::new(Some(solver));
        creator
            .start(ConstraintType::Static)
            .add(!x11)
            .add(x12)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x1)
            .add(!x12)
            .add(!x13)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x4)
            .add(!x12)
            .add(x14)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x13)
            .add(!x14)
            .add(!x15)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x2)
            .add(x15)
            .add(x16)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x3)
            .add(x15)
            .add(!x17)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x6)
            .add(!x16)
            .add(x17)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x2)
            .add(x9)
            .add(x10)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(!x4)
            .add(!x7)
            .add(!x8)
            .end_with_defaults();
        creator
            .start(ConstraintType::Static)
            .add(x5)
            .add(x6)
            .end_with_defaults();
    }
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(!x1) && solver.propagate());
    assert!(solver.assume(x2) && solver.propagate());
    assert!(solver.assume(!x3) && solver.propagate());
    assert!(solver.assume(x4) && solver.propagate());
    assert!(solver.assume(!x5) && solver.propagate());
    assert!(solver.assume(x7) && solver.propagate());
    assert!(solver.assume(!x9) && solver.propagate());
    solver.set_backtrack_level(6);
    assert!(!solver.assume(x11) || !solver.propagate());

    assert!(solver.resolve_conflict());
    assert!(solver.is_true(x15));
    assert_eq!(solver.decision_level(), 6);

    let mut antecedent = *solver.reason(x15.var());
    assert_eq!(antecedent.type_(), Antecedent::GENERIC);

    let mut reason = LitVec::new();
    antecedent.reason(solver, x15, &mut reason);
    assert!(contains(reason.as_slice(), &x2));
    assert!(contains(reason.as_slice(), &!x3));
    assert!(contains(reason.as_slice(), &x6));

    let mut has_learnt_watch = false;
    for index in 0..solver.num_clause_watches(x6) as usize {
        let clause_head = solver.get_clause_watch(x6, index).unwrap().head;
        let lits = unsafe { (*clause_head).to_lits() };
        if lits.len() == 4
            && contains(lits.as_slice(), &!x2)
            && contains(lits.as_slice(), &x3)
            && contains(lits.as_slice(), &!x6)
            && contains(lits.as_slice(), &x15)
        {
            has_learnt_watch = true;
            assert!(solver.has_clause_watch(x6, clause_head));
            break;
        }
    }
    assert!(has_learnt_watch);

    assert!(solver.backtrack_step());
    assert!(solver.is_true(x15));
    assert_eq!(solver.decision_level(), 5);

    assert!(solver.backtrack_step());
    assert_eq!(solver.value(x15.var()), value_free);
    assert_eq!(solver.decision_level(), 4);
}

#[test]
fn solver_search_returns_true_on_model_completion() {
    let mut solver = Solver::new();
    solver.set_num_vars(2);
    solver.set_num_problem_vars(2);

    assert_eq!(solver.search(u64::MAX, u32::MAX, false, 0.0), value_true);
    assert!(solver.is_true(pos_lit(1)));
    assert!(solver.is_true(pos_lit(2)));
}

#[test]
fn solver_search_returns_false_on_root_level_conflict() {
    let mut solver = Solver::new();
    solver.set_num_vars(1);
    solver.set_num_problem_vars(1);

    assert!(solver.force(pos_lit(1), Antecedent::new()));
    assert!(!solver.force(neg_lit(1), Antecedent::new()));

    assert_eq!(solver.search(u64::MAX, u32::MAX, false, 0.0), value_false);
}

#[test]
fn solver_search_returns_free_when_restart_limit_is_reached() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();

    let solver = ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(a), pos_lit(b)],
            ClauseInfo::new(ConstraintType::Static),
        ),
        true,
    ));
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(a), neg_lit(b)],
            ClauseInfo::new(ConstraintType::Static),
        ),
        true,
    ));
    assert!(ctx.end_init());

    let solver = ctx.master();
    let mut limits = SearchLimits {
        restart_conflicts: 1,
        ..SearchLimits::default()
    };

    assert_eq!(solver.search_with_limits(&mut limits, 0.0), value_free);
    assert_eq!(limits.used, 1);
    assert!(solver.is_true(neg_lit(a)));
}

#[test]
fn solver_search_returns_free_when_learnt_memory_limit_is_reached() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    let d = ctx.add_var();

    let solver = ctx.start_add_constraints();
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(a), pos_lit(b)],
            ClauseInfo::new(ConstraintType::Static),
        ),
        true,
    ));
    assert!(solver.add(
        &ClauseRep::prepared(
            &[neg_lit(a), neg_lit(b)],
            ClauseInfo::new(ConstraintType::Static),
        ),
        true,
    ));
    assert!(ctx.end_init());

    let solver = ctx.master();
    let mut learnt = LitVec::new();
    learnt.assign_from_slice(&[pos_lit(a), pos_lit(b), pos_lit(c), pos_lit(d)]);
    let local = ClauseCreator::create(
        solver,
        &mut learnt,
        0,
        ClauseInfo::new(ConstraintType::Conflict),
    )
    .local;
    assert!(!local.is_null());
    let learnt_bytes = solver.learnt_bytes();
    assert!(learnt_bytes > 0);

    let mut limits = SearchLimits {
        memory: learnt_bytes.saturating_sub(1),
        learnts: u32::MAX,
        ..SearchLimits::default()
    };

    assert_eq!(solver.search_with_limits(&mut limits, 0.0), value_free);
    assert_eq!(limits.used, 1);
    assert!(solver.is_true(neg_lit(a)));
}

#[test]
fn solver_reduce_learnts_keeps_locked_and_glue_constraints() {
    let mut solver = Solver::new();
    solver.set_num_vars(1);
    solver.set_num_problem_vars(1);

    let locked_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let glue_trace = Rc::new(RefCell::new(LearntTrace::default()));
    let remove_trace = Rc::new(RefCell::new(LearntTrace::default()));

    let locked = learnt_constraint(Rc::clone(&locked_trace), true, 0, 6);
    let glue = learnt_constraint(Rc::clone(&glue_trace), false, 1, 1);
    let removed = learnt_constraint(Rc::clone(&remove_trace), false, 0, 7);

    solver.add_learnt_constraint(locked, 3, ConstraintType::Conflict);
    solver.add_learnt_constraint(glue, 2, ConstraintType::Conflict);
    solver.add_learnt_constraint(removed, 4, ConstraintType::Conflict);

    let strategy = ReduceStrategy {
        glue: 1,
        ..ReduceStrategy::default()
    };
    let reduced = solver.reduce_learnts(0.5, &strategy);

    assert_eq!(reduced.size, 2);
    assert_eq!(reduced.locked, 1);
    assert_eq!(reduced.pinned, 1);
    assert_eq!(solver.num_learnt_constraints(), 2);
    assert_eq!(locked_trace.borrow().decreases, 1);
    assert_eq!(glue_trace.borrow().decreases, 1);
    assert_eq!(remove_trace.borrow().destroyed, 1);

    solver.remove_constraint(locked);
    solver.remove_constraint(glue);
    unsafe {
        Constraint::destroy_raw(locked, Some(&mut solver), true);
        Constraint::destroy_raw(glue, Some(&mut solver), true);
    }
}

#[test]
fn solver_copy_guiding_path_tracks_root_decisions_incrementally() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    assert!(solver.assume(a) && solver.propagate());
    assert!(solver.assume(b) && solver.propagate());
    assert!(solver.assume(c) && solver.propagate());
    assert!(solver.assume(d) && solver.propagate());

    let mut gp = LitVec::new();

    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);
    gp.push_back(!a);
    assert_eq!(gp.len(), 1);
    assert_eq!(gp[0], !a);
    assert_eq!(solver.root_level(), 1);

    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);
    gp.push_back(!b);
    assert_eq!(gp.len(), 2);
    assert_eq!(gp[1], !b);
    assert_eq!(solver.root_level(), 2);

    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);
    gp.push_back(!c);
    assert_eq!(gp.len(), 3);
    assert_eq!(gp[2], !c);
    assert_eq!(solver.root_level(), 3);
}

#[test]
fn solver_copy_guiding_path_includes_flipped_decisions_from_backtrack() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let mut gp = LitVec::new();

    assert!(solver.assume(a) && solver.propagate());
    solver.push_root_level(1);
    assert!(solver.assume(b) && solver.propagate());
    assert!(solver.backtrack_step());

    assert!(solver.assume(c) && solver.propagate());
    assert!(solver.backtrack_step());

    assert!(solver.assume(d) && solver.propagate());
    solver.copy_guiding_path(&mut gp);

    assert!(contains(gp.as_slice(), &!b));
    assert!(contains(gp.as_slice(), &!c));
}

#[test]
fn solver_copy_guiding_path_keeps_flipped_literal_when_promoted_to_root() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let mut gp = LitVec::new();

    assert!(solver.assume(a) && solver.propagate());
    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);

    assert!(solver.assume(b) && solver.propagate());
    assert!(solver.assume(c) && solver.propagate());
    assert!(solver.backtrack_step());

    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);
    assert_eq!(solver.root_level(), solver.backtrack_level());

    assert!(solver.assume(d) && solver.propagate());
    solver.copy_guiding_path(&mut gp);
    solver.push_root_level(1);

    assert!(contains(gp.as_slice(), &!c));
}

#[test]
fn solver_copy_guiding_path_includes_logically_earlier_implied_literals() {
    let mut ctx = SharedContext::default();
    let a = pos_lit(ctx.add_var());
    let b = pos_lit(ctx.add_var());
    let c = pos_lit(ctx.add_var());
    let d = pos_lit(ctx.add_var());
    let e = pos_lit(ctx.add_var());
    let f = pos_lit(ctx.add_var());

    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master();
    let implied = OwnedConstraint::new();
    let mut gp = LitVec::new();

    assert!(solver.assume(a) && solver.propagate());
    assert!(solver.assume(b) && solver.propagate());
    solver.push_root_level(2);

    assert!(solver.assume(c));
    solver.set_backtrack_level(solver.decision_level());
    assert!(solver.force_at_level(!d, 2, Antecedent::from_constraint_ptr(implied.ptr())));

    solver.copy_guiding_path(&mut gp);
    assert!(contains(gp.as_slice(), &!d));

    solver.push_root_level(1);
    assert!(solver.assume(e));
    solver.set_backtrack_level(solver.decision_level());
    assert!(solver.force_at_level(!f, 2, Antecedent::from_constraint_ptr(implied.ptr())));

    solver.copy_guiding_path(&mut gp);
    assert!(contains(gp.as_slice(), &!f));
}
