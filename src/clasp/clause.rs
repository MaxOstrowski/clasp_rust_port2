//! Partial Rust port of `original_clasp/clasp/clause.h` and `original_clasp/src/clause.cpp`.
//!
//! This module now covers the Bundle A explicit clause runtime together with
//! `ClauseCreator`'s creation and integration paths on top of the current
//! solver/shared-context kernel. Shared clauses, clause contraction, and the
//! original small-clause allocator remain simplified.

use core::cmp::Reverse;
use core::ffi::c_void;
use core::mem::size_of;
use core::ops::Index;
use core::ptr::{self, NonNull};

pub use crate::clasp::constraint::{ClauseHead, ClauseStrengthenResult};

use crate::clasp::constraint::{
    Antecedent, ClauseOwnerKind, Constraint, ConstraintDyn, ConstraintInfo, ConstraintScore,
    ConstraintType, PropResult, Solver,
};
use crate::clasp::literal::{
    LitVec, LitView, Literal, is_sentinel, lit_false, lit_true, true_value, value_free, var_max,
};
use crate::clasp::pod_vector::{shrink_vec_to, size32};
use crate::clasp::solver_strategies::WatchInit;
use crate::clasp::util::misc_types::RefCount;

pub type ClauseInfo = ConstraintInfo;
pub type CreateFlag = u32;

pub const CLAUSE_FLAG_NONE: CreateFlag = 0;
pub const CLAUSE_NO_ADD: CreateFlag = 1;
pub const CLAUSE_EXPLICIT: CreateFlag = 2;
pub const CLAUSE_NOT_SAT: CreateFlag = 4;
pub const CLAUSE_NOT_ROOT_SAT: CreateFlag = 8;
pub const CLAUSE_NOT_CONFLICT: CreateFlag = 16;
pub const CLAUSE_NO_RELEASE: CreateFlag = 32;
pub const CLAUSE_INT_LBD: CreateFlag = 64;
pub const CLAUSE_NO_PREPARE: CreateFlag = 128;
pub const CLAUSE_FORCE_SIMPLIFY: CreateFlag = 256;
pub const CLAUSE_NO_HEURISTIC: CreateFlag = 512;
pub const CLAUSE_WATCH_FIRST: CreateFlag = 1024;
pub const CLAUSE_WATCH_RAND: CreateFlag = 2048;
pub const CLAUSE_WATCH_LEAST: CreateFlag = 4096;

const EXPLICIT_CLAUSE_HEAD_LITS: usize = 3;
const EXPLICIT_CLAUSE_MAX_SHORT_LEN: u32 = 5;
const SMALL_CLAUSE_ALLOC_SIZE: u32 = 32;

