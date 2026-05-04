use rust_clasp::potassco::utils::{
    ConstString, DynamicBitset, DynamicBuffer, RADIX_DEF, RADIX_ONLY, StringMap, Temp, enumerate,
    radix_sort, try_emplace,
};
use std::rc::Rc;

#[test]
fn const_string_default_constructor_matches_upstream_empty_state() {
    let value = ConstString::default();

    assert_eq!(value.size(), 0);
    assert!(!value.c_str().is_null());
    assert_eq!(unsafe { *value.c_str() }, 0);
}

#[test]
fn const_string_small_to_large_transition_matches_upstream() {
    let mut value = String::new();

    for len in 0usize.. {
        value.clear();
        value.extend(std::iter::repeat_n('x', len));

        let string = ConstString::from(value.as_str());

        assert_eq!(value.len(), len);
        assert_eq!(string.size(), value.len());
        assert_eq!(string.view(), value);
        assert_eq!(
            std::ffi::CStr::from_bytes_until_nul(unsafe {
                std::slice::from_raw_parts(string.c_str(), string.size() + 1)
            })
            .unwrap()
            .to_str()
            .unwrap(),
            value
        );

        if !string.small() {
            break;
        }
    }

    assert_eq!(value.len(), 24);
}

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
fn dynamic_buffer_constructors_match_upstream() {
    let buffer = DynamicBuffer::default();
    assert_eq!(buffer.size(), 0);
    assert_eq!(buffer.capacity(), 0);
    assert!(buffer.as_ptr().is_null());

    let buffer = DynamicBuffer::new(256);
    assert_eq!(buffer.capacity(), 256);
    assert_eq!(buffer.size(), 0);
    assert!(!buffer.as_ptr().is_null());

    let mut borrowed = [0u8; 5];
    let ptr = borrowed.as_ptr();
    let mut buffer = DynamicBuffer::borrowed(&mut borrowed);
    assert_eq!(buffer.capacity(), 5);
    assert_eq!(buffer.size(), 0);
    assert_eq!(buffer.as_ptr(), ptr);

    buffer.alloc(4).fill(b'A');
    assert_eq!(buffer.view(), "AAAA");
    assert_eq!(buffer.size(), 4);
    assert_eq!(buffer.as_ptr(), ptr);

    buffer.alloc(1).fill(b'B');
    assert_eq!(buffer.view(), "AAAAB");
    assert_eq!(buffer.size(), 5);
    assert_eq!(buffer.as_ptr(), ptr);

    buffer.alloc(1).fill(b'C');
    assert_eq!(buffer.view(), "AAAABC");
    assert_eq!(buffer.size(), 6);
    assert_ne!(buffer.as_ptr(), ptr);
}

#[test]
fn dynamic_buffer_mutating_methods_follow_upstream_buffer_lifecycle() {
    let mut buffer = DynamicBuffer::default();

    buffer.reserve(10);
    assert!(buffer.capacity() >= 10);
    assert_eq!(buffer.size(), 0);

    buffer.append_bytes(b"ab");
    buffer.push(b'c');
    assert_eq!(buffer.view(), "abc");
    assert_eq!(buffer.size(), 3);

    *buffer.back() = b'd';
    assert_eq!(buffer.view(), "abd");

    buffer.pop(1);
    assert_eq!(buffer.view(), "ab");
    assert_eq!(buffer.size(), 2);

    let capacity_before_clear = buffer.capacity();
    buffer.clear();
    assert_eq!(buffer.size(), 0);
    assert_eq!(buffer.capacity(), capacity_before_clear);
    assert_eq!(buffer.view(), "");

    buffer.append_bytes(b"left");
    let mut other = DynamicBuffer::default();
    other.append_bytes(b"right");
    buffer.swap(&mut other);
    assert_eq!(buffer.view(), "right");
    assert_eq!(other.view(), "left");

    buffer.release();
    assert_eq!(buffer.size(), 0);
    assert_eq!(buffer.capacity(), 0);
    assert!(buffer.as_ptr().is_null());
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
fn const_string_clone_from_matches_upstream_copy_assignment() {
    let small_source = ConstString::from("small");
    let mut assigned = ConstString::from("seed");
    assigned.clone_from(&small_source);
    assert_eq!(assigned.view(), "small");
    assert_ne!(assigned.c_str(), small_source.c_str());

    let large_value = "x".repeat(32);
    let large_source = ConstString::from(large_value.as_str());
    assigned.clone_from(&large_source);
    assert_eq!(assigned.view(), large_value);
    assert_eq!(std::borrow::Borrow::<str>::borrow(&assigned), large_value);
    assert_ne!(assigned.c_str(), large_source.c_str());

    let borrowed_source = String::from("long string longer than sso");
    let borrowed = unsafe { ConstString::borrowed(&borrowed_source) };
    assigned.clone_from(&borrowed);
    assert_eq!(assigned.view(), borrowed_source);
    assert_ne!(assigned.view().as_ptr(), borrowed_source.as_ptr());
}

#[test]
fn const_string_index_reads_bytes_from_the_current_view() {
    let value = ConstString::from("small");

    assert_eq!(value[0], b's');
    assert_eq!(value[4], b'l');
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
fn dynamic_bitset_reserve_and_clear_match_source_contract() {
    let mut bitset = DynamicBitset::default();

    bitset.reserve(256);
    assert!(bitset.empty());
    assert_eq!(bitset.words(), 0);
    assert_eq!(bitset.count(), 0);

    assert!(bitset.add(4));
    assert!(bitset.add(130));
    assert!(!bitset.empty());
    assert_eq!(bitset.words(), 3);

    bitset.clear();
    assert!(bitset.empty());
    assert_eq!(bitset.words(), 0);
    assert_eq!(bitset.count(), 0);
    assert_eq!(bitset.smallest(), 0);
    assert_eq!(bitset.largest(), 0);
    assert!(!bitset.contains(4));
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

#[test]
fn temp_resize_replaces_existing_contents_like_upstream() {
    let mut temp = Temp::default();
    temp.resize(2, 1u32);
    temp.as_mut_slice().copy_from_slice(&[7, 8]);

    temp.resize(3, 9);

    assert_eq!(temp.as_slice(), &[9, 9, 9]);
}

#[test]
fn temp_default_constructor_starts_empty() {
    let mut temp = Temp::<u32>::default();

    assert_eq!(temp.size(), 0);
    assert!(temp.data().is_null());
}

#[test]
fn temp_size_counts_elements_not_bytes() {
    let mut temp = Temp::default();
    assert_eq!(temp.size(), 0);

    temp.resize(4, 5u64);

    assert_eq!(temp.size(), 4);
}

#[test]
fn temp_data_is_null_for_empty_storage_like_dynamic_buffer() {
    let mut temp = Temp::<u32>::default();

    assert!(temp.data().is_null());
}

#[test]
fn temp_drop_releases_all_live_elements() {
    let value = Rc::new(());

    {
        let mut temp = Temp::default();
        temp.resize(3, value.clone());
        assert_eq!(Rc::strong_count(&value), 4);
    }

    assert_eq!(Rc::strong_count(&value), 1);
}
