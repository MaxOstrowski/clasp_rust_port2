//! Partial Rust port of `original_clasp/clasp/parser.h` and `original_clasp/src/parser.cpp`.
//!
//! Bundle B scope:
//! - Parser framework and input-type detection.
//! - Keeps this module independent from `clasp::logic_program` runtime.

use std::io::{Cursor, Read};

use crate::clasp::claspfwd::ProblemType;
use crate::potassco::aspif_text::AspifTextInput;
use crate::potassco::basic_types::AbstractProgram;
use crate::potassco::match_basic_types::{ProgramReader, ReadMode, is_digit};
use crate::potassco::smodels::{SmodelsInput, SmodelsOptions};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Extension {
    ParseHeuristic = 1,
    ParseAcycEdge = 2,
    ParseMinimize = 4,
    ParseProject = 8,
    ParseAssume = 16,
    ParseOutput = 32,
    ParseFull = 63,
    ParseMaxsat = 128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParserOptions {
    pub set: u8,
}

impl ParserOptions {
    #[must_use]
    pub const fn is_enabled(self, e: Extension) -> bool {
        (self.set & (e as u8)) != 0
    }

    #[must_use]
    pub const fn any_of(self, mask: u8) -> bool {
        (self.set & mask) != 0
    }

    pub fn enable_heuristic(&mut self) -> &mut Self {
        self.enable(Extension::ParseHeuristic)
    }

    pub fn enable_acyc_edges(&mut self) -> &mut Self {
        self.enable(Extension::ParseAcycEdge)
    }

    pub fn enable_minimize(&mut self) -> &mut Self {
        self.enable(Extension::ParseMinimize)
    }

    pub fn enable_project(&mut self) -> &mut Self {
        self.enable(Extension::ParseProject)
    }

    pub fn enable_assume(&mut self) -> &mut Self {
        self.enable(Extension::ParseAssume)
    }

    pub fn enable_output(&mut self) -> &mut Self {
        self.enable(Extension::ParseOutput)
    }

    pub fn assign(&mut self, mask: u8, enabled: bool) -> &mut Self {
        if enabled {
            self.set |= mask;
        } else {
            self.set &= !mask;
        }
        self
    }

    pub fn enable(&mut self, e: Extension) -> &mut Self {
        self.set |= e as u8;
        self
    }
}

/// Detects the input format of a program.
///
/// This function mirrors the upstream behavior of `detectProblemType(std::istream&)`.
///
/// Note: unlike C++, we operate on an already buffered byte slice.
pub fn detect_problem_type(input: &[u8]) -> ProblemType {
    let mut line = 1usize;
    let mut col = 1usize;
    for &b in input {
        let c = b as char;
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            continue;
        }
        if is_digit(c) || c == 'a' {
            return ProblemType::Asp;
        }
        if c == 'c' || c == 'p' {
            return ProblemType::Sat;
        }
        if c == '*' {
            return ProblemType::Pb;
        }
        panic!("parse error in line {line}:{col}: <{c}>: unrecognized input format");
    }
    panic!("parse error in line {line}:{col}: <eof>: unrecognized input format");
}

pub trait ProgramParserApi {
    fn accept(&mut self, input: &mut dyn Read, opts: ParserOptions) -> bool;
    fn incremental(&self) -> bool;
    fn is_open(&self) -> bool;
    fn parse(&mut self) -> bool;
    fn more(&mut self) -> bool;
    fn reset(&mut self);
}

enum AspStrategy<'a> {
    Smodels(ProgramReader<SmodelsInput<'a>>),
    AspifText(ProgramReader<AspifTextInput<'a>>),
}

impl AspStrategy<'_> {
    fn incremental(&self) -> bool {
        match self {
            Self::Smodels(reader) => reader.incremental(),
            Self::AspifText(reader) => reader.incremental(),
        }
    }

    fn parse(&mut self) -> bool {
        match self {
            // Upstream ProgramParser::parse() parses a single step.
            Self::Smodels(reader) => reader.parse(ReadMode::Incremental),
            Self::AspifText(reader) => reader.parse(ReadMode::Incremental),
        }
    }

    fn more(&mut self) -> bool {
        match self {
            Self::Smodels(reader) => reader.more(),
            Self::AspifText(reader) => reader.more(),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Smodels(reader) => reader.reset(),
            Self::AspifText(reader) => reader.reset(),
        }
    }
}

pub struct AspParser<'a> {
    out: *mut (dyn AbstractProgram + 'a),
    input_buf: Vec<u8>,
    strategy: Option<AspStrategy<'a>>,
}

impl<'a> AspParser<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn AbstractProgram) -> Self {
        Self {
            out: out as *mut _,
            input_buf: Vec::new(),
            strategy: None,
        }
    }

    #[must_use]
    pub fn accept_char(c: char) -> bool {
        is_digit(c) || c == 'a'
    }

    fn out_mut(&mut self) -> &'a mut dyn AbstractProgram {
        // SAFETY: `out` is provided by the caller for lifetime 'a and is only
        // accessed via &mut self. This mirrors upstream unique ownership.
        unsafe { &mut *self.out }
    }

    fn first_non_ws(&self) -> Option<char> {
        self.input_buf
            .iter()
            .copied()
            .map(char::from)
            .find(|c| !matches!(c, ' ' | '\t' | '\n' | '\r'))
    }

    fn accept_smodels(&mut self, opts: ParserOptions) -> bool {
        let mut in_opts = SmodelsOptions::default().enable_clasp_ext();
        if opts.is_enabled(Extension::ParseAcycEdge) {
            in_opts = in_opts.convert_edges();
        }
        if opts.is_enabled(Extension::ParseHeuristic) {
            in_opts = in_opts.convert_heuristic();
        }

        let mut reader = ProgramReader::new(SmodelsInput::new(self.out_mut(), in_opts));
        let accepted = reader.accept(Cursor::new(self.input_buf.as_slice()));
        if accepted {
            self.strategy = Some(AspStrategy::Smodels(reader));
        }
        accepted
    }

    fn accept_aspif_text(&mut self) -> bool {
        let mut reader = ProgramReader::new(AspifTextInput::new(self.out_mut()));
        let accepted = reader.accept(Cursor::new(self.input_buf.as_slice()));
        if accepted {
            self.strategy = Some(AspStrategy::AspifText(reader));
        }
        accepted
    }
}

impl ProgramParserApi for AspParser<'_> {
    fn accept(&mut self, input: &mut dyn Read, opts: ParserOptions) -> bool {
        self.reset();
        if input.read_to_end(&mut self.input_buf).is_err() {
            return false;
        }
        if detect_problem_type(&self.input_buf) != ProblemType::Asp {
            return false;
        }
        if self.first_non_ws().is_some_and(is_digit) {
            self.accept_smodels(opts)
        } else {
            self.accept_aspif_text()
        }
    }

    fn incremental(&self) -> bool {
        self.strategy.as_ref().is_some_and(AspStrategy::incremental)
    }

    fn is_open(&self) -> bool {
        self.strategy.is_some()
    }

    fn parse(&mut self) -> bool {
        self.strategy.as_mut().is_some_and(AspStrategy::parse)
    }

    fn more(&mut self) -> bool {
        self.strategy.as_mut().is_some_and(AspStrategy::more)
    }

    fn reset(&mut self) {
        if let Some(mut strategy) = self.strategy.take() {
            strategy.reset();
        }
        self.input_buf.clear();
    }
}
