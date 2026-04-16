//! Rust port of original_clasp/libpotassco/potassco/error.h and
//! original_clasp/libpotassco/src/error.cpp.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::panic::panic_any;
use std::sync::{Mutex, OnceLock};

use crate::potassco::enums::EnumTag;
use crate::potassco::platform::{ExpressionInfo, SourceLocation};

pub type AbortHandler = fn(&str);

const E2BIG: i32 = 7;
const ENOMEM: i32 = 12;
const EINVAL: i32 = 22;
const EDOM: i32 = 33;
const ERANGE: i32 = 34;
const EOVERFLOW: i32 = 75;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Errc(i32);

impl Errc {
    pub const PRECONDITION_FAIL: Self = Self(-1);
    pub const BAD_ALLOC: Self = Self(ENOMEM);
    pub const LENGTH_ERROR: Self = Self(E2BIG);
    pub const INVALID_ARGUMENT: Self = Self(EINVAL);
    pub const DOMAIN_ERROR: Self = Self(EDOM);
    pub const OUT_OF_RANGE: Self = Self(ERANGE);
    pub const OVERFLOW_ERROR: Self = Self(EOVERFLOW);

    #[must_use]
    pub const fn from_raw(value: i32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    #[must_use]
    pub fn message(self) -> String {
        let text = std::io::Error::from_raw_os_error(self.0).to_string();
        let suffix = format!(" (os error {})", self.0);
        text.strip_suffix(&suffix).unwrap_or(&text).to_owned()
    }
}

pub trait IntoErrc {
    fn into_errc(self) -> Errc;
}

impl IntoErrc for Errc {
    fn into_errc(self) -> Errc {
        self
    }
}

impl IntoErrc for i32 {
    fn into_errc(self) -> Errc {
        Errc::from_raw(self.abs())
    }
}

#[must_use]
pub fn translate_ec<T: IntoErrc>(value: T) -> Errc {
    value.into_errc()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    location: SourceLocation,
    errc: Errc,
    what: String,
}

impl RuntimeError {
    #[must_use]
    pub fn new(errc: Errc, location: SourceLocation, message: String) -> Self {
        Self {
            location,
            errc,
            what: message,
        }
    }

    #[must_use]
    pub const fn errc(&self) -> Errc {
        self.errc
    }

    #[must_use]
    pub const fn location(&self) -> &SourceLocation {
        &self.location
    }

    #[must_use]
    pub fn what(&self) -> &str {
        &self.what
    }

    #[must_use]
    pub fn message(&self) -> &str {
        self.what
            .split_once('\n')
            .map_or(&self.what, |(head, _)| head)
    }

