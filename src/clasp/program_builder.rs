//! Partial Rust port target for `original_clasp/clasp/program_builder.h` and
//! `original_clasp/src/program_builder.cpp`.
//!
//! Bundle B scope:
//! - API boundary only
//! - no solver/runtime implementation

use std::io::Read;
use std::ptr::NonNull;

use crate::clasp::claspfwd::{ProblemType, SharedContext};
use crate::clasp::literal::Literal;
use crate::clasp::parser::ProgramParserApi;

pub type LitVec = Vec<Literal>;
pub type SumVec = Vec<i64>;

pub struct ProgramBuilderState {
    ctx: Option<NonNull<SharedContext>>,
    parser: Option<Box<dyn ProgramParserApi>>,
    frozen: bool,
}

impl Default for ProgramBuilderState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgramBuilderState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ctx: None,
            parser: None,
            frozen: true,
        }
    }

    #[must_use]
    pub fn ctx(&self) -> Option<&SharedContext> {
        self.ctx.map(|ctx| {
            // SAFETY: `ctx` is only created from a live `&mut SharedContext` supplied
            // by the caller and mirrors the upstream raw-pointer member.
            unsafe { ctx.as_ref() }
        })
    }

    #[must_use]
    pub fn ctx_mut(&mut self) -> Option<&mut SharedContext> {
        self.ctx.map(|mut ctx| {
            // SAFETY: access is gated through `&mut self`, matching the unique
            // ownership assumptions of the upstream builder state.
            unsafe { ctx.as_mut() }
        })
    }

    #[must_use]
    pub const fn frozen(&self) -> bool {
        self.frozen
    }

    #[must_use]
    pub fn ok(&self) -> bool {
        self.ctx().is_some_and(SharedContext::ok)
    }

    pub fn set_ctx(&mut self, ctx: Option<NonNull<SharedContext>>) {
        self.ctx = ctx;
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    #[must_use]
    pub const fn has_parser(&self) -> bool {
        self.parser.is_some()
    }

    pub fn set_parser(&mut self, parser: Box<dyn ProgramParserApi>) {
        self.parser = Some(parser);
    }

    pub fn parser_mut(&mut self) -> Option<&mut (dyn ProgramParserApi + 'static)> {
        self.parser.as_deref_mut()
    }

    pub fn clear_parser(&mut self) {
        self.parser = None;
    }
}

pub trait ProgramBuilder {
    fn state(&self) -> &ProgramBuilderState;
    fn state_mut(&mut self) -> &mut ProgramBuilderState;

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

    fn frozen(&self) -> bool {
        self.state().frozen()
    }

    fn ok(&self) -> bool {
        self.state().ok()
    }

    fn ctx(&self) -> Option<&SharedContext> {
        self.state().ctx()
    }

    fn set_ctx(&mut self, ctx: Option<NonNull<SharedContext>>) {
        self.state_mut().set_ctx(ctx);
    }

    fn set_frozen(&mut self, frozen: bool) {
        self.state_mut().set_frozen(frozen);
    }
}
