//! Partial Rust port of `original_clasp/clasp/clause.h` and `original_clasp/src/clause.cpp`.
//!
//! This module currently covers `SharedLiterals` plus the solver-state-only
//! `ClauseCreator` helper logic for watch ordering, preparation, status
//! classification, and clause-ignore decisions. Clause runtime objects and
//! solver integration remain unported.

use core::ptr::NonNull;

use crate::clasp::constraint::{ConstraintInfo, ConstraintType, Solver};
use crate::clasp::literal::{
    LitVec, LitView, Literal, lit_false, lit_true, true_value, value_free, var_max,
};
use crate::clasp::pod_vector::{shrink_vec_to, size32};
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
