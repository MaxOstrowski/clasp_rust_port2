//! Partial Rust port of `original_clasp/clasp/enumerator.h` and
//! `original_clasp/src/enumerator.cpp`.
//!
//! This module currently ports the solver-independent `Model` data structure,
//! the projected-output consequence counting logic, and `modelType()` text
//! mapping. Solver/shared-context initialization, enumeration constraints,
//! queue handling, and commit/update integration remain blocked on the still
//! unported concrete solver, shared context, minimize constraint, and clause
//! runtime.

use crate::clasp::cli::clasp_cli_options::ProjectMode;
use crate::clasp::literal::{
    Literal, SumVec, SumView, ValT, ValueView, Var_t, pos_lit, true_value, value_false, value_free,
    value_true,
};
use crate::clasp::solver_strategies::LowerBound;
use crate::potassco::bits;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelType {
    Sat = 0,
    Brave = 1,
    Cautious = 2,
    User = 4,
}

impl ModelType {
    pub const CONS_MASK: u32 = 3;
    pub const EST_MASK: u32 = 4;

    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputPredicate<'a> {
    pub cond: Literal,
    pub name: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputProjection<'a> {
    pub project_mode: ProjectMode,
    pub predicates: &'a [OutputPredicate<'a>],
    pub vars: &'a [Var_t],
    pub projected: &'a [Literal],
}

impl<'a> OutputProjection<'a> {
    pub const fn new(
        project_mode: ProjectMode,
        predicates: &'a [OutputPredicate<'a>],
        vars: &'a [Var_t],
        projected: &'a [Literal],
    ) -> Self {
        Self {
            project_mode,
            predicates,
            vars,
            projected,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Model<'a> {
    pub num: u64,
    pub values: ValueView<'a>,
    pub costs: SumView<'a>,
    pub lower: LowerBound,
    pub solver_id: u16,
    pub model_type: u32,
    pub opt: bool,
    pub def: bool,
    pub sym: bool,
    pub up: bool,
    pub fin: bool,
    pub lb: bool,
}

impl<'a> Model<'a> {
    pub const fn with_values(values: ValueView<'a>) -> Self {
        Self {
            values,
            sym: true,
            ..Self::new()
        }
    }

    pub const fn new() -> Self {
        Self {
            num: 0,
            values: &[],
            costs: &[],
            lower: LowerBound {
                level: 0,
                bound: crate::clasp::literal::weight_sum_min,
            },
            solver_id: 0,
            model_type: ModelType::Sat as u32,
            opt: false,
            def: false,
            sym: false,
            up: false,
            fin: false,
            lb: false,
        }
    }

    pub fn set_costs(&mut self, costs: &'a SumVec) {
        self.costs = costs.as_slice();
    }

    pub const fn has_var(&self, var: Var_t) -> bool {
        (var as usize) < self.values.len()
    }

    pub const fn has_costs(&self) -> bool {
        !self.costs.is_empty()
    }

    pub fn consequences(&self) -> bool {
        bits::test_any(self.model_type, ModelType::CONS_MASK)
    }

    pub fn value(&self, var: Var_t) -> ValT {
        assert!(self.has_var(var));
        self.values[var as usize] & 3u8
    }

    pub fn is_true(&self, lit: Literal) -> bool {
        bits::test_any(self.value(lit.var()) as u32, true_value(lit) as u32)
    }

    pub fn is_def(&self, lit: Literal) -> bool {
        self.is_true(lit)
            && (self.def
                || !bits::test_any(self.model_type, ModelType::Cautious as u32)
                || !self.is_est(lit))
    }

    pub fn is_est(&self, lit: Literal) -> bool {
        assert!(self.has_var(lit.var()));
        !self.def
            && bits::test_any(
                self.values[lit.var() as usize] as u32,
                Self::est_mask(lit) as u32,
            )
    }

    pub fn is_cons(&self, lit: Literal) -> ValT {
        if self.is_est(lit) {
            value_free
        } else if self.is_true(lit) {
            value_true
        } else {
            value_false
        }
    }

    pub fn num_consequences(&self, output: &OutputProjection<'_>) -> (u32, u32) {
        let mut low = 0u32;
        let mut est = 0u32;
        let mut count = |lit: Literal| {
            let cons = self.is_cons(lit);
            if cons == value_true {
                low += 1;
            } else if cons == value_free {
                est += 1;
            }
        };
        if output.project_mode == ProjectMode::Output {
            for pred in output.predicates {
                count(pred.cond);
            }
            for &var in output.vars {
                count(pos_lit(var));
            }
        } else {
            for &lit in output.projected {
                count(lit);
            }
        }
        debug_assert!(est == 0 || !self.def);
        (low, if self.def { 0 } else { est })
    }

    pub const fn est_mask(lit: Literal) -> u8 {
        (ModelType::EST_MASK as u8) << lit.sign() as u8
    }
}

pub const fn model_type(model: &Model<'_>) -> &'static str {
    match model.model_type & ModelType::CONS_MASK {
        x if x == ModelType::Brave as u32 => "Brave consequences",
        x if x == ModelType::Cautious as u32 => "Cautious consequences",
        _ => "Model",
    }
}