fn should_physically_share(solver: &Solver, constraint_type: ConstraintType) -> bool {
    solver
        .shared_context()
        .is_some_and(|shared| shared.physical_share(constraint_type))
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClauseStatus {
    Open = 0,
    Sat = 1,
    Unsat = 2,
    Unit = 4,
    SatAsserting = 5,
    Asserting = 6,
    Subsumed = 9,
    Empty = 10,
}

impl ClauseStatus {
    pub const fn bits(self) -> u32 {
        self as u32
    }

    pub const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Open,
            1 => Self::Sat,
            2 => Self::Unsat,
            4 => Self::Unit,
            5 => Self::SatAsserting,
            6 => Self::Asserting,
            9 => Self::Subsumed,
            10 => Self::Empty,
            _ => panic!("invalid ClauseStatus bits"),
        }
    }

    pub const fn ok(self) -> bool {
        (self.bits() & ClauseStatus::Unsat.bits()) == 0
    }

    pub const fn unit(self) -> bool {
        (self.bits() & ClauseStatus::Unit.bits()) != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClauseCreationResult {
    pub local: *mut ClauseHead,
    pub status: ClauseStatus,
}

impl Default for ClauseCreationResult {
    fn default() -> Self {
        Self::new(ptr::null_mut(), ClauseStatus::Open)
    }
}

impl ClauseCreationResult {
    pub const fn new(local: *mut ClauseHead, status: ClauseStatus) -> Self {
        Self { local, status }
    }

    pub const fn ok(self) -> bool {
        self.status.ok()
    }

    pub const fn unit(self) -> bool {
        self.status.unit()
    }
}

#[derive(Debug)]
pub struct SharedLiterals {
    ref_count: RefCount,
    size_type: u32,
    literals: LitVec,
}

impl SharedLiterals {
    pub fn new(lits: LitView<'_>, constraint_type: ConstraintType) -> Self {
        Self::new_shareable(lits, constraint_type, 1)
    }

    pub fn new_shareable(
        lits: LitView<'_>,
        constraint_type: ConstraintType,
        num_refs: u32,
    ) -> Self {
        let mut literals = LitVec::new();
        literals.assign_from_slice(lits);
        Self {
            ref_count: RefCount::new(num_refs.max(1)),
            size_type: (size32(lits) << 2) | constraint_type.as_u32(),
            literals,
        }
    }

    pub fn literals(&self) -> &[Literal] {
        &self.literals.as_slice()[..self.size() as usize]
    }

    pub fn begin(&self) -> *const Literal {
        self.literals().as_ptr_range().start
    }

    pub fn end(&self) -> *const Literal {
        self.literals().as_ptr_range().end
    }

    pub fn size(&self) -> u32 {
        self.size_type >> 2
    }

    pub fn constraint_type(&self) -> ConstraintType {
        ConstraintType::from_u32(self.size_type & 3).expect("invalid SharedLiterals type bits")
    }

    pub fn r#type(&self) -> ConstraintType {
        self.constraint_type()
    }

    pub fn simplify(&mut self, solver: &Solver) -> u32 {
        let false_inc = u32::from(!self.unique()) as usize;
        let mut new_size = 0usize;
        let old_size = self.size() as usize;
        let mut write = 0usize;

        for idx in 0..old_size {
            let literal = self.literals[idx];
            let value = solver.value(literal.var());
            if value == value_free {
                if self.literals[write] != literal {
                    self.literals[write] = literal;
                }
                write += 1;
                new_size += 1;
            } else if value == true_value(literal) {
                new_size = 0;
                break;
            } else {
                write += false_inc;
            }
        }

        if false_inc == 0 && new_size != old_size {
            self.size_type = ((new_size as u32) << 2) | (self.size_type & 3);
        }
        new_size as u32
    }

    pub fn release(&self, num_refs: u32) -> bool {
        num_refs > 0 && self.ref_count.release(num_refs)
    }

    pub fn release_one(&self) -> bool {
        self.release(1)
    }

    pub fn share(&self) -> &Self {
        self.ref_count.add(1);
        self
    }

    pub fn unique(&self) -> bool {
        self.ref_count() <= 1
    }

    pub fn ref_count(&self) -> u32 {
        self.ref_count.count()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClauseRep {
    pub info: ClauseInfo,
    pub size: u32,
    pub prep: bool,
    lits: LitVec,
}

impl ClauseRep {
    pub fn create(lits: LitView<'_>, info: ClauseInfo) -> Self {
        Self::new(lits, info, false)
    }

    pub fn prepared(lits: LitView<'_>, info: ClauseInfo) -> Self {
        Self::new(lits, info, true)
    }

    pub fn literals(&self) -> &[Literal] {
        &self.lits.as_slice()[..self.size as usize]
    }

    pub fn literals_mut(&mut self) -> &mut [Literal] {
        &mut self.lits.as_mut_slice()[..self.size as usize]
    }

    pub fn is_imp(&self) -> bool {
        self.size > 1 && self.size < 4
    }

    fn new(lits: LitView<'_>, info: ClauseInfo, prep: bool) -> Self {
        let mut owned = LitVec::new();
        owned.assign_from_slice(lits);
        Self {
            info,
            size: size32(lits),
            prep,
            lits: owned,
        }
    }
}

#[derive(Debug)]
struct ExplicitClauseState {
    head: ClauseHead,
    literals: LitVec,
    active_len: u32,
    retained_alloc_len: u32,
    contracted: bool,
    storage_is_small: bool,
    strengthened: bool,
    extend_on_backtrack: bool,
}

impl ExplicitClauseState {
    fn new(rep: &ClauseRep) -> Self {
        let mut literals = LitVec::new();
        literals.assign_from_slice(rep.literals());
        let mut state = Self {
            head: ClauseHead::new(rep.info),
            literals,
            active_len: rep.size,
            retained_alloc_len: rep.size,
            contracted: false,
            storage_is_small: rep.size <= EXPLICIT_CLAUSE_MAX_SHORT_LEN,
            strengthened: false,
            extend_on_backtrack: false,
        };
        state.sync_head();
        state
    }

    fn new_contracted(solver: &mut Solver, rep: &ClauseRep, tail_start: u32, extend: bool) -> Self {
        let mut state = Self::new(rep);
        if tail_start < rep.size {
            if extend {
                state.literals.as_mut_slice()[tail_start as usize..]
                    .sort_by_key(|lit| Reverse(solver.level(lit.var())));
            }
            if solver.level(state.literals[tail_start as usize].var()) > 0 {
                state.active_len = tail_start.max(2);
                state.contracted = true;
                state.extend_on_backtrack = extend;
            }
        }
        state.sync_head();
        state
    }

    fn contracted_tail(&self) -> &[Literal] {
        &self.literals.as_slice()[self.active_len as usize..]
    }

    fn full_len(&self) -> u32 {
        size32(self.literals.as_slice())
    }

    fn active_size(&self) -> u32 {
        self.active_len
    }

    fn alloc_len(&self) -> u32 {
        if self.strengthened {
            self.retained_alloc_len.max(self.full_len())
        } else {
            self.full_len()
        }
    }

    fn compute_alloc_size(&self) -> u32 {
        if self.storage_is_small {
            return SMALL_CLAUSE_ALLOC_SIZE;
        }
        explicit_clause_base_alloc_size() + self.alloc_len() * (size_of::<Literal>() as u32)
    }

    fn preserve_retained_allocation(&mut self, previous_full_len: u32) {
        if previous_full_len == self.full_len() || self.storage_is_small || !self.head.info.learnt()
        {
            return;
        }
        self.retained_alloc_len = self.retained_alloc_len.max(previous_full_len);
        self.strengthened = true;
    }

    fn remove_current_undo_watch(&self, solver: &mut Solver) -> bool {
        let Some(&literal) = self.contracted_tail().first() else {
            return false;
        };
        if !self.contracted || !solver.is_false(literal) {
            return false;
        }
        let level = solver.level(literal.var());
        self.extend_on_backtrack
            && level != 0
            && solver.remove_undo_watch(level, self.head.constraint_ptr())
    }

    fn add_current_undo_watch(&self, solver: &mut Solver) {
        let Some(&literal) = self.contracted_tail().first() else {
            return;
        };
        if self.contracted && self.extend_on_backtrack {
            let level = solver.level(literal.var());
            if level != 0 {
                solver.add_undo_watch(level, self.head.constraint_ptr());
            }
        }
    }

    fn sync_head(&mut self) {
        self.head.head = [lit_false; 3];
        let limit = self.active_len.min(3) as usize;
        for index in 0..limit {
            self.head.head[index] = self.literals[index];
        }
    }

    fn bump_activity_if_learnt(&mut self) {
        if self.head.info.learnt() {
            let mut score = self.head.info.score();
            score.bump_activity();
            self.head.info.set_score(score);
        }
    }
}

#[derive(Debug)]
struct ExplicitClause {
    state: *mut ExplicitClauseState,
}

impl ExplicitClause {
    fn new(state: *mut ExplicitClauseState) -> Self {
        Self { state }
    }

    fn state_mut(&mut self) -> &mut ExplicitClauseState {
        unsafe { &mut *self.state }
    }

    fn state_ref(&self) -> &ExplicitClauseState {
        unsafe { &*self.state }
    }
}

impl ConstraintDyn for ExplicitClause {
    fn propagate(&mut self, solver: &mut Solver, literal: Literal, _data: &mut u32) -> PropResult {
        clause_head_propagate(&mut self.state_mut().head, solver, literal)
    }

    fn reason(&mut self, _solver: &mut Solver, literal: Literal, lits: &mut LitVec) {
        let state = self.state_mut();
        state.bump_activity_if_learnt();
        for &clause_lit in state.literals.as_slice() {
            if clause_lit != literal {
                lits.push_back(!clause_lit);
            }
        }
    }

    fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>> {
        clone_clause_constraint(&self.state_ref().head, other)
    }

    fn simplify(&mut self, solver: &mut Solver, reinit: bool) -> bool {
        clause_head_simplify(&mut self.state_mut().head, solver, reinit)
    }

    fn undo_level(&mut self, solver: &mut Solver) {
        restore_explicit_clause(self.state_mut(), solver);
    }

    fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        NonNull::new(&mut self.state_mut().head)
    }

    fn constraint_type(&self) -> ConstraintType {
        self.state_ref().head.info.constraint_type()
    }

    fn locked(&self, solver: &Solver) -> bool {
        clause_head_locked(&self.state_ref().head, solver)
    }

    fn activity(&self) -> ConstraintScore {
        self.state_ref().head.info.score()
    }

    fn decrease_activity(&mut self) {
        self.state_mut().head.decrease_activity();
    }

    fn reset_activity(&mut self) {
        self.state_mut().head.reset_activity();
    }

    fn is_open(
        &mut self,
        solver: &Solver,
        types: &crate::clasp::constraint::TypeSet,
        free: &mut LitVec,
    ) -> u32 {
        let state = self.state_ref();
        let ty = state.head.info.constraint_type();
        if !types.contains(ty) || clause_head_satisfied(&state.head, solver) {
            return 0;
        }
        for &lit in state.literals.as_slice() {
            if solver.value(lit.var()) == value_free {
                free.push_back(lit);
            }
        }
        ty.as_u32()
    }
}

fn explicit_clause_base_alloc_size() -> u32 {
    (size_of::<ExplicitClauseState>() - (EXPLICIT_CLAUSE_HEAD_LITS * size_of::<Literal>())) as u32
}

fn explicit_state_from_head(head: &ClauseHead) -> &ExplicitClauseState {
    unsafe { &*(head.owner as *const ExplicitClauseState) }
}

fn explicit_state_from_head_mut(head: &mut ClauseHead) -> &mut ExplicitClauseState {
    unsafe { &mut *(head.owner as *mut ExplicitClauseState) }
}

fn build_explicit_clause(
    solver: &mut Solver,
    rep: &ClauseRep,
) -> (*mut ClauseHead, *mut Constraint) {
    let mut state = Box::new(ExplicitClauseState::new(rep));
    if rep.info.learnt() {
        solver.add_learnt_bytes(state.compute_alloc_size());
    }
    let state_ptr: *mut ExplicitClauseState = &mut *state;
    state.head.owner = state_ptr.cast::<c_void>();
    state.head.owner_kind = ClauseOwnerKind::Explicit;
    let constraint = Box::new(Constraint::new(ExplicitClause::new(state_ptr)));
    let constraint_ptr = Box::into_raw(constraint);
    state.head.constraint = constraint_ptr;
    let head_ptr: *mut ClauseHead = &mut state.head;
    let _ = Box::into_raw(state);
    (head_ptr, constraint_ptr)
}

fn build_contracted_explicit_clause(
    solver: &mut Solver,
    rep: &ClauseRep,
    tail_start: u32,
    extend: bool,
) -> (*mut ClauseHead, *mut Constraint) {
    let mut state = Box::new(ExplicitClauseState::new_contracted(
        solver, rep, tail_start, extend,
    ));
    if rep.info.learnt() {
        solver.add_learnt_bytes(state.compute_alloc_size());
    }
    let state_ptr: *mut ExplicitClauseState = &mut *state;
    state.head.owner = state_ptr.cast::<c_void>();
    state.head.owner_kind = ClauseOwnerKind::Explicit;
    let constraint = Box::new(Constraint::new(ExplicitClause::new(state_ptr)));
    let constraint_ptr = Box::into_raw(constraint);
    state.head.constraint = constraint_ptr;
    if state.contracted {
        state.add_current_undo_watch(solver);
    }
    let head_ptr: *mut ClauseHead = &mut state.head;
    let _ = Box::into_raw(state);
    (head_ptr, constraint_ptr)
}

pub fn new_contracted_clause(
    solver: &mut Solver,
    rep: &ClauseRep,
    tail_start: u32,
    extend: bool,
) -> *mut ClauseHead {
    let (head_ptr, _constraint_ptr) =
        build_contracted_explicit_clause(solver, rep, tail_start, extend);
    unsafe { (*head_ptr).attach(solver) };
    head_ptr
}

#[derive(Debug)]
struct SharedClauseState {
    head: ClauseHead,
    shared: *mut SharedLiterals,
}

impl SharedClauseState {
    fn new(shared: *mut SharedLiterals, info: ClauseInfo, watched: LitView<'_>) -> Self {
        let mut head = ClauseHead::new(info);
        head.owner_kind = ClauseOwnerKind::Shared;
        head.head = [lit_false; 3];
        for (dst, src) in head.head.iter_mut().zip(watched.iter().copied()) {
            *dst = src;
        }
        Self { head, shared }
    }

    fn bump_activity_if_learnt(&mut self) {
        if self.head.info.learnt() {
            let mut score = self.head.info.score();
            score.bump_activity();
            self.head.info.set_score(score);
        }
    }
}

#[derive(Debug)]
struct SharedClause {
    state: *mut SharedClauseState,
}

impl SharedClause {
    fn new(state: *mut SharedClauseState) -> Self {
        Self { state }
    }

    fn state_mut(&mut self) -> &mut SharedClauseState {
        unsafe { &mut *self.state }
    }

    fn state_ref(&self) -> &SharedClauseState {
        unsafe { &*self.state }
    }
}

impl ConstraintDyn for SharedClause {
    fn propagate(&mut self, solver: &mut Solver, literal: Literal, _data: &mut u32) -> PropResult {
        clause_head_propagate(&mut self.state_mut().head, solver, literal)
    }

    fn reason(&mut self, _solver: &mut Solver, literal: Literal, lits: &mut LitVec) {
        let state = self.state_mut();
        state.bump_activity_if_learnt();
        for &clause_lit in unsafe { &*state.shared }.literals() {
            if clause_lit != literal {
                lits.push_back(!clause_lit);
            }
        }
    }

    fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>> {
        clone_clause_constraint(&self.state_ref().head, other)
    }

    fn simplify(&mut self, solver: &mut Solver, reinit: bool) -> bool {
        clause_head_simplify(&mut self.state_mut().head, solver, reinit)
    }

    fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        NonNull::new(&mut self.state_mut().head)
    }

    fn constraint_type(&self) -> ConstraintType {
        self.state_ref().head.info.constraint_type()
    }

    fn locked(&self, solver: &Solver) -> bool {
        clause_head_locked(&self.state_ref().head, solver)
    }

    fn activity(&self) -> ConstraintScore {
        self.state_ref().head.info.score()
    }

    fn decrease_activity(&mut self) {
        self.state_mut().head.decrease_activity();
    }

    fn reset_activity(&mut self) {
        self.state_mut().head.reset_activity();
    }

    fn is_open(
        &mut self,
        solver: &Solver,
        types: &crate::clasp::constraint::TypeSet,
        free: &mut LitVec,
    ) -> u32 {
        let state = self.state_ref();
        let ty = state.head.info.constraint_type();
        if !types.contains(ty) || clause_head_satisfied(&state.head, solver) {
            return 0;
        }
        for &lit in unsafe { &*state.shared }.literals() {
            if solver.value(lit.var()) == value_free {
                free.push_back(lit);
            }
        }
        ty.as_u32()
    }
}

