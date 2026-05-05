use std::io::Read;
use std::ptr::NonNull;

use rust_clasp::clasp::claspfwd::{ProblemType, SharedContext};
use rust_clasp::clasp::parser::{ParserOptions, ProgramParserApi};
use rust_clasp::clasp::program_builder::{LitVec, ProgramBuilder, ProgramBuilderState, SumVec};

#[derive(Default)]
struct DummyParser;

impl ProgramParserApi for DummyParser {
    fn accept(&mut self, _input: &mut dyn Read, _opts: ParserOptions) -> bool {
        true
    }

    fn incremental(&self) -> bool {
        false
    }

    fn is_open(&self) -> bool {
        false
    }

    fn parse(&mut self) -> bool {
        true
    }

    fn more(&mut self) -> bool {
        false
    }

    fn reset(&mut self) {}
}

struct DummyBuilder {
    state: ProgramBuilderState,
    parser: DummyParser,
}

impl Default for DummyBuilder {
    fn default() -> Self {
        Self {
            state: ProgramBuilderState::default(),
            parser: DummyParser,
        }
    }
}

impl ProgramBuilder for DummyBuilder {
    fn state(&self) -> &ProgramBuilderState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ProgramBuilderState {
        &mut self.state
    }

    fn start_program(&mut self, _ctx: &mut SharedContext) -> bool {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn parse_program<R: Read>(&mut self, _input: R) -> bool {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn update_program(&mut self) -> bool {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn end_program(&mut self) -> bool {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn get_assumptions(&self, _out: &mut LitVec) {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn get_weak_bounds(&self, _out: &mut SumVec) {
        unreachable!("not needed for do_get_weak_bounds test")
    }

    fn do_type(&self) -> ProblemType {
        ProblemType::Asp
    }

    fn parser(&mut self) -> &mut (dyn ProgramParserApi + '_) {
        &mut self.parser
    }
}

#[test]
fn program_builder_default_do_get_weak_bounds_is_noop() {
    let builder = DummyBuilder::default();
    let mut bounds = vec![1, 2, 3];
    builder.do_get_weak_bounds(&mut bounds);
    assert_eq!(bounds, vec![1, 2, 3]);
}

#[test]
fn program_builder_type_delegates_to_problem_type_hook() {
    let builder = DummyBuilder::default();
    assert_eq!(builder.r#type(), ProblemType::Asp);
}

#[test]
fn program_builder_problem_type_delegates_to_do_type_hook() {
    let builder = DummyBuilder::default();
    assert_eq!(builder.problem_type(), ProblemType::Asp);
}

#[test]
fn program_builder_state_defaults_match_upstream_constructor() {
    let builder = DummyBuilder::default();
    assert!(builder.frozen());
    assert!(builder.ctx().is_none());
    assert!(!builder.ok());
    assert!(!builder.state().has_parser());
}

#[test]
fn program_builder_ctx_and_ok_follow_stored_context_pointer() {
    let mut builder = DummyBuilder::default();
    let mut ctx = SharedContext::default();

    builder.set_ctx(Some(NonNull::from(&mut ctx)));

    let stored = builder.ctx().expect("context should be stored");
    assert!(std::ptr::eq(stored, &ctx));
    assert_eq!(builder.ok(), ctx.ok());

    builder.set_ctx(None);
    assert!(builder.ctx().is_none());
    assert!(!builder.ok());
}

#[test]
fn program_builder_set_frozen_updates_stored_flag() {
    let mut builder = DummyBuilder::default();
    builder.set_frozen(false);
    assert!(!builder.frozen());
    builder.set_frozen(true);
    assert!(builder.frozen());
}
