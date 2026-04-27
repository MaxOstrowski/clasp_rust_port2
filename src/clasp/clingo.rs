//! Partial Rust port of `original_clasp/clasp/clingo.h`.
//!
//! This module currently ports the `ClingoAssignment` adapter from
//! `original_clasp/src/clingo.cpp`. The propagator and heuristic adapters remain
//! blocked on the still-unported concrete solver and post-propagator wiring.

use std::panic::panic_any;

use crate::clasp::literal::{
    decode_var, encode_lit, lit_true, value_false, value_free, value_true,
};
use crate::clasp::solver::Solver;
use crate::potassco::basic_types::TruthValue;
use crate::potassco::clingo::AbstractAssignment;
use crate::potassco::error::Error;

const TRAIL_OFFSET: u32 = 1;

pub struct ClingoAssignment<'a> {
    solver: &'a Solver,
}

impl<'a> ClingoAssignment<'a> {
    pub fn new(solver: &'a Solver) -> Self {
        Self { solver }
    }

    pub fn solver(&self) -> &'a Solver {
        self.solver
    }
}

impl AbstractAssignment for ClingoAssignment<'_> {
    fn solver_id(&self) -> u32 {
        self.solver.id()
    }

    fn size(&self) -> u32 {
        self.solver.num_vars().max(self.solver.num_problem_vars()) + TRAIL_OFFSET
    }

    fn unassigned(&self) -> u32 {
        self.size() - self.trail_size()
    }

    fn has_conflict(&self) -> bool {
        self.solver.has_conflict()
    }

    fn level(&self) -> u32 {
        self.solver.decision_level()
    }

    fn root_level(&self) -> u32 {
        self.solver.root_level()
    }

    fn has_lit(&self, lit: i32) -> bool {
        decode_var(lit) < self.size()
    }

    fn value(&self, lit: i32) -> TruthValue {
        if !self.has_lit(lit) {
            panic_any(Error::InvalidArgument("Invalid literal".to_owned()));
        }

        let var = decode_var(lit);
        let value = if self.solver.valid_var(var) {
            self.solver.value(var)
        } else {
            value_free
        };
        if value == value_true {
            if lit >= 0 {
                TruthValue::True
            } else {
                TruthValue::False
            }
        } else if value == value_false {
            if lit >= 0 {
                TruthValue::False
            } else {
                TruthValue::True
            }
        } else {
            TruthValue::Free
        }
    }

    fn level_of(&self, lit: i32) -> u32 {
        if self.value(lit) != TruthValue::Free {
            self.solver.level(decode_var(lit))
        } else {
            u32::MAX
        }
    }

    fn decision(&self, level: u32) -> i32 {
        if level > self.solver.decision_level() {
            panic_any(Error::InvalidArgument("Invalid decision level".to_owned()));
        }
        encode_lit(if level != 0 {
            self.solver.decision(level)
        } else {
            lit_true
        })
    }

    fn trail_size(&self) -> u32 {
        self.solver.num_assigned_vars() + TRAIL_OFFSET
    }

    fn trail_at(&self, pos: u32) -> i32 {
        if pos >= self.trail_size() {
            panic_any(Error::InvalidArgument("Invalid trail position".to_owned()));
        }
        encode_lit(if pos != 0 {
            self.solver.trail_lit(pos - TRAIL_OFFSET)
        } else {
            lit_true
        })
    }

    fn trail_begin(&self, level: u32) -> u32 {
        if level > self.solver.decision_level() {
            panic_any(Error::InvalidArgument("Invalid decision level".to_owned()));
        }
        if level != 0 {
            self.solver.level_start(level) + TRAIL_OFFSET
        } else {
            0
        }
    }
}
