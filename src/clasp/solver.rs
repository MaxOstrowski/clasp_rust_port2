//! Public Bundle A solver interface.
//!
//! The current runtime implementation still lives in `constraint.rs` while the
//! port is being split into smaller modules. This module is the stable home for
//! upstream-facing solver types so downstream code can depend on `clasp::solver`
//! instead of the cycle-heavy implementation file.

use crate::clasp::asp_preprocessor::SatPreprocessor;
use crate::clasp::literal::{Literal, ValT, value_free};
use crate::clasp::shared_context::VarInfo;
use crate::clasp::solver_strategies::{Configuration, SearchStrategy, SolveParams, UpdateMode};
use crate::clasp::solver_types::ValueSet;
use crate::potassco::enums::EnumTag;

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
    pub fn sat_prepro(&self) -> Option<&SatPreprocessor> {
        self.shared_context().and_then(|shared| shared.sat_prepro())
    }

    pub fn search_config(&self) -> Option<SolveParams> {
        self.shared_context()
            .map(|shared| *shared.configuration().search(self.id()))
    }

    pub fn search_mode(&self) -> SearchStrategy {
        match self.strategies().search {
            value if value == SearchStrategy::NoLearning as u32 => SearchStrategy::NoLearning,
            _ => SearchStrategy::UseLearning,
        }
    }

    pub fn update_mode(&self) -> UpdateMode {
        UpdateMode::from_underlying(self.strategies().up_mode as u8)
            .unwrap_or(UpdateMode::UpdateOnPropagate)
    }

    pub fn compress_limit(&self) -> u32 {
        match self.strategies().compress {
            0 => u32::MAX,
            value => value,
        }
    }

    pub fn restart_on_model(&self) -> bool {
        self.strategies().restart_on_model != 0
    }

    pub fn var_info(&self, var: u32) -> VarInfo {
        self.shared_context()
            .filter(|shared| shared.valid_var(var))
            .map(|shared| shared.var_info(var))
            .unwrap_or_default()
    }

    pub fn is_master(&self) -> bool {
        self.shared_context()
            .is_some_and(|shared| core::ptr::eq(self, shared.master_ref()))
    }

    pub fn num_aux_vars(&self) -> u32 {
        self.num_vars().saturating_sub(self.num_problem_vars())
    }

    pub fn num_free_vars(&self) -> u32 {
        self.assignment().free().saturating_sub(1)
    }

    pub fn pref(&self, var: u32) -> ValueSet {
        self.assignment().pref(var)
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