fn shared_state_from_head(head: &ClauseHead) -> &SharedClauseState {
    unsafe { &*(head.owner as *const SharedClauseState) }
}

fn shared_state_from_head_mut(head: &mut ClauseHead) -> &mut SharedClauseState {
    unsafe { &mut *(head.owner as *mut SharedClauseState) }
}

fn clone_clause_constraint(head: &ClauseHead, other: &mut Solver) -> Option<Box<Constraint>> {
    let clone = head.clone_attach(other);
    if clone.is_null() {
        return None;
    }
    let constraint = unsafe { (*clone).constraint_ptr() };
    debug_assert!(!constraint.is_null());
    Some(unsafe { Box::from_raw(constraint) })
}

fn build_shared_clause_from_ptr(
    solver: &mut Solver,
    shared: *mut SharedLiterals,
    info: ClauseInfo,
    watched: LitView<'_>,
    add_ref: bool,
) -> (*mut ClauseHead, *mut Constraint) {
    if add_ref {
        unsafe {
            (*shared).share();
        }
    }
    let mut state = Box::new(SharedClauseState::new(shared, info, watched));
    if info.learnt() {
        solver.add_learnt_bytes(SMALL_CLAUSE_ALLOC_SIZE);
    }
    let state_ptr: *mut SharedClauseState = &mut *state;
    state.head.owner = state_ptr.cast::<c_void>();
    let constraint = Box::new(Constraint::new(SharedClause::new(state_ptr)));
    let constraint_ptr = Box::into_raw(constraint);
    state.head.constraint = constraint_ptr;
    let head_ptr: *mut ClauseHead = &mut state.head;
    let _ = Box::into_raw(state);
    (head_ptr, constraint_ptr)
}

pub fn new_shared_clause(
    solver: &mut Solver,
    lits: LitView<'_>,
    info: ClauseInfo,
) -> *mut ClauseHead {
    let shared = Box::into_raw(Box::new(SharedLiterals::new_shareable(
        lits,
        info.constraint_type(),
        1,
    )));
    let (head_ptr, _constraint_ptr) =
        build_shared_clause_from_ptr(solver, shared, info, lits, false);
    unsafe { (*head_ptr).attach(solver) };
    head_ptr
}

fn select_least_watched_pair(solver: &Solver, rep: &ClauseRep) -> (usize, usize) {
    let lits = rep.literals();
    let mut first = 0usize;
    let mut second = 1usize;
    let mut first_count = solver.num_clause_watches(!lits[first]);
    let mut second_count = solver.num_clause_watches(!lits[second]);
    if first_count > second_count {
        core::mem::swap(&mut first, &mut second);
        core::mem::swap(&mut first_count, &mut second_count);
    }
    for (index, literal) in lits.iter().enumerate().skip(2) {
        let count = solver.num_clause_watches(!*literal);
        if count < first_count {
            second = first;
            second_count = first_count;
            first = index;
            first_count = count;
        } else if count < second_count {
            second = index;
            second_count = count;
        }
    }
    (first, second)
}

fn reorder_for_watch_mode(solver: &Solver, rep: &ClauseRep, flags: CreateFlag) -> ClauseRep {
    let mut reordered = rep.clone();
    if reordered.size <= 2 {
        return reordered;
    }
    let mode = if has_flag(flags, CLAUSE_WATCH_FIRST) {
        WatchInit::WatchFirst
    } else if has_flag(flags, CLAUSE_WATCH_LEAST) {
        WatchInit::WatchLeast
    } else if has_flag(flags, CLAUSE_WATCH_RAND) {
        WatchInit::WatchRand
    } else {
        solver.watch_init_mode()
    };
    let (first, second) = match mode {
        WatchInit::WatchFirst | WatchInit::WatchRand => (0usize, 1usize),
        WatchInit::WatchLeast => select_least_watched_pair(solver, &reordered),
    };
    reordered.literals_mut().swap(0, first);
    if second == 0 {
        reordered.literals_mut().swap(1, first);
    } else {
        reordered.literals_mut().swap(1, second);
    }
    reordered
}

fn assert_clause_literal(
    solver: &mut Solver,
    clause: &ClauseRep,
    local: *mut ClauseHead,
) -> ClauseStatus {
    let implied_level = clause
        .literals()
        .get(1)
        .map(|lit| solver.level(lit.var()))
        .unwrap_or(0);
    let antecedent = if !local.is_null() {
        let head = unsafe { &*local };
        Antecedent::from_constraint_ptr(head.constraint_ptr())
    } else {
        match clause.size {
            0 | 1 => Antecedent::from_literal(lit_true),
            2 => Antecedent::from_literal(!clause.literals()[1]),
            _ => Antecedent::from_literals(!clause.literals()[1], !clause.literals()[2]),
        }
    };
    if solver.force_at_level(clause.literals()[0], implied_level, antecedent) {
        ClauseStatus::Unit
    } else {
        ClauseStatus::Unsat
    }
}

pub(crate) fn clause_head_propagate(
    head: &mut ClauseHead,
    solver: &mut Solver,
    literal: Literal,
) -> PropResult {
    let pos = if !head.head[0] == literal {
        0usize
    } else if !head.head[1] == literal {
        1usize
    } else {
        return PropResult::default();
    };
    let other = head.head[1 - pos];
    if solver.is_true(other) || clause_head_satisfied(head, solver) {
        return PropResult::default();
    }
    if update_watch(head, solver, pos) {
        return PropResult::new(true, false);
    }
    let antecedent = Antecedent::from_constraint_ptr(head.constraint_ptr());
    PropResult::new(solver.force(other, antecedent), true)
}

fn update_watch(head: &mut ClauseHead, solver: &mut Solver, pos: usize) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => update_explicit_watch(head, solver, pos),
        ClauseOwnerKind::Shared => update_shared_watch(head, solver, pos),
        ClauseOwnerKind::Unknown => false,
    }
}

fn update_explicit_watch(head: &mut ClauseHead, solver: &mut Solver, pos: usize) -> bool {
    let head_ptr = head as *mut ClauseHead;
    let state = explicit_state_from_head_mut(head);
    for index in 2..state.active_len as usize {
        let candidate = state.literals[index];
        if !solver.is_false(candidate) {
            state.literals.as_mut_slice().swap(pos, index);
            state.sync_head();
            solver.add_clause_watch(!state.head.head[pos], head_ptr);
            return true;
        }
    }
    false
}

