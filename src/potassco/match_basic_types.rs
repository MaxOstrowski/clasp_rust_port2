//! Rust port of original_clasp/libpotassco/potassco/match_basic_types.h.

use std::io::Read;

use crate::potassco::basic_types::{ATOM_MAX, ATOM_MIN, Atom, ID_MAX, Id, Lit, Weight, WeightLit};
pub use crate::potassco::basic_types::{
    AbstractProgram, AtomArg, AtomArgMode, AtomCompare, atom_symbol, cmp_atom, pop_arg, predicate,
};
use crate::potassco::enums::{EnumTag, UnderlyingValue, enum_cast};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ReadMode {
    Incremental,
    Complete,
}

pub struct BufferedStream {
    buf: Vec<u8>,
    line: u32,
    pos: usize,
}

impl BufferedStream {
    #[must_use]
    pub fn new<R: Read>(mut stream: R) -> Self {
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        buf.push(0);
        Self {
            buf,
            line: 1,
            pos: 0,
        }
    }

    #[must_use]
    pub fn peek(&self) -> char {
        char::from(*self.buf.get(self.pos).unwrap_or(&0))
    }

    #[must_use]
    pub fn end(&self) -> bool {
        self.peek() == '\0'
    }

    pub fn get(&mut self) -> char {
        let current = self.peek();
        if current == '\0' {
            return '\0';
        }
        self.pos += 1;
        if current == '\r' {
            if self.peek() == '\n' {
                self.pos += 1;
            }
            self.line += 1;
            return '\n';
        }
        if current == '\n' {
            self.line += 1;
        }
        current
    }

    pub fn unget(&mut self, c: char) -> bool {
        let Ok(value) = u8::try_from(c) else {
            return false;
        };
        if self.pos == 0 {
            return false;
        }
        self.pos -= 1;
        self.buf[self.pos] = value;
        if c == '\n' {
            self.line = self.line.saturating_sub(1);
        }
        true
    }

    pub fn read_int(&mut self, out: &mut i64) -> bool {
        self.skip_ws();
        let sign = self.peek();
        if matches!(sign, '+' | '-') {
            self.pos += 1;
        }
        if !is_digit(self.peek()) {
            return false;
        }
        let mut value = i64::from(to_digit(self.pop()));
        while is_digit(self.peek()) {
            value = value.saturating_mul(10);
            value = value.saturating_add(i64::from(to_digit(self.pop())));
        }
        *out = if sign == '-' { -value } else { value };
        true
    }

    pub fn r#match(&mut self, token: &str) -> bool {
        let bytes = token.as_bytes();
        let end = self.pos.saturating_add(bytes.len());
        if self.buf.get(self.pos..end) == Some(bytes) {
            self.pos = end;
            true
        } else {
            false
        }
    }

    pub fn skip_ws(&mut self) {
        loop {
            let c = self.peek();
            if !matches!(c, '\t'..=' ') {
                break;
            }
            self.get();
        }
    }

    pub fn read(&mut self, buffer_out: &mut [u8]) -> usize {
        let mut written = 0usize;
        while written < buffer_out.len() && !self.end() {
            buffer_out[written] = self.buf[self.pos];
            written += 1;
            self.pos += 1;
        }
        written
    }

    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    fn pop(&mut self) -> char {
        let c = self.peek();
        self.pos += usize::from(c != '\0');
        c
    }
}

pub struct ProgramReaderCore {
    stream: Option<BufferedStream>,
    var_max: Atom,
    incremental: bool,
}

