use rust_clasp::clasp::pod_vector::{
    PodQueue, PodVector, PodVectorT, contains, discard_vec, drop, grow_vec_to, move_down,
    shrink_vec_to, size32, to_u32,
};

#[test]
fn to_u32_and_size32_match_upstream_helpers() {
    assert_eq!(to_u32(9), 9);
    let vector = PodVectorT::from_slice(&[1u32, 2, 3]);
    assert_eq!(size32(&vector), 3);
    assert_eq!(size32(&vec![4u32, 5]), 2);
}

#[test]
fn discard_vec_resets_to_default_instance() {
    let mut vector = PodVectorT::from_slice(&[1u32, 2, 3]);
    discard_vec(&mut vector);
    assert!(vector.is_empty());
    assert_eq!(vector.capacity(), 0);

    let mut std_vector = vec![1u32, 2, 3];
    discard_vec(&mut std_vector);
    assert!(std_vector.is_empty());
    assert_eq!(std_vector.capacity(), 0);
}

#[test]
fn grow_and_shrink_helpers_work_for_vec_and_pod_vector() {
    let mut pod = PodVectorT::from_slice(&[1u32]);
    grow_vec_to(&mut pod, 4, 9);
    assert_eq!(pod.as_slice(), &[1, 9, 9, 9]);
    shrink_vec_to(&mut pod, 2);
    assert_eq!(pod.as_slice(), &[1, 9]);

    let mut std_vector = vec![2u32];
    grow_vec_to(&mut std_vector, 3, 7);
    assert_eq!(std_vector, vec![2, 7, 7]);
    shrink_vec_to(&mut std_vector, 1);
    assert_eq!(std_vector, vec![2]);
}

#[test]
fn move_down_compacts_tail_over_gap() {
    let mut pod = PodVectorT::from_slice(&[10u32, 20, 30, 40, 50]);
    move_down(&mut pod, 3, 1);
    assert_eq!(pod.as_slice(), &[10, 40, 50]);
}

#[test]
fn contains_and_drop_operate_on_ranges() {
    let pod = PodVectorT::from_slice(&[1u32, 3, 5, 7]);
    assert!(contains(pod.as_slice(), &5));
    assert!(contains(pod.as_slice().iter(), &&7));
    assert!(!contains(pod.as_slice(), &6));
    assert_eq!(drop(&pod, 2), &[5, 7]);
}

#[test]
fn pod_queue_behaves_like_fifo_with_rewind() {
    let mut queue = PodQueue::default();
    queue.push(10u32);
    queue.push(20);
    queue.push(30);

    assert!(!queue.empty());
    assert_eq!(queue.size(), 3);
    assert_eq!(*queue.front(), 10);
    assert_eq!(*queue.back(), 30);

    queue.pop();
    assert_eq!(queue.size(), 2);
    assert_eq!(queue.pop_ret(), 20);
    assert_eq!(queue.size(), 1);
    assert_eq!(*queue.front(), 30);

    queue.rewind();
    assert_eq!(queue.size(), 3);
    assert_eq!(*queue.front(), 10);

    queue.clear();
    assert!(queue.empty());
    assert_eq!(queue.size(), 0);
}

#[test]
fn pod_vector_selector_destruct_clears_underlying_storage() {
    let mut vector = PodVectorT::from_slice(&[4u32, 5, 6]);
    PodVector::<u32>::destruct(&mut vector);
    assert!(vector.is_empty());
}
