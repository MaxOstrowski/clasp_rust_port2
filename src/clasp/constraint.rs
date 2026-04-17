//! Rust port of `original_clasp/clasp/constraint.h` and `original_clasp/src/constraint.cpp`.

use core::cmp::Ordering;
use core::ptr::NonNull;

use crate::clasp::literal::{LitVec, Literal, ValT, value_free, value_true};
use crate::potassco::bits::{
    BitIndex, Bitset, nth_bit, right_most_bit, store_clear_bit, store_clear_mask, store_set_mask,
    store_toggle_bit, test_any, test_bit,
};

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConstraintType {
    #[default]
    Static = 0,
    Conflict = 1,
    Loop = 2,
    Other = 3,
}

impl ConstraintType {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Static),
            1 => Some(Self::Conflict),
            2 => Some(Self::Loop),
            3 => Some(Self::Other),
            _ => None,
        }
    }
}

impl BitIndex for ConstraintType {
    fn bit_index(self) -> u32 {
        self as u32
    }
}

pub type TypeSet = Bitset<u32, ConstraintType>;

#[derive(Debug, Default)]
pub struct ClauseHead;

#[derive(Debug, Default)]
pub struct CCMinRecursive;

#[derive(Debug)]
pub struct Solver {
    id: u32,
    num_vars: u32,
    num_problem_vars: u32,
    has_conflict: bool,
    decision_level: u32,
    root_level: u32,
    values: Vec<ValT>,
    levels: Vec<u32>,
    seen_masks: Vec<u8>,
    tag_literal: Literal,
    decisions: Vec<Literal>,
    trail: Vec<Literal>,
    level_starts: Vec<u32>,
    minimized: Vec<Literal>,
    cc_minimize_result: bool,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    pub fn new() -> Self {
        Self {
            id: 0,
            num_vars: 0,
            num_problem_vars: 0,
            has_conflict: false,
            decision_level: 0,
            root_level: 0,
            values: vec![value_true],
            levels: vec![0],
            seen_masks: vec![0],
            tag_literal: Literal::default(),
            decisions: vec![Literal::default()],
            trail: Vec::new(),
            level_starts: vec![0],
            minimized: Vec::new(),
            cc_minimize_result: true,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn set_id(&mut self, id: u32) {
        self.id = id;
    }

    pub fn num_vars(&self) -> u32 {
        self.num_vars
    }

    pub fn set_num_vars(&mut self, num_vars: u32) {
        self.num_vars = num_vars;
        self.ensure_assignment_capacity(num_vars);
    }

    pub fn num_problem_vars(&self) -> u32 {
        self.num_problem_vars
    }

    pub fn set_num_problem_vars(&mut self, num_problem_vars: u32) {
        self.num_problem_vars = num_problem_vars;
        self.ensure_assignment_capacity(num_problem_vars);
    }

    pub fn valid_var(&self, var: u32) -> bool {
        var != 0 && var <= self.num_vars
    }

    pub fn value(&self, var: u32) -> ValT {
        self.values.get(var as usize).copied().unwrap_or(value_free)
    }

    pub fn set_value(&mut self, var: u32, value: ValT, level: u32) {
        self.ensure_assignment_capacity(var);
        self.values[var as usize] = value;
        self.levels[var as usize] = level;
    }

    pub fn level(&self, var: u32) -> u32 {
        self.levels.get(var as usize).copied().unwrap_or(u32::MAX)
    }

    pub fn seen_var(&self, var: u32) -> bool {
        self.root_seen_mask(var) != 0
            || self.seen_masks.get(var as usize).copied().unwrap_or(0) != 0
    }

    pub fn seen_literal(&self, literal: Literal) -> bool {
        let bit = Self::seen_bit(literal);
        (self.root_seen_mask(literal.var()) & bit) != 0
            || (self
                .seen_masks
                .get(literal.var() as usize)
                .copied()
                .unwrap_or(0)
                & bit)
                != 0
    }

    pub fn mark_seen_var(&mut self, var: u32) {
        self.ensure_assignment_capacity(var);
        self.seen_masks[var as usize] = 0b11;
    }

    pub fn mark_seen_literal(&mut self, literal: Literal) {
        self.ensure_assignment_capacity(literal.var());
        self.seen_masks[literal.var() as usize] |= Self::seen_bit(literal);
    }

    pub fn clear_seen_var(&mut self, var: u32) {
        self.ensure_assignment_capacity(var);
        self.seen_masks[var as usize] = 0;
    }

    pub fn has_conflict(&self) -> bool {
        self.has_conflict
    }

    pub fn set_has_conflict(&mut self, has_conflict: bool) {
        self.has_conflict = has_conflict;
    }

    pub fn decision_level(&self) -> u32 {
        self.decision_level
    }

    pub fn set_decision_level(&mut self, decision_level: u32) {
        self.decision_level = decision_level;
        if self.decisions.len() <= decision_level as usize {
            self.decisions
                .resize(decision_level as usize + 1, Literal::default());
        }
        if self.level_starts.len() <= decision_level as usize {
            self.level_starts.resize(decision_level as usize + 1, 0);
        }
    }

    pub fn root_level(&self) -> u32 {
        self.root_level
    }

    pub fn set_root_level(&mut self, root_level: u32) {
        self.root_level = root_level;
    }

    pub fn tag_literal(&self) -> Literal {
        self.tag_literal
    }

    pub fn set_tag_literal(&mut self, literal: Literal) {
        self.tag_literal = literal;
    }

    pub fn aux_var(&self, var: u32) -> bool {
        var > self.num_problem_vars
    }

    pub fn acquire_problem_var(&mut self, var: u32) {
        if var == 0 {
            return;
        }
        if self.num_vars < var {
            self.set_num_vars(var);
        }
        if self.num_problem_vars < var {
            self.set_num_problem_vars(var);
        }
    }

    pub fn decision(&self, level: u32) -> Literal {
        self.decisions[level as usize]
    }

    pub fn set_decision(&mut self, level: u32, literal: Literal) {
        if self.decisions.len() <= level as usize {
            self.decisions
                .resize(level as usize + 1, Literal::default());
        }
        self.decisions[level as usize] = literal;
    }

    pub fn num_assigned_vars(&self) -> u32 {
        self.trail.len() as u32
    }

    pub fn push_trail_literal(&mut self, literal: Literal) {
        self.trail.push(literal);
    }

    pub fn clear_trail(&mut self) {
        self.trail.clear();
    }

    pub fn trail_lit(&self, index: u32) -> Literal {
        self.trail[index as usize]
    }

    pub fn level_start(&self, level: u32) -> u32 {
        self.level_starts[level as usize]
    }

    pub fn set_level_start(&mut self, level: u32, start: u32) {
        if self.level_starts.len() <= level as usize {
            self.level_starts.resize(level as usize + 1, 0);
        }
        self.level_starts[level as usize] = start;
    }

    pub fn cc_minimize(&mut self, lit: Literal, _rec: *mut CCMinRecursive) -> bool {
        self.minimized.push(lit);
        self.cc_minimize_result
    }

    pub fn minimized_literals(&self) -> &[Literal] {
        &self.minimized
    }

    pub fn clear_minimized_literals(&mut self) {
        self.minimized.clear();
    }

    pub fn set_cc_minimize_result(&mut self, result: bool) {
        self.cc_minimize_result = result;
    }

    fn ensure_assignment_capacity(&mut self, var: u32) {
        let len = var as usize + 1;
        if self.values.len() < len {
            self.values.resize(len, value_free);
        }
        if self.levels.len() < len {
            self.levels.resize(len, u32::MAX);
        }
        if self.seen_masks.len() < len {
            self.seen_masks.resize(len, 0);
        }
    }

    fn seen_bit(literal: Literal) -> u8 {
        if literal.sign() { 0b10 } else { 0b01 }
    }

    fn root_seen_mask(&self, var: u32) -> u8 {
        if self.value(var) == value_free || self.level(var) != 0 {
            0
        } else if self.value(var) == crate::clasp::literal::value_true {
            0b01
        } else {
            0b10
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropResult {
    pub ok: bool,
    pub keep_watch: bool,
}

impl PropResult {
    pub const fn new(ok: bool, keep_watch: bool) -> Self {
        Self { ok, keep_watch }
    }
}

impl Default for PropResult {
    fn default() -> Self {
        Self::new(true, true)
    }
}

pub trait ConstraintDyn {
    fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult;
    fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec);
    fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>>;

    fn undo_level(&mut self, _s: &mut Solver) {}

    fn simplify(&mut self, _s: &mut Solver, _reinit: bool) -> bool {
        false
    }

    fn destroy(&mut self, _s: Option<&mut Solver>, _detach: bool) {}

    fn valid(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn minimize(&mut self, s: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        let mut temp = LitVec::default();
        self.reason(s, p, &mut temp);
        for lit in temp.as_slice() {
            if !s.cc_minimize(*lit, rec) {
                return false;
            }
        }
        true
    }

    fn estimate_complexity(&self, _s: &Solver) -> u32 {
        1
    }

    fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        None
    }

    fn constraint_type(&self) -> ConstraintType {
        ConstraintType::Static
    }

    fn locked(&self, _s: &Solver) -> bool {
        true
    }

    fn activity(&self) -> ConstraintScore {
        ConstraintScore::default()
    }

    fn decrease_activity(&mut self) {}

    fn reset_activity(&mut self) {}

    fn is_open(&mut self, _s: &Solver, _types: &TypeSet, _free_lits: &mut LitVec) -> u32 {
        0
    }
}

pub struct Constraint {
    inner: Box<dyn ConstraintDyn>,
}

impl core::fmt::Debug for Constraint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Constraint").finish_non_exhaustive()
    }
}

impl Constraint {
    pub fn new<T>(inner: T) -> Self
    where
        T: ConstraintDyn + 'static,
    {
        Self {
            inner: Box::new(inner),
        }
    }

    pub fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult {
        self.inner.propagate(s, p, data)
    }

    pub fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec) {
        self.inner.reason(s, p, lits);
    }

    pub fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>> {
        self.inner.clone_attach(other)
    }

    pub fn undo_level(&mut self, s: &mut Solver) {
        self.inner.undo_level(s);
    }

    pub fn simplify(&mut self, s: &mut Solver, reinit: bool) -> bool {
        self.inner.simplify(s, reinit)
    }

    #[allow(clippy::boxed_local)]
    pub fn destroy(mut self: Box<Self>, s: Option<&mut Solver>, detach: bool) {
        self.inner.destroy(s, detach);
    }

    /// # Safety
    ///
    /// `ptr` must either be null or have been created by `Box::into_raw(Box<Constraint>)`
    /// and not have been previously passed to `destroy_raw` or reconstructed by `Box::from_raw`.
    pub unsafe fn destroy_raw(ptr: *mut Self, s: Option<&mut Solver>, detach: bool) {
        if !ptr.is_null() {
            unsafe { Box::from_raw(ptr) }.destroy(s, detach);
        }
    }

    pub fn valid(&mut self, s: &mut Solver) -> bool {
        self.inner.valid(s)
    }

    pub fn minimize(&mut self, s: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        self.inner.minimize(s, p, rec)
    }

    pub fn estimate_complexity(&self, s: &Solver) -> u32 {
        self.inner.estimate_complexity(s)
    }

    pub fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        self.inner.clause()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        self.inner.constraint_type()
    }