impl Default for ProgramReaderCore {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgramReaderCore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stream: None,
            var_max: ATOM_MAX,
            incremental: false,
        }
    }

    #[must_use]
    pub fn incremental(&self) -> bool {
        self.incremental
    }

    pub fn set_max_var(&mut self, value: Atom) {
        self.var_max = value;
    }

    #[must_use]
    pub fn stream(&self) -> Option<&BufferedStream> {
        self.stream.as_ref()
    }

    pub fn skip_line(&mut self) {
        while self.peek() != '\0' && self.get() != '\n' {}
    }

    pub fn skip_ws(&mut self) -> char {
        let stream = self.stream_mut();
        stream.skip_ws();
        stream.peek()
    }

    #[must_use]
    pub fn peek(&self) -> char {
        self.stream_ref().peek()
    }

    pub fn get(&mut self) -> char {
        self.stream_mut().get()
    }

    pub fn require(&self, condition: bool, message: &str) {
        if !condition {
            self.error(message);
        }
    }

    pub fn r#match(&mut self, token: &str) -> bool {
        self.stream_mut().r#match(token)
    }

    pub fn match_char(&mut self, expected: char) {
        let got = self.get();
        if got != expected {
            panic!(
                "parse error in line {}: '{}' expected",
                self.line(),
                expected
            );
        }
    }

    #[must_use]
    pub fn match_atom(&mut self, error: &str) -> Atom {
        self.match_uint_range(ATOM_MIN, self.var_max, error)
    }

    #[must_use]
    pub fn match_atom_or_zero(&mut self, error: &str) -> Atom {
        self.match_uint_range(0, self.var_max, error)
    }

    #[must_use]
    pub fn match_id(&mut self, error: &str) -> Id {
        self.match_uint_range(0, ID_MAX, error)
    }

    #[must_use]
    pub fn match_lit(&mut self, error: &str) -> Lit {
        let value = self.match_int_range(-(self.var_max as i32), self.var_max as i32, error);
        self.require(value != 0, error);
        value
    }

    #[must_use]
    pub fn match_weight(&mut self, require_positive: bool, error: &str) -> Weight {
        self.match_int_range(if require_positive { 0 } else { i32::MIN }, i32::MAX, error)
    }

    #[must_use]
    pub fn match_wlit(&mut self, require_positive: bool, error: &str) -> WeightLit {
        WeightLit {
            lit: self.match_lit(error),
            weight: self.match_weight(require_positive, error),
        }
    }

    #[must_use]
    pub fn match_uint(&mut self, error: &str) -> u32 {
        self.match_uint_range(0, u32::MAX, error)
    }

    #[must_use]
    pub fn match_int(&mut self, error: &str) -> i32 {
        self.match_int_range(i32::MIN, i32::MAX, error)
    }

    #[must_use]
    pub fn match_uint_range(&mut self, min: u32, max: u32, error: &str) -> u32 {
        self.match_num(min as i64, max as i64, error) as u32
    }

    #[must_use]
    pub fn match_int_range(&mut self, min: i32, max: i32, error: &str) -> i32 {
        self.match_num(i64::from(min), i64::from(max), error) as i32
    }

    #[must_use]
    pub fn match_enum<E: EnumTag>(&mut self, error: &str) -> E
    where
        E::Repr: UnderlyingValue,
    {
        let min = E::min_value().to_i128();
        let max = E::max_value().to_i128();
        let value = self.match_num(min as i64, max as i64, error);
        let repr = E::Repr::from_i128(value as i128).unwrap_or_else(|| self.error(error));
        enum_cast::<E>(repr).unwrap_or_else(|| self.error(error))
    }

    #[must_use]
    pub fn more(&mut self) -> bool {
        if let Some(stream) = self.stream.as_mut() {
            stream.skip_ws();
            !stream.end()
        } else {
            false
        }
    }

    pub fn reset_stream(&mut self) {
        self.stream = None;
    }

    #[must_use]
    pub fn line(&self) -> u32 {
        self.stream.as_ref().map_or(1, BufferedStream::line)
    }

    pub fn error(&self, message: &str) -> ! {
        panic!("parse error in line {}: {}", self.line(), message);
    }

    fn attach_stream<R: Read>(&mut self, input: R) {
        self.stream = Some(BufferedStream::new(input));
    }

    fn set_incremental(&mut self, incremental: bool) {
        self.incremental = incremental;
    }

    fn match_num(&mut self, min: i64, max: i64, error: &str) -> i64 {
        let mut value = 0i64;
        let read_ok = self.stream_mut().read_int(&mut value);
        self.require(read_ok && value >= min && value <= max, error);
        value
    }

    fn stream_ref(&self) -> &BufferedStream {
        self.stream.as_ref().expect("no input stream")
    }

    fn stream_mut(&mut self) -> &mut BufferedStream {
        self.stream.as_mut().expect("no input stream")
    }
}

