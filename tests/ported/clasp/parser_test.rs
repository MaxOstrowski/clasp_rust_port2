//! Focused Rust tests for the currently ported parser surface.

use std::io::Cursor;

use rust_clasp::clasp::claspfwd::ProblemType;
use rust_clasp::clasp::parser::{
    AspParser, Extension, ParserOptions, ProgramParserApi, detect_problem_type,
};
use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, HeadType, LitSpan, TruthValue, Weight, WeightLitSpan,
};

#[derive(Default)]
struct StepObserver {
    incremental: bool,
    begin_steps: usize,
    end_steps: usize,
    rules: usize,
}

impl AbstractProgram for StepObserver {
    fn init_program(&mut self, incremental: bool) {
        self.incremental = incremental;
    }

    fn begin_step(&mut self) {
        self.begin_steps += 1;
    }

    fn rule(&mut self, _head_type: HeadType, _head: &[Atom], _body: LitSpan<'_>) {
        self.rules += 1;
    }

    fn rule_weighted(
        &mut self,
        _head_type: HeadType,
        _head: &[Atom],
        _bound: Weight,
        _body: WeightLitSpan<'_>,
    ) {
        self.rules += 1;
    }

    fn minimize(&mut self, _priority: Weight, _lits: WeightLitSpan<'_>) {}

    fn output_atom(&mut self, _atom: Atom, _name: &str) {}

    fn external(&mut self, _atom: Atom, _value: TruthValue) {}

    fn end_step(&mut self) {
        self.end_steps += 1;
    }
}

fn empty_smodels_program() -> &'static str {
    "0\n0\nB+\n0\nB-\n1\n0\n1\n"
}

#[test]
fn parser_options_match_upstream_bitmask_helpers() {
    let mut opts = ParserOptions::default();
    assert_eq!(opts.set, 0);
    assert!(!opts.is_enabled(Extension::ParseHeuristic));
    assert!(!opts.any_of(Extension::ParseFull as u8));

    opts.enable_heuristic()
        .enable_acyc_edges()
        .enable_minimize()
        .enable_project()
        .enable_assume()
        .enable_output();
    assert!(opts.is_enabled(Extension::ParseHeuristic));
    assert!(opts.is_enabled(Extension::ParseAcycEdge));
    assert!(opts.is_enabled(Extension::ParseMinimize));
    assert!(opts.is_enabled(Extension::ParseProject));
    assert!(opts.is_enabled(Extension::ParseAssume));
    assert!(opts.is_enabled(Extension::ParseOutput));
    assert!(opts.any_of(Extension::ParseFull as u8));

    opts.assign(Extension::ParseProject as u8, false)
        .assign(Extension::ParseMaxsat as u8, true);
    assert!(!opts.is_enabled(Extension::ParseProject));
    assert!(opts.is_enabled(Extension::ParseMaxsat));

    let copy = opts;
    assert_eq!(copy, opts);
}

#[test]
fn detect_problem_type_matches_first_non_whitespace_marker() {
    assert_eq!(detect_problem_type(b"  \t\n1 0 0\n"), ProblemType::Asp);
    assert_eq!(detect_problem_type(b"\nc comment\n"), ProblemType::Sat);
    assert_eq!(
        detect_problem_type(b"\n* #variable= 1 #constraint= 0\n"),
        ProblemType::Pb
    );
    assert_eq!(detect_problem_type(b"\na\n"), ProblemType::Asp);
}

#[test]
fn asp_parser_accept_char_matches_upstream_dispatch() {
    assert!(AspParser::accept_char('0'));
    assert!(AspParser::accept_char('9'));
    assert!(AspParser::accept_char('a'));
    assert!(!AspParser::accept_char('c'));
    assert!(!AspParser::accept_char('*'));
}

#[test]
fn asp_parser_accepts_smodels_and_tracks_parser_state() {
    let mut observer = StepObserver::default();
    let mut parser = AspParser::new(&mut observer);

    assert!(!parser.is_open());
    assert!(!parser.incremental());
    assert!(parser.accept(
        &mut Cursor::new(empty_smodels_program().as_bytes()),
        ParserOptions::default(),
    ));
    assert!(parser.is_open());
    assert!(!parser.incremental());
    assert!(parser.parse());
    assert!(!parser.more());

    parser.reset();
    assert!(!parser.is_open());
}

#[test]
fn asp_parser_accepts_incremental_text_and_exposes_more_steps() {
    let mut observer = StepObserver::default();
    let mut parser = AspParser::new(&mut observer);
    let program = concat!(
        "asp 1 0 0 incremental\n",
        "1 0 1 1 0 0\n",
        "0\n",
        "1 0 1 2 0 0\n",
        "0\n"
    );

    assert!(parser.accept(
        &mut Cursor::new(program.as_bytes()),
        ParserOptions::default(),
    ));
    assert!(parser.incremental());
    assert!(parser.parse());
    assert!(parser.more());
    assert!(parser.parse());
    assert!(!parser.more());

    parser.reset();
    assert!(!parser.is_open());
}

#[test]
fn asp_parser_rejects_non_asp_input() {
    let mut observer = StepObserver::default();
    let mut parser = AspParser::new(&mut observer);

    assert!(!parser.accept(
        &mut Cursor::new(b"c comment\np cnf 0 0\n".as_slice()),
        ParserOptions::default(),
    ));
    assert!(!parser.is_open());
}