    pub fn locked(&self, s: &Solver) -> bool {
        self.inner.locked(s)
    }

    pub fn activity(&self) -> ConstraintScore {
        self.inner.activity()
    }

    pub fn decrease_activity(&mut self) {
        self.inner.decrease_activity();
    }

    pub fn reset_activity(&mut self) {
        self.inner.reset_activity();
    }

    pub fn is_open(&mut self, s: &Solver, types: &TypeSet, free_lits: &mut LitVec) -> u32 {
        self.inner.is_open(s, types, free_lits)
    }
}

#[allow(non_upper_case_globals)]
pub const priority_class_simple: u32 = 0;
#[allow(non_upper_case_globals)]
pub const priority_reserved_msg: u32 = 0;
#[allow(non_upper_case_globals)]
pub const priority_reserved_ufs: u32 = 10;
#[allow(non_upper_case_globals)]
pub const priority_reserved_look: u32 = 1023;
#[allow(non_upper_case_globals)]
pub const priority_class_general: u32 = 1024;

pub trait PostPropagatorDyn {
    fn priority(&self) -> u32;
    fn propagate_fixpoint(&mut self, s: &mut Solver, ctx: Option<NonNull<PostPropagator>>) -> bool;

    fn init(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn simplify(&mut self, _s: &mut Solver, _reinit: bool) -> bool {
        false
    }

    fn valid(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn reset(&mut self) {}

    fn is_model(&mut self, s: &mut Solver) -> bool {
        self.valid(s)
    }

    fn reason(&mut self, _s: &mut Solver, _p: Literal, _lits: &mut LitVec) {}

    fn propagate(&mut self, _s: &mut Solver, _p: Literal, _data: &mut u32) -> PropResult {
        PropResult::new(true, false)
    }

    fn destroy(&mut self, _s: Option<&mut Solver>, _detach: bool) {}
}

pub trait MessageHandlerDyn {
    fn handle_messages(&mut self) -> bool;
}

struct MessageHandlerAdapter<T> {
    inner: T,
}

impl<T> PostPropagatorDyn for MessageHandlerAdapter<T>
where
    T: MessageHandlerDyn,
{
    fn priority(&self) -> u32 {
        priority_reserved_msg
    }

    fn propagate_fixpoint(
        &mut self,
        _s: &mut Solver,
        _ctx: Option<NonNull<PostPropagator>>,
    ) -> bool {
        self.inner.handle_messages()
    }
}

pub struct PostPropagator {
    inner: Box<dyn PostPropagatorDyn>,
    pub next: Option<NonNull<PostPropagator>>,
}

impl core::fmt::Debug for PostPropagator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PostPropagator")
            .field("priority", &self.priority())
            .finish_non_exhaustive()
    }
}

