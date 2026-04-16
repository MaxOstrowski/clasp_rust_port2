use rust_clasp::clasp::util::left_right_sequence::{LeftRightSequence, compute_raw_cap};

fn collect_right<const I: usize>(seq: &LeftRightSequence<i32, f64, I>) -> Vec<f64> {
    seq.right_view().copied().collect()
}

#[test]
fn matches_upstream_inline_and_growth_behavior() {
    type ListType = LeftRightSequence<i32, f64, 56>;

    assert_eq!(ListType::INLINE_RAW_CAP, compute_raw_cap::<i32, f64, 56>());
    assert_eq!(
        ListType::INLINE_RAW_CAP,
        if cfg!(target_pointer_width = "64") {
            32
        } else {
            40
        }
    );

    let cap = ListType::INLINE_RAW_CAP;
    let mut seq = ListType::new();
    assert!(seq.empty());
    assert_eq!(seq.left_capacity(), cap / std::mem::size_of::<i32>());
    assert_eq!(seq.right_capacity(), cap / std::mem::size_of::<f64>());

    seq.push_left(1);
    seq.push_left(2);
    seq.push_right(3.0);
    seq.push_right(4.0);
    seq.push_right(5.0);
    seq.push_left(6);

    assert_eq!(seq.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&seq), vec![3.0, 4.0, 5.0]);

    let mut copy = seq.clone();
    assert_eq!(copy.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&copy), vec![3.0, 4.0, 5.0]);

    let mut moved = copy;
    assert_eq!(moved.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&moved), vec![3.0, 4.0, 5.0]);

    copy = seq.clone();
    assert_eq!(copy.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&copy), vec![3.0, 4.0, 5.0]);

    moved.pop_right();
    moved.try_shrink();
    assert_eq!(moved.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&moved), vec![3.0, 4.0]);

    moved = copy;
    assert_eq!(moved.left_view(), &[1, 2, 6]);
    assert_eq!(collect_right(&moved), vec![3.0, 4.0, 5.0]);
}

#[test]
fn heap_only_variant_supports_mutation_and_reset() {
    type HeapOnly = LeftRightSequence<i32, i32, 0>;

    let mut seq = HeapOnly::new();
    assert!(seq.empty());
    assert_eq!(seq.left_capacity(), 0);
    assert_eq!(seq.right_capacity(), 0);

    seq.push_left(10);
    seq.push_left(20);
    seq.push_right(30);
    seq.push_right(40);

    assert_eq!(seq.left_view(), &[10, 20]);
    assert_eq!(seq.right_view().copied().collect::<Vec<_>>(), vec![30, 40]);

    seq.erase_left(0);
    assert_eq!(seq.left_view(), &[20]);

    seq.erase_right(0);
    assert_eq!(seq.right_view().copied().collect::<Vec<_>>(), vec![40]);

    seq.push_left(50);
    seq.push_right(60);
    seq.erase_left_unordered(0);
    assert_eq!(seq.left_view(), &[50]);

    seq.erase_right_unordered(0);
    assert_eq!(seq.right_view().copied().collect::<Vec<_>>(), vec![60]);

    seq.clear();
    assert!(seq.empty());
    assert!(seq.left_capacity() > 0);

    seq.reset();
    assert!(seq.empty());
    assert_eq!(seq.left_capacity(), 0);
    assert_eq!(seq.right_capacity(), 0);
}

#[test]
fn shrink_helpers_keep_logical_prefixes() {
    type ListType = LeftRightSequence<i32, i32, 56>;

    let mut seq = ListType::new();
    seq.push_left(1);
    seq.push_left(2);
    seq.push_left(3);
    seq.push_right(4);
    seq.push_right(5);
    seq.push_right(6);

    seq.shrink_left_to(2);
    seq.shrink_right_to(2);

    assert_eq!(seq.left_view(), &[1, 2]);
    assert_eq!(seq.right_view().copied().collect::<Vec<_>>(), vec![4, 5]);
}
