//! Port target for original_clasp/libpotassco/potassco/program_opts/string_convert.h,
//! original_clasp/libpotassco/src/string_convert.cpp.

use std::iter;

use crate::clasp::cli::clasp_app::PreFormat;
use crate::clasp::cli::clasp_cli_options as clasp_cli;
use crate::clasp::cli::clasp_options::{
    ConfigKey, OffType, parse_bool_flag, parse_config_key, parse_opt_params, parse_sat_pre_params,
};
use crate::clasp::solver_strategies::{OptParams, SatPreParams};
use crate::potassco::enums::EnumTag;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Errc {
    #[default]
    Success,
    InvalidArgument,
    ResultOutOfRange,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FromCharsResult {
    pub ptr: usize,
    pub ec: Errc,
}

impl FromCharsResult {
    pub fn remaining<'a>(&self, input: &'a str) -> &'a str {
        &input[self.ptr.min(input.len())..]
    }
}

pub mod parse {
    use super::{Errc, FromCharsResult};

    pub trait ParseOk {
        fn parse_ok(self) -> bool;
    }

    impl ParseOk for bool {
        fn parse_ok(self) -> bool {
            self
        }
    }

    impl ParseOk for Errc {
        fn parse_ok(self) -> bool {
            self == Errc::Success
        }
    }

    impl ParseOk for FromCharsResult {
        fn parse_ok(self) -> bool {
            self.ec == Errc::Success
        }
    }

    pub fn ok<T: ParseOk>(value: T) -> bool {
        value.parse_ok()
    }

    pub fn error(input: &str, ec: Errc) -> FromCharsResult {
        let _ = input;
        FromCharsResult { ptr: 0, ec }
    }

    pub fn success(input: &str, pop: usize) -> FromCharsResult {
        debug_assert!(pop <= input.len());
        FromCharsResult {
            ptr: pop,
            ec: Errc::Success,
        }
    }

    pub fn match_opt(input: &mut &str, value: char) -> bool {
        if input.starts_with(value) {
            *input = &input[value.len_utf8()..];
            true
        } else {
            false
        }
    }

    pub fn eq_ignore_case(lhs: &str, rhs: &str) -> bool {
        lhs.eq_ignore_ascii_case(rhs)
    }

    pub fn eq_ignore_case_n(lhs: &str, rhs: &str, n: usize) -> bool {
        lhs.len() >= n && rhs.len() >= n && lhs[..n].eq_ignore_ascii_case(&rhs[..n])
    }
}

pub trait EnumEntries: EnumTag {
    fn enum_entries() -> &'static [(Self, &'static str)];
}

pub trait ParseChars: Sized {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult;
}

pub trait StringTo: Sized {
    fn string_to(input: &str) -> Option<Self>;
}

impl<T> StringTo for T
where
    T: ParseChars + Default,
{
    fn string_to(input: &str) -> Option<Self> {
        let mut out = Self::default();
        if string_to_errc(input, &mut out) == Errc::Success {
            Some(out)
        } else {
            None
        }
    }
}

pub fn from_chars<T: ParseChars>(input: &str, out: &mut T) -> FromCharsResult {
    T::from_chars_impl(input, out)
}

pub fn extract<T: ParseChars>(input: &mut &str, out: &mut T) -> Errc {
    let result = from_chars(input, out);
    if parse::ok(result) {
        *input = &input[result.ptr..];
        Errc::Success
    } else {
        result.ec
    }
}

pub fn from_chars_str_ref<'a>(input: &'a str, out: &mut &'a str) -> FromCharsResult {
    *out = input;
    parse::success(input, input.len())
}

pub fn string_to_errc<T: ParseChars>(input: &str, out: &mut T) -> Errc {
    let result = from_chars(input, out);
    if !parse::ok(result) {
        result.ec
    } else if result.ptr == input.len() {
        Errc::Success
    } else {
        Errc::InvalidArgument
    }
}

pub fn string_to<T: StringTo>(input: &str, out: &mut T) -> bool {
    if let Some(value) = T::string_to(input) {
        *out = value;
        true
    } else {
        false
    }
}

macro_rules! impl_parse_cli_enum {
    ($($ty:path),+ $(,)?) => {
        $(
            impl ParseChars for $ty {
                fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
                    match clasp_cli::from_chars::<Self>(input) {
                        Ok((value, consumed)) => {
                            *out = value;
                            parse::success(input, consumed)
                        }
                        Err(_) => parse::error(input, Errc::InvalidArgument),
                    }
                }
            }
        )+
    };
}

fn shift_result(result: FromCharsResult, offset: usize) -> FromCharsResult {
    FromCharsResult {
        ptr: result.ptr + offset,
        ec: result.ec,
    }
}

