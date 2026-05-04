//! Partial Rust port of `original_clasp/clasp/cb_enumerator.h` and
//! `original_clasp/src/cb_enumerator.cpp`.
//!
//! This module currently ports the public configuration behavior of
//! `CBConsequences`: type/algorithm selection, enum-option mapping, exhaustive
//! reporting, splitting support, unsat behavior, and the query-mode fallback
//! triggered by optimization, consequence-literal tracking, and current-model
//! extraction. The solver-integrated finder implementations (`CBFinder`,
//! `QueryFinder`) and full `doInit()` integration remain blocked on the still
//! unported enumerator runtime.

use core::ptr::NonNull;

use crate::clasp::clause::SharedLiterals;
use crate::clasp::constraint::{ConstraintType, Solver};
use crate::clasp::enumerator::Model;
pub use crate::clasp::enumerator::{EnumMode, EnumOptions};
use crate::clasp::literal::{LitVec, Literal, ValueVec, true_value, value_free};
use crate::clasp::shared_context::SharedContext;

#[derive(Debug, Default)]
struct SharedConstraint {
    current: Option<NonNull<SharedLiterals>>,
}

impl SharedConstraint {
    fn release(&mut self, new_lits: Option<NonNull<SharedLiterals>>) {
        let prev = self.current.take();
        self.current = new_lits;
        if let Some(prev) = prev {
            unsafe {
                if prev.as_ref().release_one() {
                    drop(Box::from_raw(prev.as_ptr()));
                }
            }
        }
    }

    fn current_clause(&self) -> Option<Vec<Literal>> {
        self.current
            .map(|ptr| unsafe { ptr.as_ref().literals().to_vec() })
    }
}

impl Drop for SharedConstraint {
    fn drop(&mut self) {
        self.release(None);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsequenceModelType {
    Brave = 1,
    Cautious = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsequenceAlgorithm {
    Def,
    Query,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsatType {
    Stop,
    Cont,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsequenceInitWarning {
    QueryDoesNotSupportOptimization,
}

#[derive(Debug)]
pub struct CbConsequences {
    cons: LitVec,
    shared: Option<SharedConstraint>,
    model_type: ConsequenceModelType,
    algorithm: ConsequenceAlgorithm,
}

impl CbConsequences {
    pub fn new(model_type: ConsequenceModelType, algorithm: ConsequenceAlgorithm) -> Self {
        let algorithm = if model_type != ConsequenceModelType::Cautious {
            ConsequenceAlgorithm::Def
        } else {
            algorithm
        };
        Self {
            cons: LitVec::new(),
            shared: None,
            model_type,
            algorithm,
        }
    }

    pub fn from_enum_options(options: EnumOptions) -> Self {
        let model_type = if options.enum_mode == EnumMode::Brave {
            ConsequenceModelType::Brave
        } else {
            ConsequenceModelType::Cautious
        };
        let algorithm = if options.enum_mode != EnumMode::Query {
            ConsequenceAlgorithm::Def
        } else {
            ConsequenceAlgorithm::Query
        };
        Self::new(model_type, algorithm)
    }

    pub fn model_type(&self) -> ConsequenceModelType {
        self.model_type
    }

    pub fn algorithm(&self) -> ConsequenceAlgorithm {
        self.algorithm
    }

    pub fn exhaustive(&self) -> bool {
        true
    }

    pub fn supports_splitting(&self, base_supports_splitting: bool) -> bool {
        self.algorithm == ConsequenceAlgorithm::Def && base_supports_splitting
    }

    pub fn unsat_type(&self, base_unsat_type: UnsatType) -> UnsatType {
        if self.algorithm == ConsequenceAlgorithm::Def {
            base_unsat_type
        } else {
            UnsatType::Cont
        }
    }

    pub fn add_current(
        &mut self,
        solver: &Solver,
        consequence_clause: &mut LitVec,
        model_values: &mut ValueVec,
        root_level: u32,
    ) {
        let step_literal = solver
            .shared_context()
            .expect("CBConsequences::add_current requires an attached shared context")
            .step_literal();
        consequence_clause.assign_fill(1, !step_literal);
        model_values.assign_fill(solver.num_vars() as usize + 1, value_free);
        for literal in self.cons.as_mut_slice() {
            let decision_level = solver.level(literal.var());
            let mut output_state = if decision_level > root_level {
                Model::est_mask(*literal)
            } else {
                0
            };
            match self.model_type {
                ConsequenceModelType::Brave => {
                    if literal.flagged() || solver.is_true(*literal) {
                        literal.flag();
                        output_state = 0;
                    } else if decision_level != 0 {
                        consequence_clause.push_back(*literal);
                    }
                }
                ConsequenceModelType::Cautious => {
                    if !literal.flagged() || solver.is_false(*literal) {
                        literal.unflag();
                        output_state = 0;
                    } else if decision_level != 0 {
                        consequence_clause.push_back(!*literal);
                    }
                }
            }
            if literal.flagged() {
                output_state |= true_value(*literal);
            }
            model_values.as_mut_slice()[literal.var() as usize] |= output_state;
        }
        if let Some(shared) = self.shared.as_mut() {
            let shareable = Box::into_raw(Box::new(SharedLiterals::new_shareable(
                consequence_clause.as_slice(),
                ConstraintType::Other,
                1,
            )));
            shared.release(NonNull::new(shareable));
        }
    }

    pub fn add_lit(&mut self, ctx: &mut SharedContext, literal: Literal) {
        if !ctx.marked(literal) && !ctx.eliminated(literal.var()) {
            self.cons.push_back(literal);
            ctx.set_frozen(literal.var(), true);
            ctx.mark(literal);
        }
    }

    pub fn consequence_literals(&self) -> &[Literal] {
        self.cons.as_slice()
    }

    pub fn set_consequence_literals_for_test(&mut self, literals: &[Literal]) {
        self.cons.assign_from_slice(literals);
    }

    pub fn enable_shared_constraint_for_test(&mut self) {
        if self.shared.is_none() {
            self.shared = Some(SharedConstraint::default());
        }
    }

    pub fn shared_clause_for_test(&self) -> Option<Vec<Literal>> {
        self.shared
            .as_ref()
            .and_then(SharedConstraint::current_clause)
    }

    pub fn prepare_for_init(&mut self, optimize_active: bool) -> Option<ConsequenceInitWarning> {
        if optimize_active && self.algorithm == ConsequenceAlgorithm::Query {
            self.algorithm = ConsequenceAlgorithm::Def;
            return Some(ConsequenceInitWarning::QueryDoesNotSupportOptimization);
        }
        None
    }
}