impl PostPropagator {
    pub fn new<T>(inner: T) -> Self
    where
        T: PostPropagatorDyn + 'static,
    {
        Self {
            inner: Box::new(inner),
            next: None,
        }
    }

    pub fn from_message_handler<T>(inner: T) -> Self
    where
        T: MessageHandlerDyn + 'static,
    {
        Self::new(MessageHandlerAdapter { inner })
    }

    pub fn priority(&self) -> u32 {
        self.inner.priority()
    }

    pub fn init(&mut self, s: &mut Solver) -> bool {
        self.inner.init(s)
    }

    pub fn propagate_fixpoint(
        &mut self,
        s: &mut Solver,
        ctx: Option<NonNull<PostPropagator>>,
    ) -> bool {
        self.inner.propagate_fixpoint(s, ctx)
    }

    pub fn simplify(&mut self, s: &mut Solver, reinit: bool) -> bool {
        self.inner.simplify(s, reinit)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn is_model(&mut self, s: &mut Solver) -> bool {
        self.inner.is_model(s)
    }

    pub fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec) {
        self.inner.reason(s, p, lits);
    }

    pub fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult {
        self.inner.propagate(s, p, data)
    }

    pub fn cancel_propagation(&mut self) {
        let mut current = self.next;
        while let Some(mut ptr) = current {
            unsafe {
                ptr.as_mut().reset();
                current = ptr.as_ref().next;
            }
        }
    }