fn skipws(input: &mut &str) {
    let trimmed = input.trim_start_matches([' ', '\u{000c}', '\n', '\r', '\t', '\u{000b}']);
    *input = trimmed;
}

fn detect_base(input: &mut &str) -> u32 {
    if input.starts_with("0x") || input.starts_with("0X") {
        *input = &input[2..];
        16
    } else if input.len() > 1 {
        let bytes = input.as_bytes();
        if bytes[0] == b'0' && (b'0'..=b'7').contains(&bytes[1]) {
            *input = &input[2..];
            8
        } else {
            10
        }
    } else {
        10
    }
}

fn digit_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a') + 10),
        b'A'..=b'F' => Some(u32::from(byte - b'A') + 10),
        _ => None,
    }
}

fn scan_unsigned_digits(input: &str, base: u32) -> usize {
    input
        .as_bytes()
        .iter()
        .take_while(|&&byte| digit_value(byte).is_some_and(|digit| digit < base))
        .count()
}

fn scan_signed_digits(input: &str, base: u32) -> usize {
    if let Some(rest) = input.strip_prefix('-') {
        let digits = scan_unsigned_digits(rest, base);
        usize::from(digits != 0) + digits
    } else {
        scan_unsigned_digits(input, base)
    }
}

fn parse_u128_radix(input: &str, base: u32) -> Result<u128, Errc> {
    if base == 10 {
        input.parse::<u128>().map_err(|_| Errc::ResultOutOfRange)
    } else {
        u128::from_str_radix(input, base).map_err(|_| Errc::ResultOutOfRange)
    }
}

fn parse_i128_radix(input: &str, base: u32) -> Result<i128, Errc> {
    if base == 10 {
        input.parse::<i128>().map_err(|_| Errc::ResultOutOfRange)
    } else if let Some(rest) = input.strip_prefix('-') {
        let magnitude = parse_u128_radix(rest, base)?;
        if magnitude > (i128::MAX as u128) + 1 {
            return Err(Errc::ResultOutOfRange);
        }
        Ok(-(magnitude as i128))
    } else {
        let magnitude = parse_u128_radix(input, base)?;
        i128::try_from(magnitude).map_err(|_| Errc::ResultOutOfRange)
    }
}

fn parse_unsigned_limit(input: &str, out: &mut u128, max: u128) -> FromCharsResult {
    let mut view = input;
    skipws(&mut view);
    if view.starts_with('-') {
        if !view.starts_with("-1") {
            return shift_result(
                parse::error(view, Errc::InvalidArgument),
                input.len() - view.len(),
            );
        }
        *out = max;
        return shift_result(parse::success(view, 2), input.len() - view.len());
    }

    if view.starts_with("imax") || view.starts_with("umax") {
        *out = if view.starts_with("imax") {
            max >> 1
        } else {
            max
        };
        return shift_result(parse::success(view, 4), input.len() - view.len());
    }

    let _ = parse::match_opt(&mut view, '+');
    let base = detect_base(&mut view);
    let offset = input.len() - view.len();
    if view.is_empty() {
        return shift_result(parse::error(view, Errc::InvalidArgument), offset);
    }

    let digits = scan_unsigned_digits(view, base);
    if digits == 0 {
        return shift_result(parse::error(view, Errc::InvalidArgument), offset);
    }
    let token = &view[..digits];
    let value = match parse_u128_radix(token, base) {
        Ok(value) => value,
        Err(ec) => return shift_result(parse::success(view, digits), offset).with_err(ec),
    };
    if value > max {
        shift_result(parse::success(view, digits), offset).with_err(Errc::ResultOutOfRange)
    } else {
        *out = value;
        shift_result(parse::success(view, digits), offset)
    }
}

fn parse_signed_limit(input: &str, out: &mut i128, min: i128, max: i128) -> FromCharsResult {
    let mut view = input;
    skipws(&mut view);
    if view.starts_with("imax") || view.starts_with("imin") {
        *out = if view.starts_with("imax") { max } else { min };
        return shift_result(parse::success(view, 4), input.len() - view.len());
    }

    let _ = parse::match_opt(&mut view, '+');
    let base = detect_base(&mut view);
    let offset = input.len() - view.len();
    if view.is_empty() {
        return shift_result(parse::error(view, Errc::InvalidArgument), offset);
    }

    let digits = scan_signed_digits(view, base);
    if digits == 0 {
        return shift_result(parse::error(view, Errc::InvalidArgument), offset);
    }
    let token = &view[..digits];
    let value = match parse_i128_radix(token, base) {
        Ok(value) => value,
        Err(ec) => return shift_result(parse::success(view, digits), offset).with_err(ec),
    };
    if value < min || value > max {
        shift_result(parse::success(view, digits), offset).with_err(Errc::ResultOutOfRange)
    } else {
        *out = value;
        shift_result(parse::success(view, digits), offset)
    }
}

