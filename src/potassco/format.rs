//! Rust port of original_clasp/libpotassco/potassco/format.h.

use core::fmt::{self, Write};

use sprintf::{Printf, PrintfError, vsprintf};

use crate::potassco::enums::{EnumTag, enum_name};
use crate::potassco::utils::DynamicBuffer;

pub trait CharBuffer {
    fn append(&mut self, value: &str) -> &mut Self;
}

impl CharBuffer for String {
    fn append(&mut self, value: &str) -> &mut Self {
        self.push_str(value);
        self
    }
}

impl CharBuffer for DynamicBuffer {
    fn append(&mut self, value: &str) -> &mut Self {
        self.append_str(value)
    }
}

pub trait ToCharsValue {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B;
}

pub fn to_chars<'a, B: CharBuffer, T: ToCharsValue + ?Sized>(
    buffer: &'a mut B,
    value: &T,
) -> &'a mut B {
    value.write_to(buffer)
}

impl ToCharsValue for &str {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(self)
    }
}

impl ToCharsValue for str {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(self)
    }
}

impl ToCharsValue for String {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(self)
    }
}

impl ToCharsValue for char {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        let mut s = [0u8; 4];
        buffer.append(self.encode_utf8(&mut s))
    }
}

impl ToCharsValue for bool {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(if *self { "true" } else { "false" })
    }
}

macro_rules! impl_display_value {
	($($ty:ty),+ $(,)?) => {
		$(
			impl ToCharsValue for $ty {
				fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
					let mut s = String::new();
					let _ = write!(&mut s, "{}", *self);
					buffer.append(&s)
				}
			}
		)+
	};
}

impl_display_value!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl<T> ToCharsValue for T
where
    T: EnumTag,
    T::Repr: fmt::Display,
{
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        let name = enum_name(*self);
        if !name.is_empty() {
            buffer.append(name)
        } else {
            let mut s = String::new();
            let _ = write!(&mut s, "{}", self.to_underlying());
            buffer.append(&s)
        }
    }
}

impl<T: ToCharsValue> ToCharsValue for Option<T> {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        if let Some(value) = self {
            value.write_to(buffer)
        } else {
            buffer
        }
    }
}

impl<T: ToCharsValue, U: ToCharsValue> ToCharsValue for (T, U) {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        self.0.write_to(buffer);
        buffer.append(",");
        self.1.write_to(buffer)
    }
}

impl<T: ToCharsValue> ToCharsValue for Vec<T> {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        for (index, value) in self.iter().enumerate() {
            if index != 0 {
                buffer.append(",");
            }
            value.write_to(buffer);
        }
        buffer
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Signed(i128),
    Unsigned(u128),
    Float {
        value: f64,
        precision: Option<usize>,
    },
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    value: FieldValue,
    width: isize,
    term: Option<char>,
}

impl Field {
    pub fn signed(value: i128, width: isize, term: Option<char>) -> Self {
        Self {
            value: FieldValue::Signed(value),
            width,
            term,
        }
    }

    pub fn unsigned(value: u128, width: isize, term: Option<char>) -> Self {
        Self {
            value: FieldValue::Unsigned(value),
            width,
            term,
        }
    }

    pub fn float(value: f64, width: isize, precision: Option<usize>, term: Option<char>) -> Self {
        Self {
            value: FieldValue::Float { value, precision },
            width,
            term,
        }
    }

    pub fn str(value: impl Into<String>, width: isize) -> Self {
        Self {
            value: FieldValue::Str(value.into()),
            width,
            term: None,
        }
    }

    fn render(&self) -> String {
        let mut inner = match &self.value {
            FieldValue::Signed(value) => value.to_string(),
            FieldValue::Unsigned(value) => {
                if *value == u128::MAX {
                    "umax".to_string()
                } else {
                    value.to_string()
                }
            }
            FieldValue::Float { value, precision } => match precision {
                Some(precision) => format!("{value:.precision$}"),
                None => {
                    let mut out = format!("{value:.6}");
                    while out.contains('.') && out.ends_with('0') {
                        out.pop();
                    }
                    if out.ends_with('.') {
                        out.pop();
                    }
                    out
                }
            },
            FieldValue::Str(value) => value.clone(),
        };
        if let Some(term) = self.term {
            inner.push(term);
        }
        let target = self.width.unsigned_abs();
        if inner.len() >= target {
            return inner;
        }
        let pad = " ".repeat(target - inner.len());
        if self.width >= 0 {
            format!("{pad}{inner}")
        } else {
            format!("{inner}{pad}")
        }
    }
}

impl ToCharsValue for Field {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(&self.render())
    }
}

pub fn int_field(value: impl Into<i128>, width: isize, term: Option<char>) -> Field {
    Field::signed(value.into(), width, term)
}

pub fn uint_field(value: impl Into<u128>, width: isize, term: Option<char>) -> Field {
    Field::unsigned(value.into(), width, term)
}