    #[allow(clippy::boxed_local)]
    pub fn destroy(mut self: Box<Self>, s: Option<&mut Solver>, detach: bool) {
        self.inner.destroy(s, detach);
    }

    /// # Safety
    ///
    /// `ptr` must either be null or have been created by `Box::into_raw(Box<PostPropagator>)`
    /// and not have been previously passed to `destroy_raw` or reconstructed by `Box::from_raw`.
    pub unsafe fn destroy_raw(ptr: *mut Self, s: Option<&mut Solver>, detach: bool) {
        if !ptr.is_null() {
            unsafe { Box::from_raw(ptr) }.destroy(s, detach);
        }
    }
}

#[derive(Debug, Default)]
pub struct PropagatorList {
    head: Option<NonNull<PostPropagator>>,
}

impl PropagatorList {
    pub fn new() -> Self {
        Self { head: None }
    }

    pub fn head(&self) -> Option<NonNull<PostPropagator>> {
        self.head
    }

    pub fn add(&mut self, propagator: Box<PostPropagator>) {
        assert!(propagator.next.is_none(), "Invalid post propagator");

        let priority = propagator.priority();
        let leaked = Box::leak(propagator);
        let new_ptr = NonNull::from(&mut *leaked);

        unsafe {
            let mut link = &mut self.head;
            while let Some(mut current) = *link {
                if priority < current.as_ref().priority() {
                    break;
                }
                link = &mut current.as_mut().next;
            }
            leaked.next = *link;
            *link = Some(new_ptr);
        }
    }

