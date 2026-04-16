//! Port target for original_clasp/libpotassco/potassco/program_opts/errors.h,
//! original_clasp/libpotassco/src/program_options.cpp.

use std::error::Error as StdError;
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxErrorType {
    MissingValue,
    ExtraValue,
    InvalidFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    key: String,
    kind: SyntaxErrorType,
    message: String,
}

impl SyntaxError {
    pub fn new(kind: SyntaxErrorType, key: impl Into<String>) -> Self {
        let key = key.into();
        let message = format!(
            "SyntaxError: {}{}",
            quote(&key),
            match kind {
                SyntaxErrorType::MissingValue => " requires a value!",
                SyntaxErrorType::ExtraValue => " does not take a value!",
                SyntaxErrorType::InvalidFormat => " unrecognized line!",
            }
        );
        Self { key, kind, message }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn kind(&self) -> SyntaxErrorType {
        self.kind
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for SyntaxError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextErrorType {
    DuplicateOption,
    UnknownOption,
    AmbiguousOption,
    UnknownGroup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextError {
    ctx: String,
    key: String,
    kind: ContextErrorType,
    alternatives: String,
    message: String,
}

impl ContextError {
    pub fn new(
        ctx: impl Into<String>,
        kind: ContextErrorType,
        key: impl Into<String>,
        alternatives: impl Into<String>,
    ) -> Self {
        let ctx = ctx.into();
        let key = key.into();
        let alternatives = alternatives.into();
        let mut message = format_context(&ctx);
        message.push_str(match kind {
            ContextErrorType::DuplicateOption => "duplicate option: ",
            ContextErrorType::UnknownOption => "unknown option: ",
            ContextErrorType::AmbiguousOption => "ambiguous option: ",
            ContextErrorType::UnknownGroup => "unknown group: ",
        });
        message.push_str(&quote(&key));
        if kind == ContextErrorType::AmbiguousOption && !alternatives.is_empty() {
            message.push_str(" could be:\n");
            message.push_str(&alternatives);
        }
        Self {
            ctx,
            key,
            kind,
            alternatives,
            message,
        }
    }

    pub fn ctx(&self) -> &str {
        &self.ctx
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn kind(&self) -> ContextErrorType {
        self.kind
    }

    pub fn alternatives(&self) -> &str {
        &self.alternatives
    }
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for ContextError {}

macro_rules! context_wrapper_common {
    ($name:ident) => {
        impl $name {
            pub fn into_inner(self) -> ContextError {
                self.0
            }
        }

        impl Deref for $name {
            type Target = ContextError;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl StdError for $name {
            fn source(&self) -> Option<&(dyn StdError + 'static)> {
                Some(&self.0)
            }
        }

        impl From<$name> for ContextError {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl From<$name> for Error {
            fn from(value: $name) -> Self {
                Self::Context(value.0)
            }
        }
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateOption(ContextError);

impl DuplicateOption {
    pub fn new(ctx: impl Into<String>, key: impl Into<String>) -> Self {
        Self(ContextError::new(
            ctx,
            ContextErrorType::DuplicateOption,
            key,
            "",
        ))
    }
}

context_wrapper_common!(DuplicateOption);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownOption(ContextError);

impl UnknownOption {
    pub fn new(ctx: impl Into<String>, key: impl Into<String>) -> Self {
        Self(ContextError::new(
            ctx,
            ContextErrorType::UnknownOption,
            key,
            "",
        ))
    }
}

context_wrapper_common!(UnknownOption);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbiguousOption(ContextError);

impl AmbiguousOption {
    pub fn new(
        ctx: impl Into<String>,
        key: impl Into<String>,
        alternatives: impl Into<String>,
    ) -> Self {
        Self(ContextError::new(
            ctx,
            ContextErrorType::AmbiguousOption,
            key,
            alternatives,
        ))
    }
}

context_wrapper_common!(AmbiguousOption);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownGroup(ContextError);

impl UnknownGroup {
    pub fn new(ctx: impl Into<String>, key: impl Into<String>) -> Self {
        Self(ContextError::new(
            ctx,
            ContextErrorType::UnknownGroup,
            key,
            "",
        ))
    }
}

context_wrapper_common!(UnknownGroup);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueErrorType {
    MultipleOccurrences,
    InvalidDefault,
    InvalidValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueError {
    ctx: String,
    key: String,
    value: String,
    kind: ValueErrorType,
    message: String,
}

impl ValueError {
    pub fn new(
        ctx: impl Into<String>,
        kind: ValueErrorType,
        key: impl Into<String>,
        value: impl Into<String>,
        msg: impl Into<String>,
    ) -> Self {
        let ctx = ctx.into();
        let key = key.into();
        let value = value.into();
        let mut msg = msg.into();
        let mut message = format_context(&ctx);
        match kind {
            ValueErrorType::MultipleOccurrences => message.push_str("multiple occurrences: "),
            ValueErrorType::InvalidDefault => {
                if msg.is_empty() {
                    msg.push_str("invalid default value");
                }
                message.push_str(&format!("{} {} for: ", quote(&value), msg));
            }
            ValueErrorType::InvalidValue => {
                if msg.is_empty() {
                    msg.push_str("invalid value");
                }
                message.push_str(&format!("{} {} for: ", quote(&value), msg));
            }
        }
        message.push_str(&quote(&key));
        Self {
            ctx,
            key,
            value,
            kind,
            message,
        }
    }

    pub fn ctx(&self) -> &str {
        &self.ctx
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn kind(&self) -> ValueErrorType {
        self.kind
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for ValueError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Message(String),
    Syntax(SyntaxError),
    Context(ContextError),
    Value(ValueError),
}

impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Syntax(error) => fmt::Display::fmt(error, f),
            Self::Context(error) => fmt::Display::fmt(error, f),
            Self::Value(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> std::option::Option<&(dyn StdError + 'static)> {
        match self {
            Self::Message(_) => None,
            Self::Syntax(error) => Some(error),
            Self::Context(error) => Some(error),
            Self::Value(error) => Some(error),
        }
    }
}

impl From<SyntaxError> for Error {
    fn from(value: SyntaxError) -> Self {
        Self::Syntax(value)
    }
}

impl From<ContextError> for Error {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}

impl From<ValueError> for Error {
    fn from(value: ValueError) -> Self {
        Self::Value(value)
    }
}

pub(crate) fn quote(input: &str) -> String {
    format!("'{}'", input)
}

fn format_context(ctx: &str) -> String {
    if ctx.is_empty() {
        String::new()
    } else {
        format!("In context {}: ", quote(ctx))
    }
}