pub trait ProgramReaderHooks {
    fn do_attach(&mut self, reader: &mut ProgramReaderCore, incremental: &mut bool) -> bool;
    fn do_parse(&mut self, reader: &mut ProgramReaderCore) -> bool;
    fn do_reset(&mut self, _reader: &mut ProgramReaderCore) {}
}

pub struct ProgramReader<H> {
    core: ProgramReaderCore,
    hooks: H,
}

impl<H> ProgramReader<H> {
    #[must_use]
    pub fn new(hooks: H) -> Self {
        Self {
            core: ProgramReaderCore::new(),
            hooks,
        }
    }

    #[must_use]
    pub fn core(&self) -> &ProgramReaderCore {
        &self.core
    }

    #[must_use]
    pub fn core_mut(&mut self) -> &mut ProgramReaderCore {
        &mut self.core
    }

    #[must_use]
    pub fn hooks(&self) -> &H {
        &self.hooks
    }

    #[must_use]
    pub fn hooks_mut(&mut self) -> &mut H {
        &mut self.hooks
    }
}

impl<H: ProgramReaderHooks> ProgramReader<H> {
    pub fn accept<R: Read>(&mut self, input: R) -> bool {
        self.reset();
        self.core.attach_stream(input);
        self.core.set_incremental(false);
        self.core.skip_ws();
        let mut incremental = false;
        let accepted = self.hooks.do_attach(&mut self.core, &mut incremental);
        self.core.set_incremental(incremental);
        accepted
    }

    #[must_use]
    pub fn incremental(&self) -> bool {
        self.core.incremental()
    }

    pub fn parse(&mut self, mode: ReadMode) -> bool {
        assert!(self.core.stream().is_some(), "no input stream");
        loop {
            if !self.hooks.do_parse(&mut self.core) {
                return false;
            }
            self.core.skip_ws();
            let has_more = self.core.more();
            self.core
                .require(!has_more || self.incremental(), "invalid extra input");
            if mode != ReadMode::Complete || !has_more {
                break;
            }
        }
        true
    }

    #[must_use]
    pub fn more(&mut self) -> bool {
        self.core.more()
    }

    pub fn reset(&mut self) {
        self.hooks.do_reset(&mut self.core);
        self.core.reset_stream();
    }

    #[must_use]
    pub fn line(&self) -> u32 {
        self.core.line()
    }
}

pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

#[must_use]
pub fn to_digit(c: char) -> i32 {
    (c as u8 - b'0') as i32
}

pub fn match_term<'a>(input: &mut &'a str) -> Option<&'a str> {
    let bytes = input.as_bytes();
    let mut pos = 0usize;
    let mut paren = 0usize;
    while pos < bytes.len() {
        match bytes[pos] {
            b'(' => paren += 1,
            b')' => {
                if paren == 0 {
                    break;
                }
                paren -= 1;
            }
            b'"' => {
                let mut quoted = false;
                pos += 1;
                while pos < bytes.len() {
                    let c = bytes[pos];
                    if c == b'"' && !quoted {
                        break;
                    }
                    quoted = !quoted && c == b'\\';
                    pos += 1;
                }
                if pos == bytes.len() {
                    break;
                }
            }
            b',' if paren == 0 => break,
            _ => {}
        }
        pos += 1;
    }
    let matched = &input[..pos];
    *input = &input[pos..];
    (!matched.is_empty()).then_some(matched)
}

pub fn match_num<'a>(
    input: &mut &'a str,
    string_out: Option<&mut &'a str>,
    number_out: Option<&mut i32>,
) -> bool {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut pos = 0usize;
    if matches!(bytes[0], b'+' | b'-') {
        pos += 1;
    }
    let start_digits = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == start_digits {
        return false;
    }
    let matched = &input[..pos];
    let Ok(value) = matched.parse::<i32>() else {
        return false;
    };
    if let Some(out) = string_out {
        *out = matched;
    }
    if let Some(out) = number_out {
        *out = value;
    }
    *input = &input[pos..];
    true
}

pub fn read_program<H: ProgramReaderHooks, R: Read>(
    input: R,
    reader: &mut ProgramReader<H>,
) -> i32 {
    if !reader.accept(input) || !reader.parse(ReadMode::Complete) {
        reader.core().error("invalid input format");
    }
    0
}