    pub fn remove(&mut self, target: NonNull<PostPropagator>) -> Option<Box<PostPropagator>> {
        unsafe {
            let mut link = &mut self.head;
            while let Some(mut current) = *link {
                if current == target {
                    *link = current.as_ref().next;
                    current.as_mut().next = None;
                    return Some(Box::from_raw(current.as_ptr()));
                }
                link = &mut current.as_mut().next;
            }
        }
        None
    }

    pub fn clear(&mut self) {
        let mut current = self.head.take();
        while let Some(ptr) = current {
            unsafe {
                current = ptr.as_ref().next;
                PostPropagator::destroy_raw(ptr.as_ptr(), None, false);
            }
        }
    }

    pub fn find_by<P>(&self, mut pred: P, prio: Option<u32>) -> Option<NonNull<PostPropagator>>
    where
        P: FnMut(&PostPropagator) -> bool,
    {
        let mut current = self.head;
        while let Some(ptr) = current {
            let propagator = unsafe { ptr.as_ref() };
            if let Some(target_prio) = prio {
                match propagator.priority().cmp(&target_prio) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        if pred(propagator) {
                            return Some(ptr);
                        }
                    }
                    Ordering::Greater => break,
                }
            } else if pred(propagator) {
                return Some(ptr);
            }
            current = propagator.next;
        }
        None
    }

    pub fn find(&self, prio: u32) -> Option<NonNull<PostPropagator>> {
        self.find_by(|_| true, Some(prio))
    }

    pub fn init(&mut self, solver: &mut Solver) -> bool {
        let mut current = self.head;
        while let Some(mut ptr) = current {
            unsafe {
                if !ptr.as_mut().init(solver) {
                    return false;
                }
                current = ptr.as_ref().next;
            }
        }
        true
    }

    pub fn simplify(&mut self, solver: &mut Solver, reinit: bool) -> bool {
        unsafe {
            let mut link = &mut self.head;
            while let Some(mut ptr) = *link {
                if ptr.as_mut().simplify(solver, reinit) {
                    *link = ptr.as_ref().next;
                    PostPropagator::destroy_raw(ptr.as_ptr(), Some(solver), false);
                } else {
                    link = &mut ptr.as_mut().next;
                }
            }
        }
        false
    }

    pub fn is_model(&mut self, solver: &mut Solver) -> bool {
        let mut current = self.head;
        while let Some(mut ptr) = current {
            unsafe {
                if !ptr.as_mut().is_model(solver) {
                    return false;
                }
                current = ptr.as_ref().next;
            }
        }
        true
    }
}

impl Drop for PropagatorList {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Antecedent {
    data: u64,
}

impl Antecedent {
    pub const GENERIC: u64 = 0;
    pub const TERNARY: u64 = 1;
    pub const BINARY: u64 = 2;

    pub const fn new() -> Self {
        Self { data: 0 }
    }

    pub const fn from_literal(p: Literal) -> Self {
        Self {
            data: ((p.id() as u64) << 33) + Self::BINARY,
        }
    }

    pub const fn from_literals(p: Literal, q: Literal) -> Self {
        Self {
            data: ((p.id() as u64) << 33) + ((q.id() as u64) << 2) + Self::TERNARY,
        }
    }

