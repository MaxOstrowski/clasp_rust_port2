use rust_clasp::clasp::util::pod_vector::{OutOfRangeError, PodVector, erase, erase_if, swap};

#[test]
fn pod_vector_starts_empty() {
    let vector = PodVector::<u32>::new();
    assert_eq!(vector.size(), 0);
    assert_eq!(vector.capacity(), 0);
    assert!(vector.empty());
    assert!(vector.data().is_null());
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
fn pod_vector_at_reports_out_of_range() {
    let vector = PodVector::from_slice(&[10u32, 11]);
    assert_eq!(vector.at(1), Ok(&11));
    assert_eq!(vector.at(2), Err(OutOfRangeError));
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