pub fn float_field(
    value: f64,
    width: isize,
    precision: Option<usize>,
    term: Option<char>,
) -> Field {
    Field::float(value, width, precision, term)
}

pub fn str_field(value: impl Into<String>, width: isize) -> Field {
    Field::str(value, width)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Emphasis {
    None = 0,
    Bold = 1,
    Faint = 2,
    Italic = 3,
    Underline = 4,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Color {
    Black = 1,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Default = 39,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Bg(pub Color);

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TextStyleSpec {
    pub emphasis: Emphasis,
    pub foreground: Option<Color>,
    pub background: Option<Color>,
}

impl Default for Emphasis {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextStyle {
    repr: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TextStyleParseError {
    InvalidArgument,
    OutOfRange,
    DomainError,
}

impl fmt::Display for TextStyleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TextStyleParseError {}

impl TextStyle {
    pub fn new(spec: TextStyleSpec) -> Self {
        let mut repr = String::new();
        repr.push('\u{1b}');
        repr.push('[');
        let _ = write!(&mut repr, "{}", spec.emphasis as u8);
        if let Some(color) = spec.foreground {
            let _ = write!(&mut repr, ";{}", color_code(color, false));
        }
        if let Some(color) = spec.background {
            let _ = write!(&mut repr, ";{}", color_code(color, true));
        }
        repr.push('m');
        if repr == "\u{1b}[0m" {
            Self::default()
        } else {
            Self { repr }
        }
    }

    pub fn bg(color: Color) -> Bg {
        Bg(color)
    }

    pub fn from_string(input: &str, start_pos: usize) -> Result<Self, TextStyleParseError> {
        if start_pos > input.len() {
            return Err(TextStyleParseError::OutOfRange);
        }
        let mut spec = TextStyleSpec::default();
        let mut remaining = &input[start_pos..];
        if remaining.is_empty() {
            return Ok(Self::default());
        }
        while !remaining.is_empty() {
            let end = remaining.find(';').unwrap_or(remaining.len());
            let part = &remaining[..end];
            if part.is_empty() {
                return Err(TextStyleParseError::InvalidArgument);
            }
            let value: u16 = part.parse().map_err(|_| TextStyleParseError::OutOfRange)?;
            if value < 30 {
                if spec.emphasis != Emphasis::None {
                    return Err(TextStyleParseError::InvalidArgument);
                }
                spec.emphasis = match value {
                    0 => Emphasis::None,
                    1 => Emphasis::Bold,
                    2 => Emphasis::Faint,
                    3 => Emphasis::Italic,
                    4 => Emphasis::Underline,
                    _ => return Err(TextStyleParseError::DomainError),
                };
            } else {
                let (color, background) = parse_color(value)?;
                if background {
                    if spec.background.replace(color).is_some() {
                        return Err(TextStyleParseError::InvalidArgument);
                    }
                } else if spec.foreground.replace(color).is_some() {
                    return Err(TextStyleParseError::InvalidArgument);
                }
            }
            if end == remaining.len() {
                break;
            }
            remaining = &remaining[end + 1..];
            if remaining.is_empty() {
                return Err(TextStyleParseError::InvalidArgument);
            }
        }
        Ok(Self::new(spec))
    }

    pub fn view(&self) -> &str {
        &self.repr
    }

    pub fn reset_view(&self) -> &str {
        if self.repr.is_empty() {
            ""
        } else {
            "\u{1b}[0m"
        }
    }
}

fn color_code(color: Color, background: bool) -> u16 {
    match color {
        Color::Default => {
            if background {
                49
            } else {
                39
            }
        }
        Color::Black
        | Color::Red
        | Color::Green
        | Color::Yellow
        | Color::Blue
        | Color::Magenta
        | Color::Cyan
        | Color::White => {
            let base = if background { 40 } else { 30 };
            base + (color as u16 - 1)
        }
        _ => {
            let base = if background { 100 } else { 90 };
            base + (color as u16 - Color::BrightBlack as u16)
        }
    }
}

fn parse_color(value: u16) -> Result<(Color, bool), TextStyleParseError> {
    let background = value == 49 || (41..=47).contains(&value) || (101..=107).contains(&value);
    let color = match value {
        30 => Color::Black,
        31 => Color::Red,
        32 => Color::Green,
        33 => Color::Yellow,
        34 => Color::Blue,
        35 => Color::Magenta,
        36 => Color::Cyan,
        37 => Color::White,
        39 => Color::Default,
        40 => Color::Black,
        41 => Color::Red,
        42 => Color::Green,
        43 => Color::Yellow,
        44 => Color::Blue,
        45 => Color::Magenta,
        46 => Color::Cyan,
        47 => Color::White,
        49 => Color::Default,
        90 => Color::BrightBlack,
        91 => Color::BrightRed,
        92 => Color::BrightGreen,
        93 => Color::BrightYellow,
        94 => Color::BrightBlue,
        95 => Color::BrightMagenta,
        96 => Color::BrightCyan,
        97 => Color::BrightWhite,
        100 => Color::BrightBlack,
        101 => Color::BrightRed,
        102 => Color::BrightGreen,
        103 => Color::BrightYellow,
        104 => Color::BrightBlue,
        105 => Color::BrightMagenta,
        106 => Color::BrightCyan,
        107 => Color::BrightWhite,
        _ => return Err(TextStyleParseError::DomainError),
    };
    Ok((color, background))
}

pub struct Quoted<T> {
    quote: &'static str,
    value: T,
}

impl<T: ToCharsValue> ToCharsValue for Quoted<T> {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(self.quote);
        self.value.write_to(buffer);
        buffer.append(self.quote)
    }
}

pub fn quoted<T>(value: T, quote: &'static str) -> Quoted<T> {
    Quoted { quote, value }
}

pub struct Keyed<T> {
    key: &'static str,
    value: T,
}

impl<T: ToCharsValue> ToCharsValue for Keyed<T> {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        if !self.key.is_empty() {
            buffer.append(self.key).append(": ");
        }
        self.value.write_to(buffer)
    }
}

pub fn keyed<T>(key: &'static str, value: T) -> Keyed<T> {
    Keyed { key, value }
}

pub struct Styled<T> {
    style: TextStyle,
    value: T,
}

impl<T: ToCharsValue> ToCharsValue for Styled<T> {
    fn write_to<'a, B: CharBuffer>(&self, buffer: &'a mut B) -> &'a mut B {
        buffer.append(self.style.view());
        self.value.write_to(buffer);
        buffer.append(self.style.reset_view())
    }
}

pub fn styled<T>(value: T, style: TextStyle) -> Styled<T> {
    Styled { style, value }
}

#[derive(Default)]
pub struct BasicCharBuffer {
    buffer: DynamicBuffer,
    term: Option<char>,
    styled: bool,
}

impl CharBuffer for BasicCharBuffer {
    fn append(&mut self, value: &str) -> &mut Self {
        self.buffer.append_str(value);
        self
    }
}

impl BasicCharBuffer {
    pub fn append(&mut self, value: &str) -> &mut Self {
        CharBuffer::append(self, value)
    }

    pub fn try_append_f(
        &mut self,
        format: &str,
        args: &[&dyn Printf],
    ) -> Result<&mut Self, PrintfError> {
        let rendered = vsprintf(format, args)?;
        Ok(self.append(&rendered))
    }

    pub fn append_f(&mut self, format: &str, args: &[&dyn Printf]) -> &mut Self {
        let _ = self.try_append_f(format, args);
        self
    }

    pub fn v_append_f(&mut self, format: &str, args: &[&dyn Printf]) -> &mut Self {
        self.append_f(format, args)
    }

    pub fn view(&self) -> &str {
        self.buffer.view()
    }

    pub fn empty(&self) -> bool {
        self.buffer.size() == 0
    }

    pub fn clear(&mut self) {
        self.term = None;
        self.styled = false;
        self.buffer.clear();
    }

    pub fn append_value<T: ToCharsValue>(&mut self, value: &T) -> &mut Self {
        value.write_to(self)
    }

    pub fn append_repeat(&mut self, n: usize, c: char) -> &mut Self {
        for _ in 0..n {
            self.append_value(&c);
        }
        self
    }

    pub fn append_sep<T: ToCharsValue>(&mut self, sep: &str, values: &[Option<T>]) -> &mut Self {
        let mut first = true;
        for value in values.iter().flatten() {
            if !first {
                self.append(sep);
            }
            value.write_to(self);
            first = false;
        }
        self
    }

    pub fn open(&mut self, style: TextStyle, term: Option<char>) -> &mut Self {
        if self.styled || self.term.is_some() {
            let _ = self.close();
        }
        if !style.view().is_empty() {
            self.styled = true;
            self.append(style.view());
        }
        self.term = term;
        self
    }

    pub fn close(&mut self) -> &str {
        if self.styled {
            self.append("\u{1b}[0m");
        }
        if let Some(term) = self.term.take() {
            self.append_value(&term);
        }
        self.styled = false;
        self.view()
    }
}

pub fn to_string<T: ToCharsValue>(value: &T) -> String {
    let mut out = String::new();
    value.write_to(&mut out);
    out
}

#[macro_export]
macro_rules! potassco_to_string {
    ($value:expr $(,)?) => {{
        $crate::potassco::format::to_string(&$value)
    }};
    ($first:expr, $($rest:expr),+ $(,)?) => {{
        let mut out = String::new();
        $crate::potassco::format::to_chars(&mut out, &$first);
        $(
            out.push(',');
            $crate::potassco::format::to_chars(&mut out, &$rest);
        )+
        out
    }};
}
