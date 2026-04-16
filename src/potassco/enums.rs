//! Rust port of original_clasp/libpotassco/potassco/enum.h.

use core::ops::BitAnd;

pub trait EnumTag: Copy + Eq + 'static {
    type Repr: Copy + Eq + Ord + 'static;

    fn to_underlying(self) -> Self::Repr;
    fn from_underlying(value: Self::Repr) -> Option<Self>;
    fn min_value() -> Self::Repr;
    fn max_value() -> Self::Repr;
    fn count() -> usize;
    fn name(self) -> Option<&'static str> {
        None
    }
}

pub fn to_underlying<E: EnumTag>(value: E) -> E::Repr {
    value.to_underlying()
}

pub fn enum_count<E: EnumTag>() -> usize {
    E::count()
}

pub fn enum_min<E: EnumTag>() -> E::Repr {
    E::min_value()
}

pub fn enum_max<E: EnumTag>() -> E::Repr {
    E::max_value()
}

pub fn enum_name<E: EnumTag>(value: E) -> &'static str {
    value.name().unwrap_or("")
}

pub fn enum_cast<E: EnumTag>(value: E::Repr) -> Option<E> {
    E::from_underlying(value)
}

pub fn test<T>(x: T, y: T) -> bool
where
    T: Copy + Eq + BitAnd<Output = T>,
{
    (x & y) == y
}

pub trait EnumName {
    fn enum_name(&self) -> Option<&'static str>;
}

impl<E: EnumTag> EnumName for E {
    fn enum_name(&self) -> Option<&'static str> {
        self.name()
    }
}
