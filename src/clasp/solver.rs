//! Public Bundle A solver interface.
//!
//! The current runtime implementation still lives in `constraint.rs` while the
//! port is being split into smaller modules. This module is the stable home for
//! upstream-facing solver types so downstream code can depend on `clasp::solver`
//! instead of the cycle-heavy implementation file.

use crate::clasp::literal::{Literal, ValT, value_free};

pub use crate::clasp::constraint::{
    Antecedent, CCMinRecursive, DecisionHeuristic, PostPropagator, PostPropagatorDyn, SelectFirst,
    Solver, priority_class_general,
};

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndoMode {
    Default = 0,
    PopBtLevel = 1,
    PopProjLevel = 2,
    SavePhases = 4,
}

impl UndoMode {
    const MODE_MASK: u32 = 3;

    const fn as_u32(self) -> u32 {
        self as u32
    }

    const fn mode_bits(self) -> u32 {
        self.as_u32() & Self::MODE_MASK
    }
}

impl Solver {
    pub fn num_aux_vars(&self) -> u32 {
        self.num_vars().saturating_sub(self.num_problem_vars())
    }

    pub fn num_free_vars(&self) -> u32 {
        self.assignment().free().saturating_sub(1)
    }

    pub fn set_backtrack_level_with_mode(&mut self, level: u32, mode: UndoMode) {
        if mode.as_u32() >= self.backtrack_mode {
            self.set_backtrack_level(level);
            self.backtrack_mode = self
                .backtrack_mode
                .max(mode.mode_bits().max(UndoMode::PopBtLevel.as_u32()));
        }
    }

    pub fn top_value(&self, var: u32) -> ValT {
        if self.level(var) == 0 {
            self.value(var)
        } else {
            value_free
        }
    }

    pub fn valid_level(&self, level: u32) -> bool {
        level != 0 && level <= self.decision_level()
    }

    pub fn jump_level(&self) -> u32 {
        self.current_undo_target().unwrap_or(self.decision_level())
    }

    pub fn reason_literal(&self, literal: Literal) -> &Antecedent {
        debug_assert!(self.is_true(literal));
        self.reason(literal.var())
    }

    pub fn reason_data_literal(&self, literal: Literal) -> u32 {
        self.reason_data(literal.var())
    }

    pub fn level_lits(&self, level: u32) -> &[Literal] {
        if !self.valid_level(level) {
            return &[];
        }
        let trail = self.assignment().trail.as_slice();
        let start = self.level_start(level) as usize;
        let end = if level < self.decision_level() {
            self.level_start(level + 1) as usize
        } else {
            trail.len()
        };
        &trail[start..end]
    }

    pub fn trail_view(&self, offset: u32) -> &[Literal] {
        self.assignment()
            .trail
            .get(offset as usize..)
            .unwrap_or(&[])
    }
}
