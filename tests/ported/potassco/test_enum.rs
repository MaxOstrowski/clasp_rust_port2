use rust_clasp::potassco::enums::detail::EnumMeta;
use rust_clasp::potassco::enums::{
    DefaultEnum, EnumMetadata, EnumTag, FixedString, HasEnumEntries, enum_cast, enum_count,
    enum_entries, enum_max, enum_min, enum_name, make_entries,
};
use rust_clasp::potassco::program_opts::{Errc, ParseChars, string_to_errc};

fn string_cast<T>(input: &str) -> Option<T>
where
    T: ParseChars + Default,
{
    let mut out = T::default();
    (string_to_errc(input, &mut out) == Errc::Success).then_some(out)
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Foo {
    #[default]
    Value1 = 0,
    Value2 = 1,
    Value3 = 2,
    Value4 = 3,
    Value5 = 7,
    Value6 = 8,
}

impl HasEnumEntries for Foo {
    fn entries_metadata() -> rust_clasp::potassco::enums::EnumEntries<Self> {
        static ENTRIES: &[(Foo, &str)] = &[
            (Foo::Value1, "value1"),
            (Foo::Value2, "value2"),
            (Foo::Value3, "value3"),
            (Foo::Value4, "value4"),
            (Foo::Value5, "value5"),
            (Foo::Value6, "value6"),
        ];
        make_entries(ENTRIES)
    }
}

impl EnumTag for Foo {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Value1),
            1 => Some(Self::Value2),
            2 => Some(Self::Value3),
            3 => Some(Self::Value4),
            7 => Some(Self::Value5),
            8 => Some(Self::Value6),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(Self::entries_metadata()))
    }
}

#[repr(i8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SignedByte {
    #[default]
    Value1 = 0,
    Value2 = 1,
    Value3 = 2,
    Value4 = 3,
    Value5 = 7,
    Value6 = 8,
}

impl HasEnumEntries for SignedByte {
    fn entries_metadata() -> rust_clasp::potassco::enums::EnumEntries<Self> {
        static ENTRIES: &[(SignedByte, &str)] = &[
            (SignedByte::Value1, "value1"),
            (SignedByte::Value2, "value2"),
            (SignedByte::Value3, "value3"),
            (SignedByte::Value4, "value4"),
            (SignedByte::Value5, "value5"),
            (SignedByte::Value6, "value6"),
        ];
        make_entries(ENTRIES)
    }
}

impl EnumTag for SignedByte {
    type Repr = i8;

    fn to_underlying(self) -> Self::Repr {
        self as i8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Value1),
            1 => Some(Self::Value2),
            2 => Some(Self::Value3),
            3 => Some(Self::Value4),
            7 => Some(Self::Value5),
            8 => Some(Self::Value6),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(Self::entries_metadata()))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Byte {
    #[default]
    Value1 = 0,
    Value2 = 1,
    Value3 = 2,
    Value4 = 3,
    Value5 = 7,
    Value6 = 8,
}

impl HasEnumEntries for Byte {
    fn entries_metadata() -> rust_clasp::potassco::enums::EnumEntries<Self> {
        static ENTRIES: &[(Byte, &str)] = &[
            (Byte::Value1, "value1"),
            (Byte::Value2, "value2"),
            (Byte::Value3, "value3"),
            (Byte::Value4, "value4"),
            (Byte::Value5, "value5"),
            (Byte::Value6, "value6"),
        ];
        make_entries(ENTRIES)
    }
}

impl EnumTag for Byte {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Value1),
            1 => Some(Self::Value2),
            2 => Some(Self::Value3),
            3 => Some(Self::Value4),
            7 => Some(Self::Value5),
            8 => Some(Self::Value6),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(Self::entries_metadata()))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Defaulted {
    #[default]
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
}

impl EnumTag for Defaulted {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            5 => Some(Self::Five),
            6 => Some(Self::Six),
            7 => Some(Self::Seven),
            8 => Some(Self::Eight),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Default(DefaultEnum::new_with_first(4, 5)))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NoMeta {
    Zero = 0,
    One = 1,
}

impl EnumTag for NoMeta {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Zero),
            1 => Some(Self::One),
            _ => None,
        }
    }
}

