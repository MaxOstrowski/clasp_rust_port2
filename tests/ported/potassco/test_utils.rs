use rust_clasp::potassco::utils::{
    ConstString, DynamicBitset, DynamicBuffer, RADIX_DEF, RADIX_ONLY, StringMap, enumerate,
    radix_sort, try_emplace,
};

#[test]
fn dynamic_buffer_supports_borrow_growth_and_append() {
    let mut buffer = DynamicBuffer::default();
    assert_eq!(buffer.size(), 0);
    assert_eq!(buffer.capacity(), 0);

    buffer.append_str("hello");
    assert_eq!(buffer.view(), "hello");
    assert!(buffer.capacity() >= 5);

    let mut borrowed = [0u8; 5];
    let ptr = borrowed.as_ptr();
    let mut buffer = DynamicBuffer::borrowed(&mut borrowed);
    buffer.alloc(4).fill(b'A');
    assert_eq!(buffer.view(), "AAAA");
    assert_eq!(buffer.as_ptr(), ptr);
    buffer.alloc(1).fill(b'B');
    assert_eq!(buffer.view(), "AAAAB");
    assert_eq!(buffer.as_ptr(), ptr);
    buffer.alloc(1).fill(b'C');
    assert_eq!(buffer.view(), "AAAABC");
    assert_ne!(buffer.as_ptr(), ptr);
}

#[test]
fn const_string_supports_small_large_borrowed_and_map_usage() {
    let empty = ConstString::default();
    assert_eq!(empty.size(), 0);
    assert!(empty.small());

    let small = ConstString::from("small");
    assert_eq!(small.view(), "small");
    assert!(small.small());

    let large_source = "x".repeat(32);
    let large = ConstString::from(large_source.as_str());
    assert_eq!(large.view(), large_source);
    assert!(!large.small());

    let borrowed_source = String::from("borrowed string longer than sso");
    let borrowed = unsafe { ConstString::borrowed(&borrowed_source) };
    assert_eq!(borrowed.view(), borrowed_source);
    assert!(!borrowed.small());
    let materialized = borrowed.clone();
    assert_eq!(materialized.view(), borrowed_source);
    assert_ne!(materialized.c_str(), borrowed.c_str());

    let mut map: StringMap<i32> = StringMap::default();
    assert!(try_emplace(&mut map, "foo", 22));
    assert!(!try_emplace(&mut map, "foo", 23));
    assert_eq!(map.get("foo"), Some(&22));
}

#[test]
fn dynamic_bitset_matches_upstream_queries_and_ordering() {
    let mut bitset = DynamicBitset::default();
    assert_eq!(bitset.count(), 0);
    assert_eq!(bitset.words(), 0);
    assert_eq!(bitset.smallest(), 0);
    assert_eq!(bitset.largest(), 0);

    bitset.add(63);
    assert_eq!(bitset.count(), 1);
    assert_eq!(bitset.words(), 1);
    assert_eq!(bitset.smallest(), 63);
    assert_eq!(bitset.largest(), 63);
    assert!(bitset.contains(63));
    assert!(!bitset.contains(64));

    let mut other = DynamicBitset::default();
    assert!(other < bitset);
    bitset.add(64);
    bitset.add(12);
    assert_eq!(bitset.smallest(), 12);
    assert_eq!(bitset.largest(), 64);
    bitset.remove(12);

    other.add(64);
    assert!(other < bitset);
    other.add(65);
    assert!(other > bitset);
    other.add(128);
    other.remove(65);
    other.add(63);
    other.remove(128);
    assert_eq!(other, bitset);

    other.add(4096);
    other.add(100000);
    assert_eq!(other.largest(), 100000);
    other.remove(100000);
    assert_eq!(other.count(), 3);
    assert_eq!(other.words(), 65);

    other.add(0);
    other.add(1);
    other.add(65);
    other.add(128);
    other.add(129);
    other.add(130);
    other.apply(!3u64);
    assert!(!other.contains(0));
    assert!(!other.contains(1));
    assert!(!other.contains(64));
    assert!(!other.contains(65));
    assert!(!other.contains(128));
    assert!(!other.contains(129));
    assert!(other.contains(63));
    assert!(other.contains(130));
    assert_eq!(other.count(), 2);
    assert_eq!(other.words(), 3);
}

#[test]
fn enumerate_and_radix_sort_cover_original_utility_cases() {
    let mut values = vec![1, 2, 3];
    let mut seen = String::new();
    for (index, value) in enumerate(values.iter_mut()) {
        seen.push_str(&format!("({index},{value})"));
        *value += 1;
    }
    assert_eq!(seen, "(0,1)(1,2)(2,3)");
    assert_eq!(values, vec![2, 3, 4]);

    let mut owned = vec![47u32, 86, 22, 93, 102, 1, 28, 17, 100];
    let mut expected = owned.clone();
    expected.sort();
    radix_sort::<_, u32, _>(&mut owned, |value| *value, RADIX_DEF);
    assert_eq!(owned, expected);

    let mut domain = Vec::new();
    for index in 1..64u32 {
        let x = (1u64 << index) - 1;
        domain.push(x + 1);
        domain.push(x);
        domain.push(x - 1);
    }
    let mut expected = domain.clone();
    expected.sort();
    radix_sort::<_, u64, _>(&mut domain, |value| *value, RADIX_ONLY);
    assert_eq!(domain, expected);

    let mut pairs = vec![
        ("I".to_string(), 1u32),
        ("II".to_string(), 2),
        ("V".to_string(), 5),
        ("X".to_string(), 10),
        ("XL".to_string(), 40),
        ("L".to_string(), 50),
        ("D".to_string(), 500),
        ("M".to_string(), 1000),
        ("large".to_string(), 2_151_677_984),
    ];
    let mut expected = pairs.clone();
    expected.sort_by_key(|value| value.1);
    radix_sort::<_, u32, _>(&mut pairs, |value| value.1, RADIX_ONLY);
    assert_eq!(pairs, expected);
}