fn parse_float_impl(input: &str, out: &mut f64) -> FromCharsResult {
    let mut best = None;
    for end in input
        .char_indices()
        .map(|(index, _)| index)
        .skip(1)
        .chain(iter::once(input.len()))
    {
        if let Ok(value) = input[..end].parse::<f64>() {
            best = Some((end, value));
        }
    }
    if let Some((end, value)) = best {
        *out = value;
        parse::success(input, end)
    } else {
        parse::error(input, Errc::InvalidArgument)
    }
}

fn parse_float_limit(input: &str, out: &mut f64, min: f64, max: f64) -> FromCharsResult {
    let mut view = input;
    skipws(&mut view);
    let _ = parse::match_opt(&mut view, '+');
    let offset = input.len() - view.len();
    let result = shift_result(parse_float_impl(view, out), offset);
    if parse::ok(result) && (*out < min || *out > max) {
        result.with_err(Errc::ResultOutOfRange)
    } else {
        result
    }
}

fn parse_char(input: &str, out: &mut char) -> FromCharsResult {
    const C_FROM: &[u8] = b"fnrtv";
    const C_TO: &[char] = &['\u{000c}', '\n', '\r', '\t', '\u{000b}'];

    if input.is_empty() {
        return parse::error(input, Errc::InvalidArgument);
    }
    let bytes = input.as_bytes();
    if bytes[0] == b'\\' && bytes.len() > 1 {
        if let Some(position) = C_FROM.iter().position(|&value| value == bytes[1]) {
            *out = C_TO[position];
            return parse::success(input, 2);
        }
    }
    *out = input.chars().next().expect("checked for empty input above");
    parse::success(input, 1)
}

impl FromCharsResult {
    fn with_err(mut self, ec: Errc) -> Self {
        self.ec = ec;
        self
    }
}

macro_rules! impl_unsigned_parse_chars {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ParseChars for $ty {
                fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
                    let mut temp = 0u128;
                    let result = parse_unsigned_limit(input, &mut temp, <$ty>::MAX as u128);
                    if parse::ok(result) {
                        *out = temp as $ty;
                    }
                    result
                }
            }
        )+
    };
}

macro_rules! impl_signed_parse_chars {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ParseChars for $ty {
                fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
                    let mut temp = 0i128;
                    let result = parse_signed_limit(input, &mut temp, <$ty>::MIN as i128, <$ty>::MAX as i128);
                    if parse::ok(result) {
                        *out = temp as $ty;
                    }
                    result
                }
            }
        )+
    };
}

impl_unsigned_parse_chars!(u8, u16, u32, u64, u128, usize);
impl_signed_parse_chars!(i8, i16, i32, i64, i128, isize);

impl ParseChars for bool {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        if input.is_empty() {
            return parse::error(input, Errc::InvalidArgument);
        }
        if input.starts_with('0') || input.starts_with('1') {
            *out = input.starts_with('1');
            return parse::success(input, 1);
        }
        if input.starts_with("no") || input.starts_with("on") {
            *out = input.starts_with('o');
            return parse::success(input, 2);
        }
        if input.starts_with("off") || input.starts_with("yes") {
            *out = input.starts_with('y');
            return parse::success(input, 3);
        }
        if input.starts_with("false") || input.starts_with("true") {
            *out = input.starts_with('t');
            return parse::success(input, 4 + usize::from(!*out));
        }
        parse::error(input, Errc::InvalidArgument)
    }
}

impl ParseChars for char {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        let mut temp = 0u128;
        let result = parse_unsigned_limit(input, &mut temp, u8::MAX as u128);
        if parse::ok(result) {
            if let Some(value) = char::from_u32(temp as u32) {
                *out = value;
                return result;
            }
            return result.with_err(Errc::ResultOutOfRange);
        }
        parse_char(input, out)
    }
}

impl ParseChars for f32 {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        let mut temp = 0.0f64;
        let result = parse_float_limit(input, &mut temp, f32::MIN as f64, f32::MAX as f64);
        if parse::ok(result) {
            *out = temp as f32;
        }
        result
    }
}

impl ParseChars for f64 {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        parse_float_limit(input, out, f64::MIN, f64::MAX)
    }
}

impl ParseChars for String {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        out.push_str(input);
        parse::success(input, input.len())
    }
}

impl ParseChars for ConfigKey {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        match parse_config_key(input) {
            Ok(value) => {
                *out = value;
                parse::success(
                    input,
                    input
                        .split_once(',')
                        .map_or(input.len(), |(head, _)| head.len()),
                )
            }
            Err(_) => parse::error(input, Errc::InvalidArgument),
        }
    }
}