#[test]
fn explicit_enum_entries_match_upstream_gap_behavior() {
    let expected = [
        (Foo::Value1, "value1"),
        (Foo::Value2, "value2"),
        (Foo::Value3, "value3"),
        (Foo::Value4, "value4"),
        (Foo::Value5, "value5"),
        (Foo::Value6, "value6"),
    ];

    assert_eq!(enum_entries::<Foo>(), expected);
    assert_eq!(enum_count::<Foo>(), 6);
    assert_eq!(enum_name(Foo::Value3), "value3");
    assert_eq!(enum_min::<Foo>(), 0);
    assert_eq!(enum_max::<Foo>(), 8);
    assert_eq!(enum_cast::<Foo>(4), None);
    assert_eq!(enum_cast::<Foo>(5), None);
    assert_eq!(enum_cast::<Foo>(6), None);
    assert_eq!(enum_cast::<Foo>(7), Some(Foo::Value5));
}

#[test]
fn enum_entries_find_returns_matching_entry() {
    let entry = Foo::entries_metadata().find(Foo::Value5);

    assert_eq!(entry, Some(&(Foo::Value5, "value5")));
}

#[test]
fn enum_entries_constructor_sorts_unsorted_input() {
    let entries = make_entries(&[
        (Foo::Value5, "value5"),
        (Foo::Value1, "value1"),
        (Foo::Value6, "value6"),
    ]);

    assert_eq!(
        entries.entries(),
        &[
            (Foo::Value1, "value1"),
            (Foo::Value5, "value5"),
            (Foo::Value6, "value6"),
        ]
    );
    assert_eq!(entries.min(), 0);
    assert_eq!(entries.max(), 8);
}

#[test]
fn fixed_string_constructor_copies_prefix_and_appends_nul() {
    let fixed = FixedString::<5>::new("value1");

    assert_eq!(fixed.data, *b"value");
    assert_eq!(fixed.nul, 0);
}

#[test]
fn fixed_string_size_matches_template_extent() {
    let fixed = FixedString::<5>::new("value1");

    assert_eq!(fixed.size(), 5);
}

#[test]
fn fixed_string_as_str_returns_the_copied_prefix() {
    let fixed = FixedString::<5>::new("value1");

    assert_eq!(fixed.as_str(), "value");
}

#[test]
fn fixed_string_ordering_matches_lexicographic_byte_order() {
    let lhs = FixedString::<5>::new("alpha");
    let rhs = FixedString::<5>::new("bravo");

    assert!(lhs < rhs);
}

#[test]
fn enum_meta_min_uses_metadata_or_underlying_fallback() {
    assert_eq!(EnumMeta::<Foo>::min(), 0);
    assert_eq!(EnumMeta::<NoMeta>::min(), u8::MIN);
}

#[test]
fn enum_meta_max_uses_metadata_or_underlying_fallback() {
    assert_eq!(EnumMeta::<Foo>::max(), 8);
    assert_eq!(EnumMeta::<NoMeta>::max(), u8::MAX);
}

#[test]
fn enum_meta_count_uses_metadata_count() {
    assert_eq!(EnumMeta::<Foo>::count(), 6);
}

#[test]
fn enum_meta_valid_checks_explicit_entries() {
    assert!(EnumMeta::<Foo>::valid(7));
    assert!(!EnumMeta::<Foo>::valid(5));
}

#[test]
fn enum_meta_name_returns_entry_name() {
    assert_eq!(EnumMeta::<Foo>::name(Foo::Value3), "value3");
}

#[test]
fn default_enum_metadata_supports_consecutive_ranges() {
    assert_eq!(enum_min::<Defaulted>(), 5);
    assert_eq!(enum_max::<Defaulted>(), 8);
    assert_eq!(enum_count::<Defaulted>(), 4);
    assert_eq!(enum_cast::<Defaulted>(7), Some(Defaulted::Seven));
}

#[test]
fn enums_without_metadata_fall_back_to_underlying_bounds() {
    assert_eq!(enum_min::<NoMeta>(), u8::MIN);
    assert_eq!(enum_max::<NoMeta>(), u8::MAX);
}

#[test]
fn named_enum_parsing_is_case_insensitive_and_accepts_numeric_values() {
    assert_eq!(string_cast::<Foo>("Value3"), Some(Foo::Value3));
    assert_eq!(string_cast::<Foo>("7"), Some(Foo::Value5));
    assert_eq!(string_cast::<Foo>("vAlUe4"), Some(Foo::Value4));
    assert_eq!(string_cast::<Foo>("8"), Some(Foo::Value6));
    assert_eq!(string_cast::<Foo>("9"), None);
    assert_eq!(string_cast::<Foo>("Value98"), None);

    assert_eq!(
        string_cast::<SignedByte>("Value3"),
        Some(SignedByte::Value3)
    );
    assert_eq!(string_cast::<SignedByte>("8"), Some(SignedByte::Value6));

    assert_eq!(string_cast::<Byte>("Value3"), Some(Byte::Value3));
    assert_eq!(string_cast::<Byte>("8"), Some(Byte::Value6));
}
