//! Partial Rust port of original_clasp/clasp/cli/clasp_output.h.
//!
//! This module currently covers the sink abstraction and color-style parsing used
//! by the CLI output layer. The higher-level output printers remain unported.

use crate::potassco::format::{
    BasicCharBuffer, Color, Emphasis, TextStyle, TextStyleParseError, TextStyleSpec,
};
use libc::SIGALRM;
use std::fmt;
use std::io::{self, Write};

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

pub enum OutputSink<'a> {
    String(&'a mut String),
    Writer(&'a mut dyn Write),
    CharBuffer(&'a mut BasicCharBuffer),
}

impl<'a> OutputSink<'a> {
    pub fn write(&mut self, text: &str) -> usize {
        match self {
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
        if let Self::Writer(writer) = self {
            let _ = writer.flush();
        }
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
