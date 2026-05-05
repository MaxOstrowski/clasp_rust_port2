//! Partial Rust port of original_clasp/clasp/cli/clasp_output.h.
//!
//! This module currently covers the sink abstraction and color-style parsing used
//! by the CLI output layer. The higher-level output printers remain unported.

use crate::potassco::basic_types::AtomArg;
use crate::potassco::format::{
    BasicCharBuffer, Color, Emphasis, TextStyle, TextStyleParseError, TextStyleSpec,
};
use crate::potassco::platform::CFile;
use libc::SIGALRM;
use std::fmt;
use std::io::{self, Write};
use std::os::raw::{c_int, c_void};
use std::ptr;

unsafe extern "C" {
    fn fflush(stream: *mut CFile) -> c_int;
    fn fwrite(ptr: *const c_void, size: usize, count: usize, stream: *mut CFile) -> usize;
}

const SIGNAL_NAMES: &[&str] = &[
    "",
    "SIGHUP",
    "SIGINT",
    "SIGQUIT",
    "SIGILL",
    "SIGTRAP",
    "SIGABRT",
    "SIGBUS",
    "",
    "SIGKILL",
    "SIGUSR1",
    "SIGSEGV",
    "SIGUSR2",
    "SIGPIPE",
    "SIGALRM",
    "SIGTERM",
    "SIGSTKFLT",
    "SIGCHLD",
    "",
];

pub fn signal_name(signal: u8) -> Option<&'static str> {
    SIGNAL_NAMES
        .get(usize::from(signal))
        .copied()
        .filter(|name| !name.is_empty())
}

