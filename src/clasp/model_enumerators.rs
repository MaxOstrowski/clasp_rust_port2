//! Partial Rust port of `original_clasp/clasp/model_enumerators.h`.
//!
//! This module currently ports the public configuration behavior of
//! `ModelEnumerator`: strategy and projection option selection, the normalized
//! projection flag handling from `setStrategy()`, projection membership storage,
//! support predicates, and the solver-independent part of the auto-strategy
//! selection performed during initialization.
//!
//! The actual finder implementations, projection discovery from `SharedContext`,
//! domain-heuristic integration, solver callbacks, and clause/minimization
//! interactions remain blocked on the still-unported enumerator, solver, shared
//! context, clause, and minimize-constraint infrastructure.

use crate::clasp::literal::Var_t;
use crate::potassco::bits;
use crate::potassco::utils::DynamicBitset;

use super::cb_enumerator::{EnumMode, EnumOptions};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Strategy {
    Auto = 0,
    Backtrack = 1,
    Record = 2,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectOptions {
    EnableSimple = 1,
    UseHeuristic = 2,
    SaveProgress = 4,
    ShownAll = 8,
    DomLits = 16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelEnumeratorInitWarning {
    ProjectionMayDependOnEnumerationOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitConfig {
    pub strategy: Strategy,
    pub trivial: bool,
    pub warning: Option<ModelEnumeratorInitWarning>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Options {
    proj: u32,
    algo: Strategy,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            proj: 0,
            algo: Strategy::Auto,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModelEnumerator {
    project: DynamicBitset,
    filter: char,
    opts: Options,
    saved: Options,
    trivial: bool,
}

impl Default for ModelEnumerator {
    fn default() -> Self {
        Self::new(Strategy::Auto)
    }
}

impl ModelEnumerator {
    pub fn new(strategy: Strategy) -> Self {
        let mut enumerator = Self {
            project: DynamicBitset::default(),
            filter: '_',
            opts: Options::default(),
            saved: Options::default(),
            trivial: false,
        };
        enumerator.set_strategy(strategy, 0, '_');
        enumerator
    }

    pub fn from_enum_options(options: EnumOptions, project: u32) -> Self {
        let mut enumerator = Self::default();
        let strategy = match options.enum_mode {
            EnumMode::Bt => Strategy::Backtrack,
            EnumMode::Record | EnumMode::DomRecord => Strategy::Record,
            _ => Strategy::Auto,
        };
        let project = if options.enum_mode == EnumMode::DomRecord {
            project | ProjectOptions::DomLits as u32
        } else {
            project
        };
        enumerator.set_strategy(strategy, project, '_');
        enumerator
    }

    pub fn set_strategy(&mut self, strategy: Strategy, projection: u32, filter: char) {
        self.opts.algo = strategy;
        self.opts.proj = projection;
        self.filter = filter;
        if bits::test_any(
            projection,
            (ProjectOptions::EnableSimple as u32)
                | (ProjectOptions::UseHeuristic as u32)
                | (ProjectOptions::SaveProgress as u32),
        ) {
            self.opts.proj |= ProjectOptions::EnableSimple as u32;
        }
        self.saved = self.opts;
    }

    pub fn projection_enabled(&self) -> bool {
        self.project_options() != 0
    }

    pub fn dom_rec(&self) -> bool {
        bits::test_any(self.project_options(), ProjectOptions::DomLits as u32)
    }

    pub fn strategy(&self) -> Strategy {
        self.opts.algo
    }

    pub fn filter(&self) -> char {
        self.filter
    }

    pub fn project_options(&self) -> u32 {
        self.opts.proj
    }

    pub fn trivial(&self) -> bool {
        self.trivial
    }

    pub fn project(&self, var: Var_t) -> bool {
        self.project.contains(var)
    }

    pub fn add_project(&mut self, var: Var_t) -> bool {
        self.project.add(var)
    }

    pub fn clear_project(&mut self) {
        self.project.clear();
    }

    pub fn supports_restarts(&self, optimize_active: bool) -> bool {
        optimize_active || self.strategy() == Strategy::Record
    }

    pub fn supports_parallel(&self) -> bool {
        !self.projection_enabled() || self.strategy() != Strategy::Backtrack
    }

    pub fn supports_splitting(&self, base_supports_splitting: bool) -> bool {
        (self.strategy() == Strategy::Backtrack || !self.dom_rec()) && base_supports_splitting
    }

    pub fn prepare_for_init(
        &mut self,
        optimize_active: bool,
        num_models: i32,
        concurrency: u32,
        optimization_projection_trivial: bool,
    ) -> InitConfig {
        self.opts = self.saved;
        if concurrency > 1 && !self.supports_parallel() {
            self.opts.algo = Strategy::Auto;
        }

        let mut trivial = (optimize_active && !self.dom_rec()) || num_models.abs() == 1;
        let mut warning = None;
        if optimize_active && self.projection_enabled() {
            trivial = trivial && optimization_projection_trivial;
            if !trivial && !optimization_projection_trivial {
                warning = Some(ModelEnumeratorInitWarning::ProjectionMayDependOnEnumerationOrder);
            }
        }

        if self.opts.algo == Strategy::Auto {
            self.opts.algo = if trivial || (self.projection_enabled() && concurrency > 1) {
                Strategy::Record
            } else {
                Strategy::Backtrack
            };
        }
        self.trivial = trivial;

        InitConfig {
            strategy: self.opts.algo,
            trivial,
            warning,
        }
    }
}
