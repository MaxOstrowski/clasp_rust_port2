//! Partial Rust port of `original_clasp/clasp/clause.h` and `original_clasp/src/clause.cpp`.
//!
//! This module now covers the Bundle A explicit clause runtime together with
//! `ClauseCreator`'s creation and integration paths on top of the current
//! solver/shared-context kernel. Shared clauses, clause contraction, and the
//! original small-clause allocator remain simplified.

use core::ffi::c_void;
use core::ptr::{self, NonNull};

use crate::clasp::constraint::{
    Antecedent, ClauseHead, ClauseStrengthenResult, Constraint, ConstraintDyn, ConstraintInfo,
    ConstraintScore, ConstraintType, PropResult, Solver,
};
use crate::clasp::literal::{
    LitVec, LitView, Literal, lit_false, lit_true, true_value, value_free, var_max,
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

    pub fn size(&self) -> u32 {
        self.size_type >> 2
    }

    pub fn constraint_type(&self) -> ConstraintType {
        ConstraintType::from_u32(self.size_type & 3).expect("invalid SharedLiterals type bits")
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
}

impl ExplicitClauseState {
    fn new(rep: &ClauseRep) -> Self {
        let mut literals = LitVec::new();
        literals.assign_from_slice(rep.literals());
        let mut state = Self {
            head: ClauseHead::new(rep.info),
            literals,
        };
        state.sync_head();
        state
    }

    fn sync_head(&mut self) {
        self.head.head = [lit_false; 3];
        for (dst, src) in self
            .head
            .head
            .iter_mut()
            .zip(self.literals.as_slice().iter().copied())
        {
            *dst = src;
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

    fn clone_attach(&self, _other: &mut Solver) -> Option<Box<Constraint>> {
        None
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
        for &lit in state.literals.as_slice() {
            if solver.value(lit.var()) == value_free {
                free.push_back(lit);
            }
        }
        ty.as_u32()
    }
}

fn explicit_state_from_head(head: &ClauseHead) -> &ExplicitClauseState {
    unsafe { &*(head.owner as *const ExplicitClauseState) }
}

fn explicit_state_from_head_mut(head: &mut ClauseHead) -> &mut ExplicitClauseState {
    unsafe { &mut *(head.owner as *mut ExplicitClauseState) }
}

fn build_explicit_clause(rep: &ClauseRep) -> (*mut ClauseHead, *mut Constraint) {
    let mut state = Box::new(ExplicitClauseState::new(rep));
    let state_ptr: *mut ExplicitClauseState = &mut *state;
    state.head.owner = state_ptr.cast::<c_void>();
    let constraint = Box::new(Constraint::new(ExplicitClause::new(state_ptr)));
    let constraint_ptr = Box::into_raw(constraint);
    state.head.constraint = constraint_ptr;
    let head_ptr: *mut ClauseHead = &mut state.head;
    let _ = Box::into_raw(state);
    (head_ptr, constraint_ptr)
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
        .unwrap_or(0)
        .min(solver.decision_level());
    if solver.decision_level() > implied_level.max(solver.root_level()) {
        solver.backtrack(implied_level.max(solver.root_level()));
    }
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
    if solver.force(clause.literals()[0], antecedent) {
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
    let head_ptr = head as *mut ClauseHead;
    let state = explicit_state_from_head_mut(head);
    for index in 2..state.literals.len() {
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

pub(crate) fn clause_head_locked(head: &ClauseHead, solver: &Solver) -> bool {
    let state = explicit_state_from_head(head);
    state.literals.as_slice().iter().copied().any(|lit| {
        solver.is_true(lit)
            && *solver.reason(lit.var()) == head.constraint_ptr() as *const Constraint
    })
}

pub(crate) fn clause_head_satisfied(head: &ClauseHead, solver: &Solver) -> bool {
    explicit_state_from_head(head)
        .literals
        .as_slice()
        .iter()
        .copied()
        .any(|lit| solver.is_true(lit))
}

pub(crate) fn clause_head_size(head: &ClauseHead) -> u32 {
    size32(explicit_state_from_head(head).literals.as_slice())
}

pub(crate) fn clause_head_to_lits(head: &ClauseHead) -> Vec<Literal> {
    explicit_state_from_head(head).literals.as_slice().to_vec()
}

pub(crate) fn clause_head_simplify(
    head: &mut ClauseHead,
    solver: &mut Solver,
    _reinit: bool,
) -> bool {
    if clause_head_satisfied(head, solver) {
        return true;
    }
    head.detach(solver);
    let state = explicit_state_from_head_mut(head);
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
    state.sync_head();
    head.attach(solver);
    false
}

pub(crate) fn clause_head_destroy(
    head: &mut ClauseHead,
    solver: Option<&mut Solver>,
    detach: bool,
) {
    if let Some(solver) = solver {
        if detach {
            head.detach(solver);
        }
        solver.remove_constraint(head.constraint_ptr());
    }
    let state_ptr = head.owner as *mut ExplicitClauseState;
    let constraint_ptr = head.constraint_ptr();
    unsafe {
        if !constraint_ptr.is_null() {
            drop(Box::from_raw(constraint_ptr));
        }
        if !state_ptr.is_null() {
            drop(Box::from_raw(state_ptr));
        }
    }
}

pub(crate) fn clause_head_clone_attach(head: &ClauseHead, other: &mut Solver) -> *mut ClauseHead {
    let state = explicit_state_from_head(head);
    let rep = ClauseRep::prepared(state.literals.as_slice(), head.info);
    let (head_ptr, _constraint_ptr) = build_explicit_clause(&rep);
    unsafe { (*head_ptr).attach(other) };
    head_ptr
}

pub(crate) fn clause_head_strengthen(
    head: &mut ClauseHead,
    solver: &mut Solver,
    literal: Literal,
    _allow_to_short: bool,
) -> ClauseStrengthenResult {
    if clause_head_locked(head, solver) {
        return ClauseStrengthenResult::default();
    }
    head.detach(solver);
    let state = explicit_state_from_head_mut(head);
    let Some(index) = state
        .literals
        .as_slice()
        .iter()
        .position(|&lit| lit == literal)
    else {
        head.attach(solver);
        return ClauseStrengthenResult::default();
    };
    state.literals.erase(index);
    let remove_clause = state.literals.len() <= 1;
    if !remove_clause {
        state.sync_head();
        head.attach(solver);
    } else if let Some(&unit) = state.literals.as_slice().first() {
        let _ = solver.force(unit, Antecedent::from_literal(lit_true));
    }
    ClauseStrengthenResult {
        lit_removed: true,
        remove_clause,
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
        let mut temp = [lit_false; 3];
        let prepared = Self::prepare_span(
            solver,
            lits,
            ClauseInfo::default(),
            CLAUSE_FLAG_NONE,
            &mut temp,
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
        let mut temp = [lit_false; 3];
        let prepared = Self::prepare_span(
            solver,
            clause.literals(),
            ClauseInfo::new(ty),
            CLAUSE_FLAG_NONE,
            &mut temp,
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
            result.local = Self::new_unshared(solver, clause, &temp[..2], prepared.info);
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
        } else if !has_flag(flags, CLAUSE_NO_ADD) {
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
        let (head_ptr, constraint_ptr) = build_explicit_clause(&ordered);
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
        let (head_ptr, constraint_ptr) = build_explicit_clause(clause);
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
        temp.assign_from_slice(watched);
        for &lit in clause.literals() {
            if Self::watch_order(solver, lit) > 0 && !temp.as_slice().contains(&lit) {
                temp.push_back(lit);
            }
        }
        let rep = ClauseRep::prepared(temp.as_slice(), info);
        let (head_ptr, _constraint_ptr) = build_explicit_clause(&rep);
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

fn has_flag(flags: CreateFlag, mask: CreateFlag) -> bool {
    (flags & mask) != 0
}