fn update_shared_watch(head: &mut ClauseHead, solver: &mut Solver, pos: usize) -> bool {
    let state = shared_state_from_head(head);
    let shared = unsafe { &*state.shared };
    let other = head.head[1 ^ pos];
    let mut candidates = shared
        .literals()
        .iter()
        .copied()
        .filter(|lit| !solver.is_false(*lit) && *lit != other);
    let Some(new_watch) = candidates.next() else {
        return false;
    };
    head.head[pos] = new_watch;
    if let Some(cache) = candidates.next() {
        head.head[2] = cache;
    }
    solver.add_clause_watch(!head.head[pos], head as *mut ClauseHead);
    true
}

pub(crate) fn clause_head_locked(head: &ClauseHead, solver: &Solver) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head)
            .literals
            .as_slice()
            .iter()
            .copied()
            .any(|lit| {
                solver.is_true(lit)
                    && *solver.reason(lit.var()) == head.constraint_ptr() as *const Constraint
            }),
        ClauseOwnerKind::Shared => unsafe { &*shared_state_from_head(head).shared }
            .literals()
            .iter()
            .copied()
            .any(|lit| {
                solver.is_true(lit)
                    && *solver.reason(lit.var()) == head.constraint_ptr() as *const Constraint
            }),
        ClauseOwnerKind::Unknown => false,
    }
}

pub(crate) fn clause_head_satisfied(head: &ClauseHead, solver: &Solver) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head)
            .literals
            .as_slice()
            .iter()
            .copied()
            .any(|lit| solver.is_true(lit)),
        ClauseOwnerKind::Shared => unsafe { &*shared_state_from_head(head).shared }
            .literals()
            .iter()
            .copied()
            .any(|lit| solver.is_true(lit)),
        ClauseOwnerKind::Unknown => false,
    }
}

pub(crate) fn clause_head_size(head: &ClauseHead) -> u32 {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).active_size(),
        ClauseOwnerKind::Shared => unsafe { &*shared_state_from_head(head).shared }.size(),
        ClauseOwnerKind::Unknown => 0,
    }
}

pub(crate) fn clause_head_is_small(head: &ClauseHead) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).storage_is_small,
        ClauseOwnerKind::Shared => true,
        ClauseOwnerKind::Unknown => false,
    }
}

pub(crate) fn clause_head_contracted(head: &ClauseHead) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).contracted,
        ClauseOwnerKind::Shared | ClauseOwnerKind::Unknown => false,
    }
}

pub(crate) fn clause_head_strengthened(head: &ClauseHead) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).strengthened,
        ClauseOwnerKind::Shared | ClauseOwnerKind::Unknown => false,
    }
}

pub(crate) fn clause_head_compute_alloc_size(head: &ClauseHead) -> u32 {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).compute_alloc_size(),
        ClauseOwnerKind::Shared => SMALL_CLAUSE_ALLOC_SIZE,
        ClauseOwnerKind::Unknown => 0,
    }
}

pub(crate) fn clause_head_to_lits(head: &ClauseHead) -> Vec<Literal> {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => explicit_state_from_head(head).literals.as_slice().to_vec(),
        ClauseOwnerKind::Shared => unsafe { &*shared_state_from_head(head).shared }
            .literals()
            .to_vec(),
        ClauseOwnerKind::Unknown => Vec::new(),
    }
}

pub(crate) fn clause_head_simplify(
    head: &mut ClauseHead,
    solver: &mut Solver,
    reinit: bool,
) -> bool {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => simplify_explicit_clause(head, solver),
        ClauseOwnerKind::Shared => simplify_shared_clause(head, solver, reinit),
        ClauseOwnerKind::Unknown => false,
    }
}

fn simplify_explicit_clause(head: &mut ClauseHead, solver: &mut Solver) -> bool {
    if clause_head_satisfied(head, solver) {
        explicit_state_from_head(head).remove_current_undo_watch(solver);
        head.detach(solver);
        return true;
    }
    explicit_state_from_head(head).remove_current_undo_watch(solver);
    head.detach(solver);
    let state = explicit_state_from_head_mut(head);
    let old_full_len = state.full_len();
    let retained: Vec<Literal> = state
        .literals
        .as_slice()
        .iter()
        .copied()
        .filter(|lit| !solver.is_false(*lit))
        .collect();
    if retained.len() < 2 {
        return true;
    }
    state.literals.clear();
    state.literals.assign_from_slice(&retained);
    state.active_len = state.full_len();
    state.contracted = false;
    state.preserve_retained_allocation(old_full_len);
    state.sync_head();
    head.attach(solver);
    false
}

fn restore_explicit_clause(state: &mut ExplicitClauseState, solver: &mut Solver) {
    if !state.contracted {
        return;
    }
    let jump_level = solver.jump_level();
    let mut active_len = state.active_len as usize;
    while active_len < state.literals.len() {
        let literal = state.literals[active_len];
        let value = solver.value(literal.var());
        let level = solver.level(literal.var());
        if value == value_free || level > jump_level {
            active_len += 1;
            continue;
        }
        break;
    }
    if active_len >= state.literals.len() || solver.level(state.literals[active_len].var()) == 0 {
        state.active_len = state.full_len();
        state.contracted = false;
        state.extend_on_backtrack = false;
    } else if state.extend_on_backtrack {
        state.active_len = active_len as u32;
        solver.add_undo_watch(
            solver.level(state.literals[active_len].var()),
            state.head.constraint_ptr(),
        );
    } else {
        state.active_len = active_len as u32;
    }
    state.sync_head();
}

fn maybe_to_short(state: &ExplicitClauseState, solver: &mut Solver, allow_to_short: bool) -> bool {
    if !allow_to_short {
        return false;
    }
    if state.active_len != state.full_len() || state.active_len > 3 {
        return false;
    }
    let mut info = ClauseInfo::new(state.head.info.constraint_type());
    info.set_lbd(2);
    info.set_tagged(state.head.info.tagged());
    let rep = ClauseRep::prepared(
        &state.literals.as_slice()[..state.active_len as usize],
        info,
    );
    if !solver.allow_implicit(&rep) {
        return false;
    }
    solver.add(&rep, true)
}

fn simplify_shared_clause(head: &mut ClauseHead, solver: &mut Solver, _reinit: bool) -> bool {
    if clause_head_satisfied(head, solver) {
        head.detach(solver);
        return true;
    }
    let state = shared_state_from_head_mut(head);
    let shared = unsafe { &mut *state.shared };
    let opt_size = shared.simplify(solver);
    if opt_size == 0 {
        head.detach(solver);
        return true;
    }
    if solver.is_false(head.head[2]) {
        for &literal in shared.literals() {
            if !solver.is_false(literal) && !head.head[..2].contains(&literal) {
                head.head[2] = literal;
                break;
            }
        }
    }
    false
}

pub(crate) fn clause_head_destroy(
    head: &mut ClauseHead,
    solver: Option<&mut Solver>,
    detach: bool,
) {
    if let Some(solver) = solver {
        if head.info.learnt() {
            solver.free_learnt_bytes(u64::from(head.compute_alloc_size()));
        }
        if detach {
            if matches!(head.owner_kind, ClauseOwnerKind::Explicit) {
                explicit_state_from_head(head).remove_current_undo_watch(solver);
            }
            head.detach(solver);
        }
        solver.remove_constraint(head.constraint_ptr());
    }
    let constraint_ptr = head.constraint_ptr();
    unsafe {
        if !constraint_ptr.is_null() {
            drop(Box::from_raw(constraint_ptr));
        }
        match head.owner_kind {
            ClauseOwnerKind::Explicit => {
                let state_ptr = head.owner as *mut ExplicitClauseState;
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                }
            }
            ClauseOwnerKind::Shared => {
                let state_ptr = head.owner as *mut SharedClauseState;
                if !state_ptr.is_null() {
                    let shared_ptr = (*state_ptr).shared;
                    if !shared_ptr.is_null() && (*shared_ptr).release(1) {
                        drop(Box::from_raw(shared_ptr));
                    }
                    drop(Box::from_raw(state_ptr));
                }
            }
            ClauseOwnerKind::Unknown => {}
        }
    }
}