    pub fn from_constraint_ptr(con: *mut Constraint) -> Self {
        Self {
            data: con as usize as u64,
        }
    }

    pub const fn is_null(&self) -> bool {
        self.data == 0
    }

    pub const fn type_(&self) -> u64 {
        self.data & 3
    }

    pub fn learnt(&self) -> bool {
        right_most_bit(self.data) > Self::BINARY
            && self.constraint().constraint_type() != ConstraintType::Static
    }

    pub fn constraint(&self) -> &Constraint {
        assert_eq!(self.type_(), Self::GENERIC);
        unsafe { &*(self.data as usize as *const Constraint) }
    }

    pub fn constraint_mut(&mut self) -> &mut Constraint {
        assert_eq!(self.type_(), Self::GENERIC);
        unsafe { &mut *(self.data as usize as *mut Constraint) }
    }

    pub const fn first_literal(&self) -> Literal {
        assert!(self.type_() != Self::GENERIC);
        Literal::from_id((self.data >> 33) as u32)
    }

    pub const fn second_literal(&self) -> Literal {
        assert!(self.type_() == Self::TERNARY);
        Literal::from_id(((self.data >> 1) as u32) >> 1)
    }

    pub fn reason(&mut self, solver: &mut Solver, p: Literal, lits: &mut LitVec) {
        assert!(!self.is_null());
        match self.type_() {
            Self::GENERIC => self.constraint_mut().reason(solver, p, lits),
            Self::BINARY => lits.push_back(self.first_literal()),
            Self::TERNARY => {
                lits.push_back(self.first_literal());
                lits.push_back(self.second_literal());
            }
            _ => unreachable!(),
        }
    }

    pub fn minimize(&mut self, solver: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        assert!(!self.is_null());
        match self.type_() {
            Self::GENERIC => self.constraint_mut().minimize(solver, p, rec),
            Self::BINARY => solver.cc_minimize(self.first_literal(), rec),
            Self::TERNARY => {
                solver.cc_minimize(self.first_literal(), rec)
                    && solver.cc_minimize(self.second_literal(), rec)
            }
            _ => unreachable!(),
        }
    }

    pub const fn as_u64(&self) -> u64 {
        self.data
    }

    pub fn as_u64_mut(&mut self) -> &mut u64 {
        &mut self.data
    }
}

impl From<Literal> for Antecedent {
    fn from(value: Literal) -> Self {
        Self::from_literal(value)
    }
}

impl From<(Literal, Literal)> for Antecedent {
    fn from(value: (Literal, Literal)) -> Self {
        Self::from_literals(value.0, value.1)
    }
}

impl PartialEq<*const Constraint> for Antecedent {
    fn eq(&self, other: &*const Constraint) -> bool {
        self.data as usize == *other as usize
    }
}

#[allow(non_upper_case_globals)]
pub const lbd_max: u32 = 127;
#[allow(non_upper_case_globals)]
pub const act_max: u32 = (1 << 20) - 1;

#[derive(Clone, Copy, Debug, Default)]
pub struct ConstraintScore {
    rep: u32,
}

#[allow(non_upper_case_globals)]
impl ConstraintScore {
    pub const bits_used: u32 = 28;
    pub const bumped_bit: u32 = 27;
    pub const lbd_shift: u32 = 20;
    pub const lbd_mask: u32 = lbd_max << Self::lbd_shift;
    pub const score_mask: u32 = (1u32 << Self::bits_used) - 1;

    pub const fn new(act: u32, lbd: u32) -> Self {
        Self {
            rep: (if lbd < lbd_max { lbd } else { lbd_max }) << Self::lbd_shift
                | if act < act_max { act } else { act_max },
        }
    }

    const fn from_rep(rep: u32) -> Self {
        Self { rep }
    }

    pub fn reset(&mut self, act: u32, lbd: u32) {
        self.assign(Self::new(act, lbd));
    }

    pub const fn activity(&self) -> u32 {
        self.rep & act_max
    }

    pub fn lbd(&self) -> u32 {
        if self.has_lbd() {
            (self.rep & Self::lbd_mask) >> Self::lbd_shift
        } else {
            lbd_max
        }
    }

