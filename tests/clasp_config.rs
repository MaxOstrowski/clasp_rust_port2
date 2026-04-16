use rust_clasp::clasp::config::{
    CLASP_HAS_THREADS, CLASP_LEGAL, CLASP_USE_STD_VECTOR, CLASP_VERSION, CLASP_VERSION_MAJOR,
    CLASP_VERSION_MINOR, CLASP_VERSION_PATCH, cache_line_size,
};
use rust_clasp::clasp::mt::{
    ThreadSafe, has_threads, hasThreads, memory_order_relaxed, memory_order_seq_cst,
};

#[test]
fn config_constants_match_generated_upstream_values() {
    assert_eq!(CLASP_VERSION, "4.0.0");
    assert_eq!(CLASP_VERSION_MAJOR, 4);
    assert_eq!(CLASP_VERSION_MINOR, 0);
    assert_eq!(CLASP_VERSION_PATCH, 0);
    assert_eq!(CLASP_LEGAL, "Copyright (C) Benjamin Kaufmann");
    assert_eq!(CLASP_HAS_THREADS, 1);
    assert_eq!(CLASP_USE_STD_VECTOR, 0);
    assert_eq!(cache_line_size, 64);
    assert!(has_threads());
    assert!(hasThreads());
}

#[test]
fn thread_safe_matches_upstream_solver_test_semantics() {
    let a = ThreadSafe::<i32>::default();
    let b = ThreadSafe::<i32, false>::default();

    assert_eq!(a, 0);
    assert_eq!(b, 0);
    assert_eq!(ThreadSafe::<i32>::new(32), 32);
    assert_eq!(ThreadSafe::<i32, false>::new(32), 32);

    assert_eq!(a.add(12, memory_order_seq_cst), 12);
    assert_eq!(
        b.add(a.load(memory_order_seq_cst), memory_order_seq_cst),
        12
    );
    assert_eq!(
        a.add(b.load(memory_order_seq_cst), memory_order_seq_cst),
        24
    );
    assert_eq!(
        b.add(a.load(memory_order_seq_cst), memory_order_seq_cst),
        36
    );

    assert_eq!(a.sub(4, memory_order_seq_cst), 20);
    assert_eq!(b.sub(b.load(memory_order_seq_cst), memory_order_seq_cst), 0);

    a.store(17, memory_order_seq_cst);
    b.store(83, memory_order_seq_cst);
    assert_eq!(a.exchange(12, memory_order_seq_cst), 17);
    assert_eq!(b.exchange(99, memory_order_seq_cst), 83);

    let mut expected = 12;
    assert!(a.compare_exchange_weak(&mut expected, 32, memory_order_seq_cst));
    assert!(!a.compare_exchange_strong(&mut expected, 99, memory_order_seq_cst));
    assert_eq!(expected, 32);
    assert!(a.compare_exchange_strong(&mut expected, 64, memory_order_seq_cst));
    assert_eq!(expected, 32);
    assert_eq!(a.load(memory_order_relaxed), 64);

    assert!(!b.compare_exchange_strong(&mut expected, 103, memory_order_seq_cst));
    assert_eq!(expected, 99);
    assert!(b.compare_exchange_weak(&mut expected, 192, memory_order_seq_cst));
    assert_eq!(expected, 99);
    assert_eq!(b.load(memory_order_relaxed), 192);
}

#[test]
fn thread_safe_ref_exposes_underlying_storage() {
    let value = ThreadSafe::<u32>::new(7);
    let mut guard = value
        .r#ref()
        .lock()
        .expect("thread-safe value mutex poisoned");
    *guard = 11;
    drop(guard);
    assert_eq!(value.load(memory_order_seq_cst), 11);
}
