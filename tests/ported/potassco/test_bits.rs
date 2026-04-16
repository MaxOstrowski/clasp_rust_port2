use rust_clasp::potassco::bits::{
    BitIndex, Bitset, bit_ceil, bit_count, bit_floor, bit_max, bit_width, clear_bit, clear_mask,
    countl_one, countl_zero, countr_one, countr_zero, has_single_bit, left_most_bit, log2, nth_bit,
    popcount, right_most_bit, rotl, rotr, set_bit, set_mask, store_clear_bit, store_set_bit,
    store_toggle_bit, test_any, test_bit, test_mask, toggle_bit,
};
use std::mem::size_of;

#[derive(Copy, Clone)]
enum DummyEnum {
    Five = 5,
    Seven = 7,
    Eight = 8,
}

impl BitIndex for DummyEnum {
    fn bit_index(self) -> u32 {
        self as u32
    }
}

#[test]
fn bit_functions_match_upstream_behavior() {
    assert_eq!(nth_bit::<u32>(0), 1);
    assert_eq!(nth_bit::<u32>(3), 0b1000);
    assert!(test_bit(7u32, 0));
    assert!(!test_bit(8u32, 4));
    assert_eq!(set_bit(6u32, 0), 7);
    assert_eq!(clear_bit(7u32, 0), 6);
    assert_eq!(toggle_bit(6u32, 0), 7);
    assert!(test_mask(15u32, 7u32));
    assert!(!test_mask(10u32, 6u32));
    assert!(test_any(10u32, 6u32));
    assert_eq!(set_mask(1024u32, 7u32), 1031);
    assert_eq!(clear_mask(19u32, 17u32), 2);
    assert_eq!(bit_max::<u32>(0), 0);
    assert_eq!(bit_max::<u32>(3), 7);
    assert_eq!(bit_max::<u32>(32), u32::MAX);
    assert_eq!(right_most_bit(0b0001_0100u32), 0b0000_0100);
    assert_eq!(left_most_bit(0b0001_0100u32), 0b0001_0000);
    assert_eq!(log2(0u32), 0);
    assert_eq!(log2(1u32), 0);
    assert_eq!(log2(255u32), 7);
    assert_eq!(bit_count(127u32), 7);
}

#[test]
fn cxx20_bit_wrappers_match_upstream_expectations() {
    assert_eq!(bit_ceil(0u32), 1);
    assert_eq!(bit_ceil(1u32), 1);
    assert_eq!(bit_ceil(9u32), 16);
    assert_eq!(bit_floor(0u32), 0);
    assert_eq!(bit_floor(20u32), 16);
    assert_eq!(bit_width(0u32), 0);
    assert_eq!(bit_width(255u32), 8);
    assert_eq!(countl_one(0b1110_0000u8), 3);
    assert_eq!(countl_zero(0b0001_0100u8), 3);
    assert_eq!(countr_one(0b0001_0111u8), 3);
    assert_eq!(countr_zero(0b0001_0100u8), 2);
    assert!(!has_single_bit(0u32));
    assert!(has_single_bit(8u32));
    assert!(!has_single_bit(10u32));
    assert_eq!(popcount(127u32), 7);
    assert_eq!(rotl(0x12u8, 1), 0x24);
    assert_eq!(rotr(0x12u8, 1), 0x09);
}

#[test]
fn store_bit_helpers_mutate_in_place() {
    let mut value = 0u32;
    assert_eq!(store_set_bit(&mut value, 2), 4);
    assert!(test_bit(value, 2));
    assert_eq!(store_toggle_bit(&mut value, 3), 12);
    assert_eq!(store_toggle_bit(&mut value, 2), 8);
    assert_eq!(store_clear_bit(&mut value, 3), 0);
    assert_eq!(value, 0);
}

#[test]
fn bitset_supports_unsigned_and_enum_indices() {
    let mut bitset = Bitset::<u32>::from([1u32, 2, 5]);
    assert_eq!(bitset.count(), 3);
    assert!(bitset.contains(1));
    assert!(bitset.contains(2));
    assert!(bitset.contains(5));
    assert!(!bitset.contains(3));
    assert_eq!(Bitset::<u32>::MAX_COUNT, 32);
    assert_eq!(Bitset::<u32>::new().rep(), 0);
    assert!(Bitset::<u32>::from_rep(8).contains(3));
    assert_eq!(Bitset::<u32>::from_rep(15).count(), 4);

    bitset.remove_max(5);
    assert!(!bitset.contains(5));
    assert_eq!(bitset.count(), 2);
    assert!(bitset.add(3));
    assert!(bitset.add(4));
    assert!(bitset.add(5));
    bitset.remove_max(4);
    assert!(bitset.contains(3));
    assert!(!bitset.contains(4));
    assert!(!bitset.contains(5));
    assert!(bitset.remove(3));
    assert_eq!(bitset.count(), 2);

    let copy = bitset;
    bitset.remove_max(0);
    assert_eq!(bitset.count(), 0);
    assert_eq!(copy.count(), 2);

    let mut copy = copy;
    copy.clear();
    assert_eq!(copy.count(), 0);

    bitset.add(31);
    bitset.add(30);
    assert_eq!(bitset.count(), 2);
    bitset.remove_max(32);
    assert_eq!(bitset.count(), 2);
    bitset.remove_max(31);
    assert_eq!(bitset.count(), 1);

    assert_eq!(size_of::<Bitset<u32, DummyEnum>>(), size_of::<u32>());

    let mut enum_set = Bitset::<u32, DummyEnum>::default();
    assert!(enum_set.add(DummyEnum::Eight));
    assert!(enum_set.contains(DummyEnum::Eight));
    assert!(!enum_set.contains(DummyEnum::Seven));
    assert!(enum_set.add(DummyEnum::Five));
    enum_set.remove_max(DummyEnum::Seven);
    assert!(enum_set.contains(DummyEnum::Five));
    assert!(!enum_set.contains(DummyEnum::Eight));
}