pub(crate) fn clause_head_clone_attach(head: &ClauseHead, other: &mut Solver) -> *mut ClauseHead {
    match head.owner_kind {
        ClauseOwnerKind::Explicit => {
            let state = explicit_state_from_head(head);
            let rep = ClauseRep::prepared(state.literals.as_slice(), head.info);
            let (head_ptr, _constraint_ptr) = build_explicit_clause(other, &rep);
            unsafe { (*head_ptr).attach(other) };
            head_ptr
        }
        ClauseOwnerKind::Shared => {
            let state = shared_state_from_head(head);
            let (head_ptr, _constraint_ptr) =
                build_shared_clause_from_ptr(other, state.shared, head.info, &head.head, true);
            unsafe { (*head_ptr).attach(other) };
            head_ptr
        }
        ClauseOwnerKind::Unknown => ptr::null_mut(),
    }
}

pub(crate) fn clause_head_strengthen(
    head: &mut ClauseHead,
    solver: &mut Solver,
    literal: Literal,
    allow_to_short: bool,
) -> ClauseStrengthenResult {
    if matches!(
        head.owner_kind,
        ClauseOwnerKind::Shared | ClauseOwnerKind::Unknown
    ) {
        return ClauseStrengthenResult::default();
    }
    explicit_state_from_head(head).remove_current_undo_watch(solver);
    head.detach(solver);
    let state = explicit_state_from_head_mut(head);
    let Some(index) = state
        .literals
        .as_slice()
        .iter()
        .position(|&lit| lit == literal)
    else {
        state.add_current_undo_watch(solver);
        head.attach(solver);
        return ClauseStrengthenResult::default();
    };
    let old_active_len = state.active_len;
    let old_full_len = state.full_len();
    state.literals.erase(index);
    if solver.tag_literal().var() != 0 && literal == !solver.tag_literal() {
        state.head.info.set_tagged(false);
    }
    if (index as u32) >= old_active_len || old_active_len < old_full_len {
        state.active_len = old_active_len.min(state.full_len());
    } else {
        state.active_len = old_active_len.saturating_sub(1).min(state.full_len());
    }
    if state.active_len > state.full_len() {
        state.active_len = state.full_len();
    }
    if state.active_len < 2 && state.full_len() >= 2 {
        state.active_len = 2;
    }
    if state.active_len < state.full_len() {
        state.contracted = true;
    } else {
        state.contracted = false;
        state.extend_on_backtrack = false;
    }
    state.preserve_retained_allocation(old_full_len);
    if literal == !solver.tag_literal() {
        state.head.info.set_tagged(false);
    }
    let remove_clause = state.literals.len() <= 1;
    let to_short = !remove_clause && maybe_to_short(state, solver, allow_to_short);
    if !remove_clause && !to_short {
        state.sync_head();
        state.add_current_undo_watch(solver);
        head.attach(solver);
    } else if remove_clause {
        if let Some(&unit) = state.literals.as_slice().first() {
            let _ = solver.force(unit, Antecedent::from_literal(lit_true));
        }
    }
    ClauseStrengthenResult {
        lit_removed: true,
        remove_clause: remove_clause || to_short,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LoopFormulaHandle {
    pub constraint: *mut Constraint,
    state: *mut LoopFormulaState,
}

impl LoopFormulaHandle {
    pub fn size(&self) -> u32 {
        unsafe { (*self.state).size() }
    }
}

#[derive(Debug)]
struct LoopFormulaState {
    constraint: *mut Constraint,
    activity: ConstraintScore,
    end: u32,
    size: u32,
    strengthened: bool,
    x_pos: u32,
    other: u32,
    literals: LitVec,
}

impl LoopFormulaState {
    fn new(clause: &ClauseRep, atoms: LitView<'_>) -> Self {
        let mut literals = LitVec::new();
        literals.push_back(lit_true);
        for &literal in clause.literals() {
            literals.push_back(literal);
        }
        literals.push_back(lit_true);
        for &atom in atoms {
            literals.push_back(atom);
        }
        Self {
            constraint: ptr::null_mut(),
            activity: clause.info.score(),
            end: clause.size + 1,
            size: size32(literals.as_slice()),
            strengthened: false,
            x_pos: 1,
            other: 1,
            literals,
        }
    }

    fn begin(&self) -> usize {
        1
    }

    fn x_begin(&self) -> usize {
        self.end as usize + 1
    }

    fn x_end(&self) -> usize {
        self.size as usize
    }

    fn size(&self) -> u32 {
        self.size - (2 + self.x_pos)
    }

    fn init(&mut self, solver: &mut Solver, clause: &ClauseRep, atoms: LitView<'_>, heu: bool) {
        if self.end > 2 {
            self.literals[2].flag();
            solver.add_watch(!self.literals[2], self.constraint, (2 << 1) + 1);
        }
        let first_atom = self.literals[1];
        for &atom in atoms {
            self.activity.bump_activity();
            solver.add_watch(!atom, self.constraint, (1 << 1) + 1);
            if heu {
                self.literals[1] = atom;
                let solver_ptr: *const Solver = solver;
                solver.heuristic_mut().new_constraint(
                    unsafe { &*solver_ptr },
                    &self.literals.as_slice()[1..1 + clause.size as usize],
                    ConstraintType::Loop,
                );
            }
        }
        self.literals[1] = first_atom;
        self.literals[1].flag();
    }

    fn detach(&mut self, solver: &mut Solver) {
        for index in (self.begin() + self.x_pos as usize)..self.end as usize {
            if is_sentinel(self.literals[index]) {
                break;
            }
            if self.literals[index].flagged() {
                solver.remove_watch(!self.literals[index], self.constraint);
                self.literals[index].unflag();
            }
        }
        for index in self.x_begin()..self.x_end() {
            solver.remove_watch(!self.literals[index], self.constraint);
        }
    }

    fn attach(&mut self, solver: &mut Solver) {
        for index in (self.begin() + self.x_pos as usize)..self.end as usize {
            if is_sentinel(self.literals[index]) {
                break;
            }
            if self.literals[index].flagged() {
                solver.add_watch(
                    !self.literals[index],
                    self.constraint,
                    ((index as u32) << 1) + 1,
                );
            }
        }
        if self.x_pos != 0 {
            for index in self.x_begin()..self.x_end() {
                solver.add_watch(
                    !self.literals[index],
                    self.constraint,
                    (self.x_pos << 1) + 1,
                );
            }
        }
    }

    fn other_is_sat(&mut self, solver: &Solver) -> bool {
        if self.other != self.x_pos {
            return solver.is_true(self.literals[self.other as usize]);
        }
        if !solver.is_true(self.literals[self.other as usize]) {
            return false;
        }
        for index in self.x_begin()..self.x_end() {
            let lit = self.literals[index];
            if !solver.is_true(lit) {
                if self.literals[self.x_pos as usize].flagged() {
                    self.literals[self.x_pos as usize] = lit;
                    self.literals[self.x_pos as usize].flag();
                } else {
                    self.literals[self.x_pos as usize] = lit;
                }
                return false;
            }
        }
        true
    }
}

#[derive(Debug)]
struct LoopFormulaConstraint {
    state: *mut LoopFormulaState,
}

impl LoopFormulaConstraint {
    fn new(state: *mut LoopFormulaState) -> Self {
        Self { state }
    }

    fn state_mut(&mut self) -> &mut LoopFormulaState {
        unsafe { &mut *self.state }
    }

    fn state_ref(&self) -> &LoopFormulaState {
        unsafe { &*self.state }
    }
}

impl ConstraintDyn for LoopFormulaConstraint {
    fn propagate(
        &mut self,
        solver: &mut Solver,
        mut literal: Literal,
        data: &mut u32,
    ) -> PropResult {
        let state = self.state_mut();
        if state.other_is_sat(solver) {
            return PropResult::new(true, true);
        }
        let idx = (*data >> 1) as usize;
        let head = idx == state.x_pos as usize;
        if head {
            literal = !literal;
            if state.literals[idx] != literal && solver.is_false(state.literals[idx]) {
                return PropResult::new(true, true);
            }
            if !state.literals[idx].flagged() {
                state.literals[idx] = literal;
                return PropResult::new(true, true);
            }
            state.literals[idx] = literal;
            state.literals[idx].flag();
        }
        let mut bounds = 0;
        let mut dir = (((*data) & 1) << 1) as i32 - 1;
        let mut watch = idx as i32;
        loop {
            loop {
                watch += dir;
                let candidate = state.literals[watch as usize];
                if !solver.is_false(candidate) {
                    break;
                }
            }
            let next = state.literals[watch as usize];
            if !is_sentinel(next) {
                let next_index = watch as usize;
                if next.flagged() {
                    state.other = next_index as u32;
                    continue;
                }
                state.literals[idx].unflag();
                state.literals[next_index].flag();
                if next_index != state.x_pos as usize {
                    solver.add_watch(
                        !state.literals[next_index],
                        state.constraint,
                        ((next_index as u32) << 1) + u32::from(dir == 1),
                    );
                }
                return PropResult::new(true, head);
            }
            bounds += 1;
            if bounds == 1 {
                watch = idx as i32;
                dir *= -1;
                *data ^= 1;
                continue;
            }
            let antecedent = Antecedent::from_constraint_ptr(state.constraint);
            let mut ok = solver.force(state.literals[state.other as usize], antecedent);
            if state.other == state.x_pos && ok {
                for index in state.x_begin()..state.x_end() {
                    ok = solver.force(state.literals[index], antecedent);
                    if !ok {
                        break;
                    }
                }
            }
            return PropResult::new(ok, true);
        }
    }

    fn reason(&mut self, _solver: &mut Solver, literal: Literal, lits: &mut LitVec) {
        let state = self.state_mut();
        let mut score = state.activity;
        score.bump_activity();
        state.activity = score;
        let start = state.begin() + usize::from(state.other == state.x_pos);
        for index in start..state.end as usize {
            let current = state.literals[index];
            if !is_sentinel(current) && current != literal {
                lits.push_back(!current);
            }
        }
    }

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
    }

    fn simplify(&mut self, solver: &mut Solver, _reinit: bool) -> bool {
        let state = self.state_mut();
        if state.other_is_sat(solver)
            || (state.other != state.x_pos && {
                state.other = state.x_pos;
                state.other_is_sat(solver)
            })
        {
            state.detach(solver);
            return true;
        }

        let old_total = state.x_end();
        let mut it = state.begin();
        while it < state.end as usize && solver.value(state.literals[it].var()) == value_free {
            it += 1;
        }
        let mut write = it;
        if it < state.end as usize && !is_sentinel(state.literals[it]) {
            if state.literals[it] == state.literals[state.x_pos as usize] {
                state.x_pos = 0;
            }
            while it < state.end as usize && !is_sentinel(state.literals[it]) {
                let current = state.literals[it];
                if solver.value(current.var()) == value_free {
                    state.literals[write] = current;
                    write += 1;
                } else if solver.is_true(current) {
                    state.detach(solver);
                    return true;
                } else {
                    debug_assert!(!current.flagged(), "constraint not propagated");
                }
                it += 1;
            }
            state.literals[write] = lit_true;
            state.end = write as u32;
        }

        let mut read_atoms = it.saturating_add(1);
        let mut write_atoms = write.saturating_add(1);
        while read_atoms < old_total {
            let current = state.literals[read_atoms];
            if solver.value(current.var()) == value_free && state.x_pos != 0 {
                state.literals[write_atoms] = current;
                write_atoms += 1;
            } else {
                solver.remove_watch(!current, state.constraint);
            }
            read_atoms += 1;
        }

        let is_clause = write_atoms.saturating_sub(state.x_begin()) == 1;
        if is_clause {
            write_atoms -= 1;
        }
        let size_changed = write_atoms != old_total;
        if size_changed {
            state.strengthened = true;
            if is_clause {
                state.x_pos = 0;
            }
            shrink_vec_to(&mut state.literals, write_atoms);
            state.size = write_atoms as u32;
        }

        state.other = state.x_pos + 1;
        let active_end = state.end.saturating_sub(1) as usize;
        let active = ClauseRep::create(
            &state.literals.as_slice()[state.begin()..active_end],
            ClauseInfo::new(ConstraintType::Loop),
        );
        if solver.allow_implicit(&active) {
            state.detach(solver);
            let loop_atoms = if state.x_pos != 0 {
                state.literals.as_slice()[state.x_begin()..state.x_end()].to_vec()
            } else {
                vec![state.literals[state.begin()]]
            };
            for atom in loop_atoms {
                debug_assert_eq!(solver.value(atom.var()), value_free);
                let mut lits = state.literals.as_slice()[state.begin()..active_end].to_vec();
                lits[0] = atom;
                let rep = ClauseRep::prepared(&lits, ClauseInfo::new(ConstraintType::Loop));
                let _ = ClauseCreator::create_from_rep(solver, &rep, CLAUSE_NO_ADD);
            }
            return true;
        }
        if size_changed {
            state.detach(solver);
            if state.x_pos != 0 {
                state.literals[state.x_pos as usize].flag();
            }
            let body_watch = state.begin() + state.x_pos as usize + 1;
            if body_watch < state.end as usize && !is_sentinel(state.literals[body_watch]) {
                state.literals[body_watch].flag();
            }
            state.attach(solver);
        }
        false
    }

    fn destroy(&mut self, solver: Option<&mut Solver>, detach: bool) {
        let state = self.state_mut();
        if let Some(solver) = solver {
            if detach {
                state.detach(solver);
            }
            solver.remove_constraint(state.constraint);
        }
        unsafe {
            drop(Box::from_raw(self.state));
        }
    }

    fn minimize(
        &mut self,
        solver: &mut Solver,
        literal: Literal,
        rec: *mut crate::clasp::constraint::CCMinRecursive,
    ) -> bool {
        let state = self.state_mut();
        let mut score = state.activity;
        score.bump_activity();
        state.activity = score;
        let start = state.begin() + usize::from(state.other == state.x_pos);
        for index in start..state.end as usize {
            let current = state.literals[index];
            if !is_sentinel(current) && current != literal && !solver.cc_minimize(!current, rec) {
                return false;
            }
        }
        true
    }

    fn constraint_type(&self) -> ConstraintType {
        ConstraintType::Loop
    }

    fn locked(&self, solver: &Solver) -> bool {
        let state = self.state_ref();
        if state.other != state.x_pos || !solver.is_true(state.literals[state.other as usize]) {
            return solver.is_true(state.literals[state.other as usize])
                && *solver.reason(state.literals[state.other as usize].var())
                    == state.constraint as *const Constraint;
        }
        (state.x_begin()..state.x_end()).any(|index| {
            let literal = state.literals[index];
            solver.is_true(literal)
                && *solver.reason(literal.var()) == state.constraint as *const Constraint
        })
    }

    fn activity(&self) -> ConstraintScore {
        self.state_ref().activity
    }

    fn decrease_activity(&mut self) {
        self.state_mut().activity.reduce();
    }

    fn reset_activity(&mut self) {
        self.state_mut()
            .activity
            .reset(0, crate::clasp::constraint::lbd_max);
    }

    fn is_open(
        &mut self,
        solver: &Solver,
        types: &crate::clasp::constraint::TypeSet,
        free_lits: &mut LitVec,
    ) -> u32 {
        let state = self.state_mut();
        if !types.contains(ConstraintType::Loop) || state.other_is_sat(solver) {
            return 0;
        }
        for index in (state.begin() + state.x_pos as usize)..state.end as usize {
            let literal = state.literals[index];
            if is_sentinel(literal) {
                break;
            }
            let value = solver.value(literal.var());
            if value == value_free {
                free_lits.push_back(literal);
            } else if value == true_value(literal) {
                state.other = index as u32;
                return 0;
            }
        }
        for index in state.x_begin()..state.x_end() {
            let literal = state.literals[index];
            if solver.value(literal.var()) == value_free {
                free_lits.push_back(literal);
            }
        }
        ConstraintType::Loop.as_u32()
    }
}

pub fn new_loop_formula(
    solver: &mut Solver,
    clause: &ClauseRep,
    atoms: LitView<'_>,
    update_heuristic: bool,
) -> LoopFormulaHandle {
    let mut state = Box::new(LoopFormulaState::new(clause, atoms));
    let state_ptr: *mut LoopFormulaState = &mut *state;
    let constraint = Box::new(Constraint::new(LoopFormulaConstraint::new(state_ptr)));
    let constraint_ptr = Box::into_raw(constraint);
    state.constraint = constraint_ptr;
    state.init(solver, clause, atoms, update_heuristic);
    let _ = Box::into_raw(state);
    LoopFormulaHandle {
        constraint: constraint_ptr,
        state: state_ptr,
    }
}

#[derive(Debug, Default)]
pub struct ClauseCreator {
    solver: Option<NonNull<Solver>>,
    literals: LitVec,
    extra: ClauseInfo,
    flags: CreateFlag,
}

impl ClauseCreator {
    pub fn new(solver: Option<&mut Solver>) -> Self {
        Self {
            solver: solver.map(NonNull::from),
            literals: LitVec::new(),
            extra: ClauseInfo::default(),
            flags: CLAUSE_FLAG_NONE,
        }
    }

    pub fn set_solver(&mut self, solver: &mut Solver) {
        self.solver = Some(NonNull::from(solver));
    }

    pub fn add_default_flags(&mut self, flags: CreateFlag) {
        self.flags |= flags;
    }

    pub fn default_flags(&self) -> CreateFlag {
        self.flags
    }

    pub fn reserve(&mut self, size: u32) {
        self.literals.reserve(size as usize);
    }

    pub fn clear(&mut self) {
        self.literals.clear();
    }

    pub fn start(&mut self, constraint_type: ConstraintType) -> &mut Self {
        let solver = self.solver_mut();
        assert!(solver.decision_level() == 0 || constraint_type != ConstraintType::Static);
        self.literals.clear();
        self.extra = ClauseInfo::new(constraint_type);
        self
    }

    pub fn start_default(&mut self) -> &mut Self {
        self.start(ConstraintType::Static)
    }

    pub fn set_activity(&mut self, activity: u32) -> &mut Self {
        self.extra.set_activity(activity);
        self
    }

    pub fn set_lbd(&mut self, lbd: u32) -> &mut Self {
        self.extra.set_lbd(lbd);
        self
    }

    pub fn add(&mut self, literal: Literal) -> &mut Self {
        self.literals.push_back(literal);
        self
    }

    pub fn size(&self) -> u32 {
        size32(&self.literals)
    }

    pub fn lits(&self) -> &[Literal] {
        self.literals.as_slice()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        self.extra.constraint_type()
    }

    pub fn r#type(&self) -> ConstraintType {
        self.constraint_type()
    }

    pub fn info(&self) -> ClauseInfo {
        self.extra
    }

    pub fn prepare(&mut self, force_simplify: bool) -> ClauseRep {
        let flags = if force_simplify {
            CLAUSE_FORCE_SIMPLIFY
        } else {
            CLAUSE_FLAG_NONE
        };
        let info = self.extra;
        let solver = self.solver.expect("ClauseCreator requires a solver");
        unsafe {
            Self::prepare_vec(
                solver.as_ptr().as_mut().expect("valid solver"),
                &mut self.literals,
                flags,
                info,
            )
        }
    }

    pub fn watch_order(solver: &Solver, literal: Literal) -> u32 {
        let value = solver.value(literal.var());
        if value == value_free {
            return solver.decision_level() + 1;
        }
        solver.level(literal.var()) ^ (0u32.wrapping_sub(u32::from(value == true_value(literal))))
    }

    pub fn prepare_vec(
        solver: &mut Solver,
        lits: &mut LitVec,
        flags: CreateFlag,
        info: ClauseInfo,
    ) -> ClauseRep {
        if lits.is_empty() {
            lits.push_back(lit_false);
        }
        if !has_flag(flags, CLAUSE_NO_PREPARE) || has_flag(flags, CLAUSE_FORCE_SIMPLIFY) {
            let input = lits.as_slice().to_vec();
            let prepared = Self::prepare_span(solver, &input, info, flags, lits.as_mut_slice());
            shrink_vec_to(lits, prepared.size as usize);
            return prepared;
        }
        ClauseRep::prepared(lits.as_slice(), info)
    }

    pub fn status(solver: &mut Solver, lits: LitView<'_>) -> ClauseStatus {
        if lits.is_empty() {
            return ClauseStatus::Empty;
        }
        let mut prepared_lits = LitVec::new();
        prepared_lits.assign_from_slice(lits);
        let prepared = Self::prepare_vec(
            solver,
            &mut prepared_lits,
            CLAUSE_FLAG_NONE,
            ClauseInfo::default(),
        );
        Self::status_prepared(solver, &prepared)
    }

    pub fn status_clause(solver: &mut Solver, clause: &ClauseRep) -> ClauseStatus {
        if clause.prep {
            Self::status_prepared(solver, clause)
        } else {
            Self::status(solver, clause.literals())
        }
    }

    pub fn ignore_clause(
        solver: &Solver,
        clause: &ClauseRep,
        status: ClauseStatus,
        flags: CreateFlag,
    ) -> bool {
        let state = status.bits() & (ClauseStatus::Sat.bits() | ClauseStatus::Unsat.bits());
        if state == ClauseStatus::Open.bits() {
            return false;
        }
        if state == ClauseStatus::Unsat.bits() {
            return status != ClauseStatus::Empty && has_flag(flags, CLAUSE_NOT_CONFLICT);
        }
        debug_assert_eq!(state, ClauseStatus::Sat.bits());
        status == ClauseStatus::Subsumed
            || (status == ClauseStatus::Sat
                && (has_flag(flags, CLAUSE_NOT_SAT)
                    || (has_flag(flags, CLAUSE_NOT_ROOT_SAT)
                        && solver.level(clause.literals()[0].var()) <= solver.root_level())))
    }

    pub fn end_with_defaults(&mut self) -> ClauseCreationResult {
        self.end(CLAUSE_NOT_SAT | CLAUSE_NOT_CONFLICT)
    }

    pub fn end(&mut self, flags: CreateFlag) -> ClauseCreationResult {
        assert!(self.solver.is_some());
        let flags = flags | self.flags;
        let info = self.extra;
        let solver_ptr = self
            .solver
            .expect("ClauseCreator requires a solver")
            .as_ptr();
        let prepared =
            unsafe { Self::prepare_vec(&mut *solver_ptr, &mut self.literals, flags, info) };
        unsafe { Self::create_prepared(&mut *solver_ptr, &prepared, flags) }
    }

    pub fn create(
        solver: &mut Solver,
        lits: &mut LitVec,
        flags: CreateFlag,
        info: ClauseInfo,
    ) -> ClauseCreationResult {
        let prepared = Self::prepare_vec(solver, lits, flags, info);
        Self::create_prepared(solver, &prepared, flags)
    }

    pub fn create_from_rep(
        solver: &mut Solver,
        rep: &ClauseRep,
        flags: CreateFlag,
    ) -> ClauseCreationResult {
        let prepared = if !rep.prep && !has_flag(flags, CLAUSE_NO_PREPARE) {
            let mut lits = LitVec::new();
            lits.assign_from_slice(rep.literals());
            Self::prepare_vec(solver, &mut lits, flags, rep.info)
        } else {
            ClauseRep::prepared(rep.literals(), rep.info)
        };
        Self::create_prepared(solver, &prepared, flags)
    }

    pub fn create_rep(
        solver: &mut Solver,
        rep: &ClauseRep,
        flags: CreateFlag,
    ) -> ClauseCreationResult {
        Self::create_from_rep(solver, rep, flags)
    }

    pub fn integrate(
        solver: &mut Solver,
        clause: &SharedLiterals,
        flags: CreateFlag,
    ) -> ClauseCreationResult {
        Self::integrate_typed(solver, clause, flags, clause.constraint_type())
    }

    pub fn integrate_typed(
        solver: &mut Solver,
        clause: &SharedLiterals,
        mut flags: CreateFlag,
        ty: ConstraintType,
    ) -> ClauseCreationResult {
        assert!(!solver.has_conflict());
        let mut prepared_lits = LitVec::new();
        prepared_lits.assign_from_slice(clause.literals());
        let prepared = Self::prepare_vec(
            solver,
            &mut prepared_lits,
            CLAUSE_FLAG_NONE,
            ClauseInfo::new(ty),
        );
        let status = Self::status_prepared(solver, &prepared);
        let implicit_limit =
            if has_flag(flags, CLAUSE_EXPLICIT) || !solver.allow_implicit(&prepared) {
                1
            } else {
                3
            };
        if Self::ignore_clause(solver, &prepared, status, flags) {
            return ClauseCreationResult::new(ptr::null_mut(), status);
        }
        if !has_flag(flags, CLAUSE_NO_HEURISTIC) {
            let lits = clause.literals().to_vec();
            let solver_ptr: *const Solver = solver;
            solver
                .heuristic_mut()
                .new_constraint(unsafe { &*solver_ptr }, &lits, ty);
        }
        let mut result = ClauseCreationResult::new(ptr::null_mut(), status);
        if prepared.size > implicit_limit {
            result.local = if prepared.size > EXPLICIT_CLAUSE_MAX_SHORT_LEN
                && should_physically_share(solver, ty)
            {
                Self::new_integrated_shared(solver, clause, prepared.literals(), prepared.info)
            } else {
                Self::new_unshared(solver, clause, prepared.literals(), prepared.info)
            };
        } else if !has_flag(flags, CLAUSE_NO_ADD) {
            let _ = solver.add(&prepared, true);
            if prepared.info.learnt() {
                solver
                    .stats_mut()
                    .add_learnt(prepared.size, prepared.info.constraint_type());
            }
            flags |= CLAUSE_NO_ADD;
        }
        if !has_flag(flags, CLAUSE_NO_ADD) && !result.local.is_null() {
            let head = unsafe { &*result.local };
            solver.add_learnt_constraint(
                head.constraint_ptr(),
                prepared.size,
                prepared.info.constraint_type(),
            );
            if has_flag(flags, CLAUSE_INT_LBD) && status.unit() {
                let lbd = solver.count_levels(clause.literals());
                unsafe { &mut *result.local }
                    .reset_score(ConstraintScore::new(prepared.info.activity(), lbd));
            }
        }
        if status.unit() || status == ClauseStatus::Unsat || status == ClauseStatus::Asserting {
            result.status = assert_clause_literal(solver, &prepared, result.local);
        }
        result
    }

    fn prepare_span(
        solver: &mut Solver,
        input: LitView<'_>,
        info: ClauseInfo,
        flags: CreateFlag,
        out: &mut [Literal],
    ) -> ClauseRep {
        assert!(!out.is_empty() || input.is_empty());
        let mut info_out = info;
        let mut size = 0u32;
        let mut abst_w1 = 0u32;
        let mut abst_w2 = 0u32;
        let simplify = has_flag(flags, CLAUSE_FORCE_SIMPLIFY) && out.len() >= input.len();
        let tag = !solver.tag_literal();
        let mut v_max = if solver.num_problem_vars() > solver.num_vars() && !input.is_empty() {
            input.iter().map(|literal| literal.var()).max().unwrap_or(0)
        } else {
            0
        };
        solver.acquire_problem_var(v_max);
        let max_out = out.len().saturating_sub(1);
        let mut j = 0usize;

        for &literal in input {
            let mut abst_p = Self::watch_order(solver, literal);
            if abst_p != u32::MAX && (!simplify || !solver.seen_var(literal.var())) {
                out[j] = literal;
                if literal == tag {
                    info_out.set_tagged(true);
                }
                if literal.var() > v_max {
                    v_max = literal.var();
                }
                if simplify {
                    solver.mark_seen_literal(literal);
                }
                if abst_p > abst_w1 {
                    core::mem::swap(&mut abst_p, &mut abst_w1);
                    out.swap(0, j);
                }
                if abst_p > abst_w2 {
                    core::mem::swap(&mut abst_p, &mut abst_w2);
                    if out.len() > 1 {
                        out.swap(1, j);
                    }
                }
                if j != max_out {
                    j += 1;
                }
                size += 1;
            } else if abst_p == u32::MAX
                || (simplify && abst_p != 0 && solver.seen_literal(!literal))
            {
                abst_w1 = u32::MAX;
                break;
            }
        }

        if simplify {
            for literal in &out[..size as usize] {
                solver.clear_seen_var(literal.var());
            }
        }

        if abst_w1 == u32::MAX || (abst_w2 != 0 && out.len() > 1 && out[0].var() == out[1].var()) {
            out[0] = if abst_w1 == u32::MAX || (out.len() > 1 && out[0] == !out[1]) {
                lit_true
            } else {
                out[0]
            };
            size = 1;
        }
        info_out.set_aux(solver.aux_var(v_max));
        ClauseRep::prepared(&out[..size as usize], info_out)
    }

    fn status_prepared(solver: &Solver, clause: &ClauseRep) -> ClauseStatus {
        let decision_level = solver.decision_level();
        let literals = clause.literals();
        let mut first_watch = if clause.size != 0 {
            Self::watch_order(solver, literals[0])
        } else {
            0
        };
        if first_watch == u32::MAX {
            return ClauseStatus::Subsumed;
        }
        let second_watch = if clause.size > 1 {
            Self::watch_order(solver, literals[1])
        } else {
            0
        };

        let mut status = ClauseStatus::Open.bits();
        if first_watch > var_max {
            status |= ClauseStatus::Sat.bits();
            first_watch = !first_watch;
        } else if first_watch <= decision_level {
            status |= if first_watch != 0 {
                ClauseStatus::Unsat.bits()
            } else {
                ClauseStatus::Empty.bits()
            };
        }
        if second_watch <= decision_level && first_watch > second_watch {
            status |= ClauseStatus::Unit.bits();
        }
        ClauseStatus::from_bits(status)
    }

    fn create_prepared(
        solver: &mut Solver,
        clause: &ClauseRep,
        flags: CreateFlag,
    ) -> ClauseCreationResult {
        assert!(solver.decision_level() == 0 || clause.info.learnt());
        let mut status = Self::status_clause(solver, clause);
        if Self::ignore_clause(solver, clause, status, flags) {
            return ClauseCreationResult::new(ptr::null_mut(), status);
        }
        if clause.size <= 1 {
            let _ = solver.add(clause, true);
            status = if solver.has_conflict() {
                ClauseStatus::Unsat
            } else {
                ClauseStatus::Unit
            };
            return ClauseCreationResult::new(ptr::null_mut(), status);
        }
        if !has_flag(flags, CLAUSE_NO_HEURISTIC) {
            let lits = clause.literals().to_vec();
            let ty = clause.info.constraint_type();
            let solver_ptr: *const Solver = solver;
            solver
                .heuristic_mut()
                .new_constraint(unsafe { &*solver_ptr }, &lits, ty);
        }
        let mut result = ClauseCreationResult::new(ptr::null_mut(), status);
        if clause.size > 3 || has_flag(flags, CLAUSE_EXPLICIT) || !solver.allow_implicit(clause) {
            result.local = if clause.info.learnt() {
                Self::new_learnt_clause(solver, clause, flags)
            } else {
                Self::new_problem_clause(solver, clause, flags)
            };
        } else {
            let _ = solver.add(clause, true);
        }
        if status.unit() || status == ClauseStatus::Unsat || status == ClauseStatus::Asserting {
            result.status = assert_clause_literal(solver, clause, result.local);
        }
        result
    }

    fn new_problem_clause(
        solver: &mut Solver,
        clause: &ClauseRep,
        flags: CreateFlag,
    ) -> *mut ClauseHead {
        let ordered = reorder_for_watch_mode(solver, clause, flags);
        let (head_ptr, constraint_ptr) = build_explicit_clause(solver, &ordered);
        unsafe { (*head_ptr).attach(solver) };
        if !has_flag(flags, CLAUSE_NO_ADD) {
            solver.add_constraint(constraint_ptr);
        }
        head_ptr
    }

    fn new_learnt_clause(
        solver: &mut Solver,
        clause: &ClauseRep,
        flags: CreateFlag,
    ) -> *mut ClauseHead {
        let compress_limit = if solver.strategies().compress == 0 {
            u32::MAX
        } else {
            solver.strategies().compress
        };
        let second_false = clause
            .literals()
            .get(1)
            .copied()
            .is_some_and(|lit| solver.is_false(lit));
        let (head_ptr, constraint_ptr) = if second_false && clause.size >= compress_limit {
            build_contracted_explicit_clause(solver, clause, 2, true)
        } else {
            build_explicit_clause(solver, clause)
        };
        unsafe { (*head_ptr).attach(solver) };
        if !has_flag(flags, CLAUSE_NO_ADD) {
            solver.add_learnt_constraint(
                constraint_ptr,
                clause.size,
                clause.info.constraint_type(),
            );
        }
        head_ptr
    }

    fn new_unshared(
        solver: &mut Solver,
        clause: &SharedLiterals,
        watched: &[Literal],
        info: ClauseInfo,
    ) -> *mut ClauseHead {
        let mut temp = LitVec::new();
        temp.reserve(clause.size() as usize);
        temp.assign_from_slice(&watched[..watched.len().min(2)]);
        for &literal in clause.literals() {
            if Self::watch_order(solver, literal) > 0 && !temp.as_slice().contains(&literal) {
                temp.push_back(literal);
            }
        }
        let rep = ClauseRep::prepared(temp.as_slice(), info);
        let (head_ptr, _constraint_ptr) = build_explicit_clause(solver, &rep);
        unsafe { (*head_ptr).attach(solver) };
        head_ptr
    }

    fn new_integrated_shared(
        solver: &mut Solver,
        clause: &SharedLiterals,
        watched: &[Literal],
        info: ClauseInfo,
    ) -> *mut ClauseHead {
        let shared = Box::into_raw(Box::new(SharedLiterals::new_shareable(
            clause.literals(),
            info.constraint_type(),
            1,
        )));
        let (head_ptr, _constraint_ptr) = build_shared_clause_from_ptr(
            solver,
            shared,
            info,
            &watched[..watched.len().min(3)],
            false,
        );
        unsafe { (*head_ptr).attach(solver) };
        head_ptr
    }

    fn solver_mut(&mut self) -> &mut Solver {
        unsafe {
            self.solver
                .expect("ClauseCreator requires a solver")
                .as_mut()
        }
    }
}

impl Index<usize> for ClauseCreator {
    type Output = Literal;

    fn index(&self, index: usize) -> &Self::Output {
        &self.literals.as_slice()[index]
    }
}

fn has_flag(flags: CreateFlag, mask: CreateFlag) -> bool {
    (flags & mask) != 0
}