    #[must_use]
    pub fn details(&self) -> &str {
        self.what.split_once('\n').map_or("", |(_, tail)| tail)
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.what)
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    BadAlloc,
    LengthError(String),
    InvalidArgument(String),
    DomainError(String),
    OutOfRange(String),
    OverflowError(String),
    Runtime(RuntimeError),
}

impl Error {
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::BadAlloc => "memory allocation failed",
            Self::LengthError(message)
            | Self::InvalidArgument(message)
            | Self::DomainError(message)
            | Self::OutOfRange(message)
            | Self::OverflowError(message) => message,
            Self::Runtime(error) => error.message(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadAlloc => f.write_str("memory allocation failed"),
            Self::LengthError(message)
            | Self::InvalidArgument(message)
            | Self::DomainError(message)
            | Self::OutOfRange(message)
            | Self::OverflowError(message) => f.write_str(message),
            Self::Runtime(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for Error {}

pub trait FailureCode {
    fn fail(self, info: ExpressionInfo, message: Option<String>) -> !;
}

impl FailureCode for Errc {
    fn fail(self, info: ExpressionInfo, message: Option<String>) -> ! {
        panic_any(build_error(self, info, message))
    }
}

impl FailureCode for i32 {
    fn fail(self, info: ExpressionInfo, message: Option<String>) -> ! {
        translate_ec(self).fail(info, message)
    }
}

fn abort_handler_slot() -> &'static Mutex<Option<AbortHandler>> {
    static SLOT: OnceLock<Mutex<Option<AbortHandler>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[must_use]
pub fn set_abort_handler(handler: Option<AbortHandler>) -> Option<AbortHandler> {
    let mut slot = abort_handler_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    core::mem::replace(&mut *slot, handler)
}

pub fn fail_abort(info: ExpressionInfo, message: Option<String>) -> ! {
    let mut buffer = append_info("Assertion ", info, true);
    if let Some(message) = message.filter(|value| !value.is_empty()) {
        buffer.push_str("\nmessage: ");
        buffer.push_str(&message);
    }
    let handler = {
        let slot = abort_handler_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot
    };
    if let Some(handler) = handler {
        handler(&buffer);
    }
    eprintln!("{buffer}");
    std::process::abort();
}

pub fn fail_throw<C: IntoErrc>(code: C, info: ExpressionInfo, message: Option<String>) -> ! {
    translate_ec(code).fail(info, message)
}

#[must_use]
pub fn build_error(mut code: Errc, info: ExpressionInfo, message: Option<String>) -> Error {
    if code == Errc::BAD_ALLOC {
        return Error::BadAlloc;
    }
    let message = if code == Errc::PRECONDITION_FAIL {
        let mut buffer = append_info("Precondition ", info, false);
        if let Some(message) = message.filter(|value| !value.is_empty()) {
            buffer.push_str("\nmessage: ");
            buffer.push_str(&message);
        }
        code = Errc::INVALID_ARGUMENT;
        buffer
    } else {
        let mut buffer = String::new();
        if let Some(message) = message.filter(|value| !value.is_empty()) {
            buffer.push_str(&message);
            buffer.push_str(": ");
        }
        buffer.push_str(&code.message());
        buffer.push('\n');
        let prefix = if has_expression(info) { "check " } else { "" };
        buffer.push_str(&append_info(prefix, info, false));
        buffer
    };
    match code {
        code if code == Errc::LENGTH_ERROR => Error::LengthError(message),
        code if code == Errc::INVALID_ARGUMENT => Error::InvalidArgument(message),
        code if code == Errc::DOMAIN_ERROR => Error::DomainError(message),
        code if code == Errc::OUT_OF_RANGE => Error::OutOfRange(message),
        code if code == Errc::OVERFLOW_ERROR => Error::OverflowError(message),
        _ => Error::Runtime(RuntimeError::new(code, info.location, message)),
    }
}

#[must_use]
pub fn format_message(args: fmt::Arguments<'_>) -> String {
    args.to_string()
}

pub struct ScopeExit<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> ScopeExit<F> {
    #[must_use]
    pub fn new(action: F) -> Self {
        Self {
            action: Some(action),
        }
    }
}

impl<F: FnOnce()> Drop for ScopeExit<F> {
    fn drop(&mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
    }
}

#[must_use]
pub fn scope_exit<F: FnOnce()>(action: F) -> ScopeExit<F> {
    ScopeExit::new(action)
}

pub trait HasSize {
    fn size(&self) -> usize;
}

impl<T> HasSize for [T] {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T, const N: usize> HasSize for [T; N] {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> HasSize for Vec<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> HasSize for VecDeque<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl HasSize for str {
    fn size(&self) -> usize {
        self.len()
    }
}

impl HasSize for String {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<K, V> HasSize for BTreeMap<K, V> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> HasSize for BTreeSet<T> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<K, V, S> HasSize for HashMap<K, V, S> {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T, S> HasSize for HashSet<T, S> {
    fn size(&self) -> usize {
        self.len()
    }
}

pub trait SafeCastSource: Copy {
    fn try_to_i128(self) -> Option<i128>;
    fn try_to_u128(self) -> Option<u128>;
}

macro_rules! impl_signed_source {
	($($ty:ty),+ $(,)?) => {
		$(
			impl SafeCastSource for $ty {
				fn try_to_i128(self) -> Option<i128> {
					Some(i128::from(self))
				}

				fn try_to_u128(self) -> Option<u128> {
					u128::try_from(self).ok()
				}
			}
		)+
	};
}

macro_rules! impl_unsigned_source {
	($($ty:ty),+ $(,)?) => {
		$(
			impl SafeCastSource for $ty {
				fn try_to_i128(self) -> Option<i128> {
					i128::try_from(self).ok()
				}

				fn try_to_u128(self) -> Option<u128> {
					Some(u128::from(self))
				}
			}
		)+
	};
}

impl_signed_source!(i8, i16, i32, i64, i128);
impl_unsigned_source!(u8, u16, u32, u64, u128);

impl SafeCastSource for isize {
    fn try_to_i128(self) -> Option<i128> {
        Some(self as i128)
    }

    fn try_to_u128(self) -> Option<u128> {
        u128::try_from(self).ok()
    }
}

impl SafeCastSource for usize {
    fn try_to_i128(self) -> Option<i128> {
        i128::try_from(self).ok()
    }

    fn try_to_u128(self) -> Option<u128> {
        Some(self as u128)
    }
}

pub trait SafeCastTarget: Sized {
    fn try_from_i128(value: i128) -> Option<Self>;
    fn try_from_u128(value: u128) -> Option<Self>;
}

macro_rules! impl_signed_target {
	($($ty:ty),+ $(,)?) => {
		$(
			impl SafeCastTarget for $ty {
				fn try_from_i128(value: i128) -> Option<Self> {
					<$ty>::try_from(value).ok()
				}

				fn try_from_u128(value: u128) -> Option<Self> {
					<$ty>::try_from(value).ok()
				}
			}
		)+
	};
}

macro_rules! impl_unsigned_target {
	($($ty:ty),+ $(,)?) => {
		$(
			impl SafeCastTarget for $ty {
				fn try_from_i128(value: i128) -> Option<Self> {
					<$ty>::try_from(value).ok()
				}

				fn try_from_u128(value: u128) -> Option<Self> {
					<$ty>::try_from(value).ok()
				}
			}
		)+
	};
}

impl_signed_target!(i8, i16, i32, i64, i128, isize);
impl_unsigned_target!(u8, u16, u32, u64, u128, usize);

#[must_use]
pub fn safe_cast<To, From>(from: From) -> To
where
    To: SafeCastTarget,
    From: SafeCastSource,
{
    if let Some(value) = from.try_to_i128().and_then(To::try_from_i128) {
        return value;
    }
    if let Some(value) = from.try_to_u128().and_then(To::try_from_u128) {
        return value;
    }
    fail_throw(Errc::OUT_OF_RANGE, crate::capture_expression!(from), None)
}

#[must_use]
pub fn safe_cast_enum<To, From>(from: From) -> To
where
    To: SafeCastTarget,
    From: EnumTag + Copy,
    From::Repr: SafeCastSource,
{
    safe_cast(from.to_underlying())
}

#[must_use]
pub fn size_cast<To, C>(collection: &C) -> To
where
    To: SafeCastTarget,
    C: HasSize + ?Sized,
    usize: SafeCastSource,
{
    safe_cast(collection.size())
}

fn append_info(kind: &str, info: ExpressionInfo, add_file: bool) -> String {
    let mut buffer = String::new();
    if add_file {
        buffer.push_str(ExpressionInfo::relative_file_name(&info.location));
    } else {
        buffer.push_str(info.location.function);
    }
    buffer.push(':');
    buffer.push_str(&info.location.line.to_string());
    if add_file {
        buffer.push_str(": ");
        buffer.push_str(info.location.function);
    }
    buffer.push_str(": ");
    buffer.push_str(kind);
    if has_expression(info) {
        buffer.push('\'');
        buffer.push_str(info.expression);
        buffer.push_str("' ");
    }
    buffer.push_str("failed.");
    buffer
}

const fn has_expression(info: ExpressionInfo) -> bool {
    !info.expression.is_empty()
}

#[macro_export]
macro_rules! potassco_fail {
	($code:expr $(,)?) => {{
		$crate::potassco::error::FailureCode::fail(
			$code,
			$crate::potassco::platform::ExpressionInfo {
				expression: "",
				location: $crate::current_location!(),
			},
			None,
		)
	}};
	($code:expr, $fmt:literal $(, $args:expr)* $(,)?) => {{
		$crate::potassco::error::FailureCode::fail(
			$code,
			$crate::potassco::platform::ExpressionInfo {
				expression: "",
				location: $crate::current_location!(),
			},
			Some($crate::potassco::error::format_message(format_args!($fmt $(, $args)*))),
		)
	}};
}

#[macro_export]
macro_rules! potassco_check {
	($exp:expr, $code:expr $(,)?) => {{
		if !($exp) {
			$crate::potassco::error::FailureCode::fail($code, $crate::capture_expression!($exp), None);
		}
	}};
	($exp:expr, $code:expr, $fmt:literal $(, $args:expr)* $(,)?) => {{
		if !($exp) {
			$crate::potassco::error::FailureCode::fail(
				$code,
				$crate::capture_expression!($exp),
				Some($crate::potassco::error::format_message(format_args!($fmt $(, $args)*))),
			);
		}
	}};
}

#[macro_export]
macro_rules! potassco_check_pre {
	($exp:expr $(,)?) => {{
		$crate::potassco_check!($exp, $crate::potassco::error::Errc::PRECONDITION_FAIL)
	}};
	($exp:expr, $fmt:literal $(, $args:expr)* $(,)?) => {{
		$crate::potassco_check!(
			$exp,
			$crate::potassco::error::Errc::PRECONDITION_FAIL,
			$fmt $(, $args)*
		)
	}};
}

#[macro_export]
macro_rules! potassco_assert {
	($exp:expr $(,)?) => {{
		if !($exp) {
			$crate::potassco::error::fail_abort($crate::capture_expression!($exp), None);
		}
	}};
	($exp:expr, $fmt:literal $(, $args:expr)* $(,)?) => {{
		if !($exp) {
			$crate::potassco::error::fail_abort(
				$crate::capture_expression!($exp),
				Some($crate::potassco::error::format_message(format_args!($fmt $(, $args)*))),
			);
		}
	}};
}

#[macro_export]
macro_rules! potassco_assert_not_reached {
	($fmt:literal $(, $args:expr)* $(,)?) => {{
		$crate::potassco::error::fail_abort(
			$crate::potassco::platform::ExpressionInfo {
				expression: "not reached",
				location: $crate::current_location!(),
			},
			Some($crate::potassco::error::format_message(format_args!($fmt $(, $args)*))),
		)
	}};
}

#[macro_export]
macro_rules! potassco_debug_assert {
	($($arg:tt)*) => {{
		if cfg!(debug_assertions) {
			$crate::potassco_assert!($($arg)*);
		}
	}};
}

#[macro_export]
macro_rules! potassco_debug_check_pre {
    ($($arg:tt)*) => {{
        if cfg!(debug_assertions) {
            $crate::potassco_check_pre!($($arg)*);
        }
    }};
}
