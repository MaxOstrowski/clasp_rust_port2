use rust_clasp::clasp::util::pod_vector::{OutOfRangeError, PodVector, erase, erase_if, swap};
use std::mem::size_of;

#[test]
fn pod_vector_starts_empty() {
    let vector = PodVector::<u32>::new();
    assert_eq!(vector.size(), 0);
    assert_eq!(vector.capacity(), 0);
    assert!(vector.empty());
    assert!(vector.data().is_null());
}

#[test]
fn pod_vector_begin_matches_data_pointer() {
    let empty = PodVector::<u32>::new();
    assert!(empty.begin().is_null());

    let mut reserved = PodVector::<u32>::new();
    reserved.reserve(4);
    assert_eq!(reserved.begin(), reserved.data());

    let vector = PodVector::from_slice(&[10u32, 20, 30]);
    assert_eq!(vector.begin(), vector.data());
    assert_eq!(unsafe { *vector.begin() }, 10);
}

#[test]
fn pod_vector_end_matches_one_past_last_pointer() {
    let empty = PodVector::<u32>::new();
    assert!(empty.end().is_null());

    let mut reserved = PodVector::<u32>::new();
    reserved.reserve(4);
    assert_eq!(reserved.end(), reserved.begin());

    let vector = PodVector::from_slice(&[10u32, 20, 30]);
    assert_eq!(unsafe { vector.end().offset_from(vector.begin()) }, 3);
    assert_eq!(unsafe { *vector.end().sub(1) }, 30);
}

#[test]
fn pod_vector_supports_push_insert_and_erase() {
    let mut vector = PodVector::new();
    vector.push_back(1);
    vector.push_back(3);
    vector.insert(1, 2);
    vector.insert_n(3, 2, 4);
    vector.insert_slice(0, &[0]);
    assert_eq!(vector.as_slice(), &[0, 1, 2, 3, 4, 4]);

    assert_eq!(vector.erase(1), 1);
    assert_eq!(vector.erase_range(2..4), 2);
    assert_eq!(vector.as_slice(), &[0, 2, 4]);
}

#[test]
fn pod_vector_assign_replaces_contents_from_an_iterator() {
    let mut vector = PodVector::from_slice(&[1u32, 2, 3]);

    vector.assign([7u32, 8, 9]);

    assert_eq!(vector.as_slice(), &[7, 8, 9]);
}

#[test]
fn pod_vector_assign_resize_and_clone_match_slice_semantics() {
    let mut vector = PodVector::with_size(3, 7u32);
    vector.assign_fill(2, 9);
    assert_eq!(vector.as_slice(), &[9, 9]);

    vector.assign_from_slice(&[1, 2, 3]);
    vector.resize(5, 8);
    assert_eq!(vector.as_slice(), &[1, 2, 3, 8, 8]);

    vector.resize(2, 0);
    assert_eq!(vector.as_slice(), &[1, 2]);

    let clone = vector.clone();
    assert_eq!(clone, vector);
    assert!(clone >= vector);
}

#[test]
fn pod_vector_resize_uses_upstream_append_realloc_growth() {
    let mut vector = PodVector::new();
    vector.resize(1, 7u32);
    assert_eq!(vector.as_slice(), &[7]);
    assert_eq!(vector.capacity(), 4);

    vector.resize(5, 9);
    assert_eq!(vector.as_slice(), &[7, 9, 9, 9, 9]);
    assert_eq!(vector.capacity(), 6);
}

#[test]
fn pod_vector_at_reports_out_of_range() {
    let vector = PodVector::from_slice(&[10u32, 11]);
    assert_eq!(vector.at(1), Ok(&11));
    assert_eq!(vector.at(2), Err(OutOfRangeError));
}

#[test]
fn pod_vector_front_back_and_index_follow_current_storage() {
    let mut vector = PodVector::from_slice(&[10u32, 20, 30]);

    assert_eq!(*vector.front(), 10);
    assert_eq!(*vector.back(), 30);

    vector[1] = 99;

    assert_eq!(vector[1], 99);
    assert_eq!(vector.as_slice(), &[10, 99, 30]);
}

#[test]
fn pod_vector_clear_only_resets_logical_size() {
    let mut vector = PodVector::from_slice(&[1u32, 2, 3]);
    let capacity = vector.capacity();
    let data = vector.data();

    vector.clear();

    assert_eq!(vector.size(), 0);
    assert!(vector.empty());
    assert_eq!(vector.capacity(), capacity);
    assert_eq!(vector.data(), data);
    assert_eq!(vector.begin(), vector.end());
}

#[test]
fn pod_vector_insert_range_inserts_iterator_values_in_order() {
    let mut vector = PodVector::from_slice(&[1u32, 4]);

    assert_eq!(vector.insert_range(1, [2u32, 3]), 1);

    assert_eq!(vector.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn pod_vector_reserve_grows_capacity_without_changing_contents() {
    let mut vector = PodVector::from_slice(&[1u32, 2, 3]);

    vector.reserve(10);

    assert_eq!(vector.as_slice(), &[1, 2, 3]);
    assert_eq!(vector.size(), 3);
    assert!(vector.capacity() >= 10);

    let grown_capacity = vector.capacity();
    vector.reserve(2);
    assert_eq!(vector.capacity(), grown_capacity);
}

#[test]
fn pod_vector_pop_back_discards_only_the_last_element() {
    let mut vector = PodVector::from_slice(&[4u32, 5, 6]);
    let capacity = vector.capacity();

    vector.pop_back();

    assert_eq!(vector.as_slice(), &[4, 5]);
    assert_eq!(vector.capacity(), capacity);
    assert_eq!(*vector.back(), 5);
}

#[test]
fn pod_vector_max_size_scales_with_element_width() {
    assert_eq!(PodVector::<u8>::new().max_size(), usize::MAX);
    assert_eq!(
        PodVector::<u32>::new().max_size(),
        usize::MAX / size_of::<u32>()
    );
}

#[test]
fn pod_vector_unsafe_resize_no_init_can_be_filled_afterwards() {
    let mut vector = PodVector::<u32>::new();
    unsafe {
        vector.resize_no_init(3);
        let ptr = vector.data_mut();
        ptr.add(0).write(5);
        ptr.add(1).write(6);
        ptr.add(2).write(7);
    }
    assert_eq!(vector.as_slice(), &[5, 6, 7]);
}

#[test]
fn erase_helpers_remove_matching_values() {
    let mut vector = PodVector::from_slice(&[1u32, 2, 3, 2, 4]);
    assert_eq!(erase(&mut vector, &2), 2);
    assert_eq!(vector.as_slice(), &[1, 3, 4]);

    assert_eq!(erase_if(&mut vector, |value| *value % 2 == 1), 2);
    assert_eq!(vector.as_slice(), &[4]);
}

#[test]
fn swap_exchanges_storage() {
    let mut lhs = PodVector::from_slice(&[1u32, 2]);
    let mut rhs = PodVector::from_slice(&[3u32]);
    swap(&mut lhs, &mut rhs);
    assert_eq!(lhs.as_slice(), &[3]);
    assert_eq!(rhs.as_slice(), &[1, 2]);
}