    pub fn has_lbd(&self) -> bool {
        test_any(self.rep, Self::lbd_mask)
    }

    pub fn bumped(&self) -> bool {
        test_bit(self.rep, Self::bumped_bit)
    }

    pub fn bump_activity(&mut self) {
        self.rep += u32::from(self.activity() < act_max);
    }

    pub fn bump_lbd(&mut self, x: u32) {
        if x < self.lbd() {
            store_clear_mask(&mut self.rep, Self::lbd_mask);
            store_set_mask(
                &mut self.rep,
                (x << Self::lbd_shift) | nth_bit::<u32>(Self::bumped_bit),
            );
        }
    }

    pub fn clear_bumped(&mut self) {
        store_clear_bit(&mut self.rep, Self::bumped_bit);
    }

    pub fn reduce(&mut self) {
        self.clear_bumped();
        let activity = self.activity();
        if activity != 0 {
            store_clear_mask(&mut self.rep, act_max);
            store_set_mask(&mut self.rep, activity >> 1);
        }
    }

    pub fn assign(&mut self, other: Self) {
        store_clear_mask(&mut self.rep, Self::score_mask);
        store_set_mask(&mut self.rep, other.rep & Self::score_mask);
    }
}

impl PartialEq for ConstraintScore {
    fn eq(&self, other: &Self) -> bool {
        (self.rep & Self::score_mask) == (other.rep & Self::score_mask)
    }
}

impl Eq for ConstraintScore {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConstraintInfo {
    rep: u32,
}

#[allow(non_upper_case_globals)]
impl ConstraintInfo {
    const tag_bit: u32 = 31;
    const aux_bit: u32 = 30;
    const type_shift: u32 = 28;
    const type_mask: u32 = 3 << Self::type_shift;

    pub fn new(constraint_type: ConstraintType) -> Self {
        Self {
            rep: constraint_type.as_u32() << Self::type_shift,
        }
    }

    pub fn activity(&self) -> u32 {
        self.score().activity()
    }

    pub fn lbd(&self) -> u32 {
        self.score().lbd()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        ConstraintType::from_u32((self.rep & Self::type_mask) >> Self::type_shift)
            .expect("invalid ConstraintInfo type bits")
    }

    pub fn tagged(&self) -> bool {
        test_bit(self.rep, Self::tag_bit)
    }

    pub fn aux(&self) -> bool {
        self.tagged() || test_bit(self.rep, Self::aux_bit)
    }

    pub fn learnt(&self) -> bool {
        self.constraint_type() != ConstraintType::Static
    }

    pub fn score(&self) -> ConstraintScore {
        ConstraintScore::from_rep(self.rep & ConstraintScore::score_mask)
    }

    pub fn set_type(&mut self, constraint_type: ConstraintType) -> &mut Self {
        store_clear_mask(&mut self.rep, Self::type_mask);
        store_set_mask(&mut self.rep, constraint_type.as_u32() << Self::type_shift);
        self
    }

    pub fn set_score(&mut self, score: ConstraintScore) -> &mut Self {
        store_clear_mask(&mut self.rep, ConstraintScore::score_mask);
        store_set_mask(&mut self.rep, score.rep & ConstraintScore::score_mask);
        self
    }

    pub fn set_activity(&mut self, activity: u32) -> &mut Self {
        self.set_score(ConstraintScore::new(activity, self.lbd()))
    }

    pub fn set_lbd(&mut self, lbd: u32) -> &mut Self {
        self.set_score(ConstraintScore::new(self.activity(), lbd))
    }

    pub fn set_tagged(&mut self, value: bool) -> &mut Self {
        self.set_bit::<{ Self::tag_bit }>(value)
    }

    pub fn set_aux(&mut self, value: bool) -> &mut Self {
        self.set_bit::<{ Self::aux_bit }>(value)
    }

    fn set_bit<const BIT: u32>(&mut self, value: bool) -> &mut Self {
        if test_bit(self.rep, BIT) != value {
            store_toggle_bit(&mut self.rep, BIT);
        }
        self
    }
}
