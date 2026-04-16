use rust_clasp::potassco::bits::{
    BitIndex, Bitset, bit_count, bit_max, clear_bit, clear_mask, nth_bit, right_most_bit, set_bit,
    set_mask, store_clear_bit, store_set_bit, store_toggle_bit, test_any, test_bit, test_mask,
    toggle_bit,
};

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
    assert_eq!(bit_count(127u32), 7);
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
    let mut bitset = Bitset::<u32>::from_iter([1u32, 2, 5]);
    assert_eq!(bitset.count(), 3);
    assert!(bitset.contains(1));
    assert!(bitset.contains(2));
    assert!(bitset.contains(5));
    assert!(!bitset.contains(3));

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

    let mut enum_set = Bitset::<u32, DummyEnum>::default();
    assert!(enum_set.add(DummyEnum::Eight));
    assert!(enum_set.contains(DummyEnum::Eight));
    assert!(!enum_set.contains(DummyEnum::Seven));
    assert!(enum_set.add(DummyEnum::Five));
    enum_set.remove_max(DummyEnum::Seven);
    assert!(enum_set.contains(DummyEnum::Five));
    assert!(!enum_set.contains(DummyEnum::Eight));
}