impl ParseChars for SatPreParams {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        match parse_sat_pre_params(input) {
            Ok(value) => {
                *out = value;
                parse::success(input, input.len())
            }
            Err(_) => parse::error(input, Errc::InvalidArgument),
        }
    }
}

impl ParseChars for OptParams {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        match parse_opt_params(input) {
            Ok(value) => {
                *out = value;
                parse::success(input, input.len())
            }
            Err(_) => parse::error(input, Errc::InvalidArgument),
        }
    }
}

impl ParseChars for OffType {
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        match parse_bool_flag(input) {
            Ok(false) => {
                *out = OffType;
                parse::success(input, input.len())
            }
            _ => parse::error(input, Errc::InvalidArgument),
        }
    }
}

impl_parse_cli_enum!(
    PreFormat,
    crate::clasp::cli::clasp_cli_options::context_params::ShareMode,
    crate::clasp::cli::clasp_cli_options::context_params::ShortSimpMode,
    crate::clasp::cli::clasp_cli_options::heu_params::Score,
    crate::clasp::cli::clasp_cli_options::heu_params::ScoreOther,
    crate::clasp::cli::clasp_cli_options::solver_strategies::WatchInit,
    crate::clasp::cli::clasp_cli_options::solver_strategies::UpdateMode,
    crate::clasp::cli::clasp_cli_options::solver_strategies::SignHeu,
    crate::clasp::cli::clasp_cli_options::asp_logic_program::ExtendedRuleMode,
    crate::clasp::cli::clasp_cli_options::asp_logic_program::AtomSorting,
    crate::clasp::cli::clasp_cli_options::solve_options::EnumType,
    crate::clasp::cli::clasp_cli_options::restart_params::SeqUpdate,
    crate::clasp::cli::clasp_cli_options::default_unfounded_check::ReasonStrategy,
);

impl<T, U> ParseChars for (T, U)
where
    T: ParseChars + Clone,
    U: ParseChars + Clone,
{
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        let mut rest = input;
        let mut temp = (out.0.clone(), out.1.clone());
        let wrapped = parse::match_opt(&mut rest, '(');
        if let ec @ Errc::InvalidArgument | ec @ Errc::ResultOutOfRange =
            extract(&mut rest, &mut temp.0)
        {
            return FromCharsResult {
                ptr: input.len() - rest.len(),
                ec,
            };
        }
        if parse::match_opt(&mut rest, ',') {
            if let ec @ Errc::InvalidArgument | ec @ Errc::ResultOutOfRange =
                extract(&mut rest, &mut temp.1)
            {
                return FromCharsResult {
                    ptr: input.len() - rest.len(),
                    ec,
                };
            }
        }
        if wrapped && !parse::match_opt(&mut rest, ')') {
            return FromCharsResult {
                ptr: input.len() - rest.len(),
                ec: Errc::InvalidArgument,
            };
        }
        *out = temp;
        FromCharsResult {
            ptr: input.len() - rest.len(),
            ec: Errc::Success,
        }
    }
}

impl<T> ParseChars for Vec<T>
where
    T: ParseChars + Default,
{
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        let mut rest = input;
        let wrapped = parse::match_opt(&mut rest, '[');
        while !rest.is_empty() {
            let mut temp = T::default();
            if let ec @ Errc::InvalidArgument | ec @ Errc::ResultOutOfRange =
                extract(&mut rest, &mut temp)
            {
                return FromCharsResult {
                    ptr: input.len() - rest.len(),
                    ec,
                };
            }
            out.push(temp);
            if rest.len() < 2 || !parse::match_opt(&mut rest, ',') {
                break;
            }
        }
        if wrapped && !parse::match_opt(&mut rest, ']') {
            return FromCharsResult {
                ptr: input.len() - rest.len(),
                ec: Errc::InvalidArgument,
            };
        }
        FromCharsResult {
            ptr: input.len() - rest.len(),
            ec: Errc::Success,
        }
    }
}

impl<E> ParseChars for E
where
    E: EnumEntries,
    E::Repr: ParseChars + Default + Copy,
{
    fn from_chars_impl(input: &str, out: &mut Self) -> FromCharsResult {
        let mut raw = E::Repr::default();
        let mut result = from_chars(input, &mut raw);
        if parse::ok(result) {
            if let Some(value) = E::from_underlying(raw) {
                *out = value;
            } else {
                result = parse::error(input, Errc::InvalidArgument);
            }
        } else {
            for &(key, name) in E::enum_entries() {
                let count = name.len();
                if parse::eq_ignore_case_n(input, name, count)
                    && (count == input.len() || input.as_bytes()[count] == b',')
                {
                    *out = key;
                    result = parse::success(input, count);
                    break;
                }
            }
        }
        result
    }
}
