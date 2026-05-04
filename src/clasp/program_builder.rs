//! Partial Rust port target for `original_clasp/clasp/program_builder.h` and
//! `original_clasp/src/program_builder.cpp`.
//!
//! Bundle B scope:
//! - API boundary only
//! - no solver/runtime implementation

use std::io::Read;

use crate::clasp::claspfwd::{ProblemType, SharedContext};
use crate::clasp::literal::Literal;
use crate::clasp::parser::ProgramParserApi;

pub type LitVec = Vec<Literal>;
pub type SumVec = Vec<i64>;

pub trait ProgramBuilder {
    fn start_program(&mut self, ctx: &mut SharedContext) -> bool;
    fn parse_program<R: Read>(&mut self, input: R) -> bool;
    fn update_program(&mut self) -> bool;
    fn end_program(&mut self) -> bool;

    fn get_assumptions(&self, out: &mut LitVec);
    fn get_weak_bounds(&self, out: &mut SumVec);

    fn do_type(&self) -> ProblemType;

    fn problem_type(&self) -> ProblemType {
        self.do_type()
    }

    fn parser(&mut self) -> &mut (dyn ProgramParserApi + '_);

    fn r#type(&self) -> ProblemType {
        self.do_type()
    }

    fn do_get_weak_bounds(&self, _out: &mut SumVec) {}
}
