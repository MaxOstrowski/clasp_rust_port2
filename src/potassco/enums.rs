//! Rust port of original_clasp/libpotassco/potassco/enum.h.

use core::marker::PhantomData;
use core::ops::BitAnd;

pub trait UnderlyingValue: Copy + Eq + Ord + 'static {
    const MIN: Self;
    const MAX: Self;

    fn to_i128(self) -> i128;
    fn from_i128(value: i128) -> Option<Self>;
}

macro_rules! impl_underlying_value {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl UnderlyingValue for $ty {
                const MIN: Self = <$ty>::MIN;
                const MAX: Self = <$ty>::MAX;

                fn to_i128(self) -> i128 {
                    self as i128
                }

                fn from_i128(value: i128) -> Option<Self> {
                    <$ty>::try_from(value).ok()
                }
            }
        )+
    };
}

impl_underlying_value!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);

pub mod detail {
    use super::EnumTag;
    use core::marker::PhantomData;

    pub struct EnumMeta<E: EnumTag>(PhantomData<fn() -> E>);

    impl<E: EnumTag> EnumMeta<E> {
        pub fn min() -> E::Repr {
            E::min_value()
        }

        pub fn max() -> E::Repr {
            E::max_value()
        }

        pub fn count() -> usize {
            E::count()
        }

        pub fn valid(value: E::Repr) -> bool {
            E::metadata().is_some_and(|meta| meta.valid(value))
        }

        pub fn name(value: E) -> &'static str {
            E::metadata()
                .and_then(|meta| meta.name(value))
                .unwrap_or("")
        }
    }
}

#[derive(Clone, Copy)]
pub struct DefaultEnum<E: EnumTag> {
    first: E::Repr,
    count: usize,
    _marker: PhantomData<fn() -> E>,
}

impl<E: EnumTag> DefaultEnum<E> {
    pub fn new(count: usize) -> Self {
        Self::new_with_first(
            count,
            E::Repr::from_i128(0).expect("zero must be representable in enum underlying type"),
        )
    }

    pub const fn new_with_first(count: usize, first: E::Repr) -> Self {
        Self {
            first,
            count,
            _marker: PhantomData,
        }
    }

    pub const fn min(&self) -> E::Repr {
        self.first
    }

    pub fn max(&self) -> E::Repr {
        if self.count == 0 {
            self.first
        } else {
            E::Repr::from_i128(self.first.to_i128() + (self.count - 1) as i128)
                .expect("enum metadata range exceeds underlying type")
        }
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub fn valid(&self, value: E::Repr) -> bool {
        value >= self.min() && value <= self.max()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FixedString<const N: usize> {
    pub data: [u8; N],
    pub nul: u8,
}

impl<const N: usize> FixedString<N> {
    pub fn new(value: &str) -> Self {
        let mut data = [0; N];
        data.copy_from_slice(&value.as_bytes()[..N]);
        Self { data, nul: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data).expect("fixed strings are built from UTF-8 input")
    }

    pub const fn size(&self) -> usize {
        N
    }
}

#[derive(Clone, Copy)]
pub struct EnumEntries<E: EnumTag> {
    entries: &'static [(E, &'static str)],
}

impl<E: EnumTag> EnumEntries<E> {
    pub fn new(entries: &'static [(E, &'static str)]) -> Self {
        if entries
            .windows(2)
            .all(|pair| pair[0].0.to_underlying() <= pair[1].0.to_underlying())
        {
            Self { entries }
        } else {
            let mut sorted = entries.to_vec();
            sorted.sort_by_key(|(value, _)| value.to_underlying());
            Self {
                entries: Box::leak(sorted.into_boxed_slice()),
            }
        }
    }

    pub fn min(&self) -> E::Repr {
        self.entries
            .iter()
            .map(|&(value, _)| value.to_underlying())
            .min()
            .expect("enum entries metadata requires at least one entry")
    }

    pub fn max(&self) -> E::Repr {
        self.entries
            .iter()
            .map(|&(value, _)| value.to_underlying())
            .max()
            .expect("enum entries metadata requires at least one entry")
    }

    pub const fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn trivial(&self) -> bool {
        self.entries
            .iter()
            .enumerate()
            .all(|(index, &(value, _))| value.to_underlying().to_i128() == index as i128)
    }

    pub fn valid(&self, value: E::Repr) -> bool {
        self.entries
            .iter()
            .any(|&(entry, _)| entry.to_underlying() == value)
    }

    pub fn find(&self, value: E) -> Option<&'static (E, &'static str)> {
        self.entries.iter().find(|(entry, _)| *entry == value)
    }

    pub fn name(&self, value: E) -> Option<&'static str> {
        self.find(value).map(|entry| entry.1)
    }

    pub const fn entries(&self) -> &'static [(E, &'static str)] {
        self.entries
    }
}

#[derive(Clone, Copy)]
pub enum EnumMetadata<E: EnumTag> {
    Default(DefaultEnum<E>),
    Entries(EnumEntries<E>),
}

impl<E: EnumTag> EnumMetadata<E> {
    pub fn min(self) -> E::Repr {
        match self {
            Self::Default(meta) => meta.min(),
            Self::Entries(meta) => meta.min(),
        }
    }

    pub fn max(self) -> E::Repr {
        match self {
            Self::Default(meta) => meta.max(),
            Self::Entries(meta) => meta.max(),
        }
    }

    pub fn count(self) -> usize {
        match self {
            Self::Default(meta) => meta.count(),
            Self::Entries(meta) => meta.count(),
        }
    }

    pub fn valid(self, value: E::Repr) -> bool {
        match self {
            Self::Default(meta) => meta.valid(value),
            Self::Entries(meta) => meta.valid(value),
        }
    }

    pub fn name(self, value: E) -> Option<&'static str> {
        match self {
            Self::Default(_) => None,
            Self::Entries(meta) => meta.name(value),
        }
    }

    pub fn entries(self) -> Option<&'static [(E, &'static str)]> {
        match self {
            Self::Default(_) => None,
            Self::Entries(meta) => Some(meta.entries()),
        }
    }
}

pub trait EnumTag: Copy + Eq + 'static {
    type Repr: UnderlyingValue;

    fn to_underlying(self) -> Self::Repr;
    fn from_underlying(value: Self::Repr) -> Option<Self>;

    fn metadata() -> Option<EnumMetadata<Self>> {
        None
    }

    fn min_value() -> Self::Repr {
        Self::metadata().map_or(Self::Repr::MIN, EnumMetadata::min)
    }

    fn max_value() -> Self::Repr {
        Self::metadata().map_or(Self::Repr::MAX, EnumMetadata::max)
    }

    fn count() -> usize {
        Self::metadata()
            .map(EnumMetadata::count)
            .expect("enum_count requires enum metadata")
    }

    fn name(self) -> Option<&'static str> {
        Self::metadata().and_then(|meta| meta.name(self))
    }
}

pub trait HasEnumEntries: EnumTag {
    fn entries_metadata() -> EnumEntries<Self>;
}

pub fn make_entries<E: EnumTag>(entries: &'static [(E, &'static str)]) -> EnumEntries<E> {
    EnumEntries::new(entries)
}

pub fn to_underlying<E: EnumTag>(value: E) -> E::Repr {
    value.to_underlying()
}

pub fn enum_entries<E: HasEnumEntries>() -> &'static [(E, &'static str)] {
    E::entries_metadata().entries()
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
