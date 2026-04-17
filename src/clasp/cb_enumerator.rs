//! Partial Rust port of `original_clasp/clasp/cb_enumerator.h` and
//! `original_clasp/src/cb_enumerator.cpp`.
//!
//! This module currently ports the public configuration behavior of
//! `CBConsequences`: type/algorithm selection, enum-option mapping, exhaustive
//! reporting, splitting support, unsat behavior, and the query-mode fallback
//! triggered by optimization. The solver-integrated finder implementations
//! (`CBFinder`, `QueryFinder`, shared literals, and model extraction/update
//! logic) remain blocked on the still-unported enumerator, solver, shared
//! context, and clause infrastructure.

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

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnumMode {
    Auto = 0,
    Bt = 1,
    Record = 2,
    DomRecord = 3,
    Consequences = 4,
    Brave = 5,
    Cautious = 6,
    Query = 7,
    User = 8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnumOptions {
    pub enum_mode: EnumMode,
}

impl Default for EnumOptions {
    fn default() -> Self {
        Self {
            enum_mode: EnumMode::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CbConsequences {
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

    pub fn model_type(self) -> ConsequenceModelType {
        self.model_type
    }

    pub fn algorithm(self) -> ConsequenceAlgorithm {
        self.algorithm
    }

    pub fn exhaustive(self) -> bool {
        true
    }

    pub fn supports_splitting(self, base_supports_splitting: bool) -> bool {
        self.algorithm == ConsequenceAlgorithm::Def && base_supports_splitting
    }

    pub fn unsat_type(self, base_unsat_type: UnsatType) -> UnsatType {
        if self.algorithm == ConsequenceAlgorithm::Def {
            base_unsat_type
        } else {
            UnsatType::Cont
        }
    }

    pub fn prepare_for_init(&mut self, optimize_active: bool) -> Option<ConsequenceInitWarning> {
        if optimize_active && self.algorithm == ConsequenceAlgorithm::Query {
            self.algorithm = ConsequenceAlgorithm::Def;
            return Some(ConsequenceInitWarning::QueryDoesNotSupportOptimization);
        }
        None
    }
}