pub fn interrupted_string(signal: i32) -> &'static str {
    if signal == SIGALRM {
        "TIME LIMIT"
    } else {
        "INTERRUPTED"
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatAtom {
    buffer_: String,
    atom_sep_: u32,
    var_start_: u32,
    var_sep_: u32,
}

impl CatAtom {
    pub const EMPTY_POS: u32 = u32::MAX;

    pub fn new() -> Self {
        Self {
            buffer_: String::new(),
            atom_sep_: Self::EMPTY_POS,
            var_start_: Self::EMPTY_POS,
            var_sep_: Self::EMPTY_POS,
        }
    }

    pub fn has_atom(&self) -> bool {
        self.atom_sep_ != Self::EMPTY_POS
    }

    pub fn has_var(&self) -> bool {
        self.var_start_ != Self::EMPTY_POS && self.var_sep_ != Self::EMPTY_POS
    }

    pub fn active(&self) -> bool {
        self.has_atom() || self.has_var()
    }

    pub fn buffer(&self) -> &str {
        &self.buffer_
    }

    pub fn atom_sep(&self) -> u32 {
        self.atom_sep_
    }

    pub fn var_start(&self) -> u32 {
        self.var_start_
    }

    pub fn var_sep(&self) -> u32 {
        self.var_sep_
    }

    pub fn set_layout_for_test(
        &mut self,
        buffer: &str,
        atom_sep: u32,
        var_start: u32,
        var_sep: u32,
    ) {
        self.buffer_.clear();
        self.buffer_.push_str(buffer);
        self.atom_sep_ = atom_sep;
        self.var_start_ = var_start;
        self.var_sep_ = var_sep;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatTemplate {
    data_: String,
    cap_start_: u32,
    fmt_start_: u32,
    arity_: u8,
    max_arg_: u8,
}

impl CatTemplate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> bool {
        !self.data_.is_empty()
    }

    pub fn data(&self) -> &str {
        &self.data_
    }

    pub fn cap_start(&self) -> u32 {
        self.cap_start_
    }

    pub fn fmt_start(&self) -> u32 {
        self.fmt_start_
    }

    pub fn arity(&self) -> u8 {
        self.arity_
    }

    pub fn max_arg(&self) -> u8 {
        self.max_arg_
    }

    pub fn set_layout_for_test(
        &mut self,
        data: &str,
        cap_start: u32,
        fmt_start: u32,
        arity: u8,
        max_arg: u8,
    ) {
        self.data_.clear();
        self.data_.push_str(data);
        self.cap_start_ = cap_start;
        self.fmt_start_ = fmt_start;
        self.arity_ = arity;
        self.max_arg_ = max_arg;
    }
}

pub type CatAssign = CatTemplate;
pub type CatCost = CatTemplate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatStep {
    caption_: String,
    arg_: AtomArg,
    active_: bool,
}

impl Default for CatStep {
    fn default() -> Self {
        Self {
            caption_: String::new(),
            arg_: AtomArg::Last,
            active_: false,
        }
    }
}

impl CatStep {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> bool {
        self.active_
    }

    pub fn step_arg(&self) -> AtomArg {
        self.arg_
    }

    pub fn arg_name(&self) -> &str {
        &self.caption_
    }

    pub fn set_layout_for_test(&mut self, caption: &str, arg: AtomArg, active: bool) {
        self.caption_.clear();
        self.caption_.push_str(caption);
        self.arg_ = arg;
        self.active_ = active;
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutputSinkInitError;

impl fmt::Display for OutputSinkInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid output sink")
    }
}

impl std::error::Error for OutputSinkInitError {}

pub enum OutputSink<'a> {
    File(*mut CFile),
    String(&'a mut String),
    Writer(&'a mut dyn Write),
    CharBuffer(&'a mut BasicCharBuffer),
}

impl<'a> OutputSink<'a> {
    pub fn from_c_file(file: *mut CFile) -> Result<Self, OutputSinkInitError> {
        if file.is_null() {
            Err(OutputSinkInitError)
        } else {
            Ok(Self::File(file))
        }
    }

    pub fn file(&self) -> *mut CFile {
        match self {
            Self::File(file) => *file,
            Self::String(_) | Self::Writer(_) | Self::CharBuffer(_) => Self::no_file(),
        }
    }

    pub fn write(&mut self, text: &str) -> usize {
        match self {
            Self::File(file) => {
                // Match the C++ sink adapter and report the byte count `fwrite` accepted.
                unsafe { fwrite(text.as_ptr().cast(), 1, text.len(), *file) }
            }
            Self::String(buffer) => {
                buffer.push_str(text);
                text.len()
            }
            Self::Writer(writer) => writer.write(text.as_bytes()).unwrap_or(0),
            Self::CharBuffer(buffer) => {
                buffer.append(text);
                text.len()
            }
        }
    }

    pub fn flush(&mut self) {
        match self {
            Self::File(file) => unsafe {
                let _ = fflush(*file);
            },
            Self::Writer(writer) => {
                let _ = writer.flush();
            }
            Self::String(_) | Self::CharBuffer(_) => Self::no_flush(),
        }
    }

    fn no_flush() {}

    fn no_file() -> *mut CFile {
        ptr::null_mut()
    }
}

impl<'a> TryFrom<*mut CFile> for OutputSink<'a> {
    type Error = OutputSinkInitError;

    fn try_from(value: *mut CFile) -> Result<Self, Self::Error> {
        Self::from_c_file(value)
    }
}

impl<'a> From<&'a mut String> for OutputSink<'a> {
    fn from(value: &'a mut String) -> Self {
        Self::String(value)
    }
}

impl<'a> From<&'a mut BasicCharBuffer> for OutputSink<'a> {
    fn from(value: &'a mut BasicCharBuffer) -> Self {
        Self::CharBuffer(value)
    }
}

impl<'a> From<&'a mut dyn Write> for OutputSink<'a> {
    fn from(value: &'a mut dyn Write) -> Self {
        Self::Writer(value)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ColorStyleParseError {
    message: String,
}

impl ColorStyleParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ColorStyleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ColorStyleParseError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ColorStyleSpec {
    trace: TextStyle,
    info: TextStyle,
    note: TextStyle,
    warn: TextStyle,
    err: TextStyle,
}

impl ColorStyleSpec {
    pub fn new(style: &str) -> Result<Self, ColorStyleParseError> {
        Self::parse(style)
    }

    pub fn default_colors() -> Self {
        Self {
            trace: TextStyle::new(TextStyleSpec {
                emphasis: Emphasis::None,
                foreground: Some(Color::BrightMagenta),
                background: None,
            }),
            info: TextStyle::new(TextStyleSpec {
                emphasis: Emphasis::Bold,
                foreground: Some(Color::Green),
                background: None,
            }),
            note: TextStyle::new(TextStyleSpec {
                emphasis: Emphasis::None,
                foreground: Some(Color::BrightYellow),
                background: None,
            }),
            warn: TextStyle::new(TextStyleSpec {
                emphasis: Emphasis::Bold,
                foreground: Some(Color::BrightYellow),
                background: None,
            }),
            err: TextStyle::new(TextStyleSpec {
                emphasis: Emphasis::Bold,
                foreground: Some(Color::Red),
                background: None,
            }),
        }
    }

    pub fn parse(style: &str) -> Result<Self, ColorStyleParseError> {
        let mut result = Self::default();
        let mut rest = style;
        if let Some(stripped) = rest.strip_prefix("*:") {
            result = Self::default_colors();
            rest = stripped;
        }
        if rest.is_empty() {
            return Ok(result);
        }

        for entry in rest.split(':') {
            let (key, _) = entry
                .split_once('=')
                .ok_or_else(|| ColorStyleParseError::new(format!("unknown color key '{entry}'")))?;
            let parsed = parse_style_value(entry, key.len() + 1)?;
            match key {
                "trace" => result.trace = parsed,
                "info" => result.info = parsed,
                "note" => result.note = parsed,
                "warning" => result.warn = parsed,
                "error" => result.err = parsed,
                _ => {
                    return Err(ColorStyleParseError::new(format!(
                        "unknown color key '{key}'"
                    )));
                }
            }
        }
        Ok(result)
    }

    pub fn trace(&self) -> TextStyle {
        self.trace.clone()
    }

    pub fn info(&self) -> TextStyle {
        self.info.clone()
    }

    pub fn note(&self) -> TextStyle {
        self.note.clone()
    }

    pub fn warn(&self) -> TextStyle {
        self.warn.clone()
    }

    pub fn err(&self) -> TextStyle {
        self.err.clone()
    }
}

impl std::str::FromStr for ColorStyleSpec {
    type Err = ColorStyleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

fn parse_style_value(entry: &str, start_pos: usize) -> Result<TextStyle, ColorStyleParseError> {
    TextStyle::from_string(entry, start_pos).map_err(|error| match error {
        TextStyleParseError::InvalidArgument => {
            if duplicate_emphasis(&entry[start_pos..]) {
                ColorStyleParseError::new(format!("duplicate emphasis in '{entry}'"))
            } else {
                ColorStyleParseError::new(format!("invalid style in '{entry}'"))
            }
        }
        TextStyleParseError::OutOfRange => {
            ColorStyleParseError::new(format!("invalid numeric value in '{entry}'"))
        }
        TextStyleParseError::DomainError => {
            let tail = &entry[start_pos..];
            let first = tail.split(';').next().unwrap_or(tail);
            if first.parse::<u16>().is_ok_and(|value| value < 30) {
                ColorStyleParseError::new(format!("invalid emphasis in '{entry}'"))
            } else {
                ColorStyleParseError::new(format!("invalid color in '{entry}'"))
            }
        }
    })
}

fn duplicate_emphasis(value: &str) -> bool {
    value
        .split(';')
        .filter_map(|part| part.parse::<u16>().ok())
        .filter(|value| *value < 30)
        .nth(1)
        .is_some()
}

pub fn write_styled(sink: &mut String, style: &TextStyle, text: &str) {
    sink.push_str(style.view());
    sink.push_str(text);
    sink.push_str(style.reset_view());
}

pub fn flush_writer(writer: &mut dyn Write) -> io::Result<()> {
    writer.flush()
}
