//! Partial Rust port of `original_clasp/clasp/solve_algorithms.h` and
//! `original_clasp/src/solve_algorithms.cpp`.

use crate::clasp::literal::{LitView, Literal};
use crate::clasp::solver::Solver;
use crate::clasp::solver_strategies::SolveEvent;
use crate::clasp::util::misc_types::TaggedPtr;
use crate::clasp::util::misc_types::{EventLike, Verbosity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolveLimits {
    pub conflicts: u64,
    pub restarts: u64,
}

impl SolveLimits {
    pub const fn new(conflicts: u64, restarts: u64) -> Self {
        Self {
            conflicts,
            restarts,
        }
    }

    pub const fn reached(self) -> bool {
        self.conflicts == 0 || self.restarts == 0
    }

    pub const fn enabled(self) -> bool {
        self.conflicts != u64::MAX || self.restarts != u64::MAX
    }
}

impl Default for SolveLimits {
    fn default() -> Self {
        Self::new(u64::MAX, u64::MAX)
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BasicSolveEventOp {
    None = 0,
    Deletion = 'D' as u32,
    Exit = 'E' as u32,
    Grow = 'G' as u32,
    Restart = 'R' as u32,
}

#[derive(Clone, Copy, Debug)]
pub struct BasicSolveEvent {
    pub base: SolveEvent,
    pub c_limit: u64,
    pub l_limit: u32,
}

impl BasicSolveEvent {
    pub fn new(solver: &Solver, op: BasicSolveEventOp, c_limit: u64, l_limit: u32) -> Self {
        let mut base = SolveEvent::new::<Self>(solver, Verbosity::VerbosityMax);
        base.base.op = op as u32;
        Self {
            base,
            c_limit,
            l_limit,
        }
    }
}

impl EventLike for BasicSolveEvent {
    fn event(&self) -> &crate::clasp::util::misc_types::Event {
        &self.base.base
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[derive(Debug, Default)]
pub struct Path {
    lits: TaggedPtr<Literal>,
    size: usize,
}

impl Path {
    pub fn new() -> Self {
        Self {
            lits: TaggedPtr::new(std::ptr::null_mut()),
            size: 0,
        }
    }

    pub fn acquire(path: LitView<'_>) -> Self {
        if path.is_empty() {
            let mut owned = Self::new();
            owned.lits.set::<0>();
            return owned;
        }
        let mut owned = path.to_vec().into_boxed_slice();
        let ptr = owned.as_mut_ptr();
        std::mem::forget(owned);
        let mut lits = TaggedPtr::new(ptr);
        lits.set::<0>();
        Self {
            lits,
            size: path.len(),
        }
    }

    pub fn borrow(path: LitView<'_>) -> Self {
        if path.is_empty() {
            return Self::new();
        }
        Self {
            lits: TaggedPtr::new(path.as_ptr() as *mut Literal),
            size: path.len(),
        }
    }

    pub fn begin(&self) -> *const Literal {
        self.lits.get() as *const Literal
    }

    pub fn end(&self) -> *const Literal {
        self.begin().wrapping_add(self.size)
    }

    pub fn owner(&self) -> bool {
        self.lits.test::<0>()
    }

    /// # Safety
    ///
    /// For borrowed paths, the caller must ensure the original literal storage
    /// remains alive for the returned view and that no mutable aliasing occurs
    /// while the view is used.
    pub unsafe fn as_lit_view<'a>(&self) -> LitView<'a> {
        if self.size == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.begin(), self.size) }
        }
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        if self.owner() && self.size != 0 {
            let raw = std::ptr::slice_from_raw_parts_mut(self.lits.get(), self.size);
            unsafe {
                drop(Box::from_raw(raw));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequentialSolve {
    limit: SolveLimits,
    term: i32,
}

impl SequentialSolve {
    pub const fn new(limit: SolveLimits) -> Self {
        Self { limit, term: -1 }
    }

    pub const fn interrupted(&self) -> bool {
        self.term > 0
    }

    pub const fn solve_limit(&self) -> SolveLimits {
        self.limit
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BasicSolveOptions {
    pub limit: SolveLimits,
}

impl BasicSolveOptions {
    pub fn create_solve_object(&self) -> SequentialSolve {
        SequentialSolve::new(self.limit)
    }

    pub const fn supported_solvers() -> u32 {
        1
    }

    pub const fn recommended_solvers() -> u32 {
        1
    }

    pub const fn num_solver(&self) -> u32 {
        1
    }

    pub fn set_solvers(&mut self, _num_solvers: u32) {}

    pub const fn default_portfolio(&self) -> bool {
        false
    }
}
