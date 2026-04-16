use std::collections::BTreeSet;
use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use rust_clasp::clasp::util::multi_queue::{
    LockFreeStack, LockFreeStackNode, MpScPtrQueue, MpScPtrQueueNode, MultiQueue,
};

#[derive(Debug)]
struct DropTracker {
    value: u32,
    drops: Arc<AtomicUsize>,
}

impl Drop for DropTracker {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::AcqRel);
    }
}

#[test]
fn lock_free_stack_pushes_and_pops_in_lifo_order() {
    let stack = LockFreeStack::new();
    let first = Box::into_raw(Box::new(LockFreeStackNode::new()));
    let second = Box::into_raw(Box::new(LockFreeStackNode::new()));
    let third = Box::into_raw(Box::new(LockFreeStackNode::new()));

    unsafe {
        stack.push(first);
        stack.push(second);
        stack.push(third);
    }

    assert_eq!(stack.try_pop(), third);
    assert_eq!(stack.try_pop(), second);
    assert_eq!(stack.try_pop(), first);
    assert!(stack.try_pop().is_null());
    assert!(stack.release().is_null());

    // SAFETY: each node was allocated via `Box::into_raw` and popped exactly once.
    unsafe {
        drop(Box::from_raw(first));
        drop(Box::from_raw(second));
        drop(Box::from_raw(third));
    }
}

#[test]
fn multi_queue_delivers_each_item_to_every_thread_in_order() {
    let queue = MultiQueue::<DropTracker>::new(2);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut left = queue.add_thread();
    let mut right = queue.add_thread();

    queue.publish(DropTracker {
        value: 10,
        drops: Arc::clone(&drops),
    });
    queue.publish(DropTracker {
        value: 20,
        drops: Arc::clone(&drops),
    });

    assert!(queue.has_items(&left));
    assert!(queue.has_items(&right));

    let left_first = queue.try_consume(&mut left).expect("first item missing");
    let right_first = queue.try_consume(&mut right).expect("first item missing");
    let left_second = queue.try_consume(&mut left).expect("second item missing");
    let right_second = queue.try_consume(&mut right).expect("second item missing");

    // SAFETY: returned pointers remain valid until the next consume on the same thread id.
    unsafe {
        assert_eq!(left_first.as_ref().value, 10);
        assert_eq!(right_first.as_ref().value, 10);
        assert_eq!(left_second.as_ref().value, 20);
        assert_eq!(right_second.as_ref().value, 20);
    }

    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert!(!queue.has_items(&left));
    assert!(!queue.has_items(&right));

    drop(queue);
    assert_eq!(drops.load(Ordering::Acquire), 2);
}

#[test]
fn multi_queue_safe_publish_accepts_multiple_producers() {
    let queue = Arc::new(MultiQueue::<usize>::new(1));
    let mut consumer = queue.add_thread();
    let expected = 24_usize;
    let mut producers = Vec::new();
    for value in 0..expected {
        let queue = Arc::clone(&queue);
        producers.push(thread::spawn(move || {
            queue.publish(value);
        }));
    }
    for producer in producers {
        producer.join().expect("producer thread should not panic");
    }

    let mut seen = BTreeSet::new();
    while seen.len() < expected {
        if let Some(item) = queue.try_consume(&mut consumer) {
            // SAFETY: the returned item is valid until the next consume for this thread id.
            let value = unsafe { *item.as_ref() };
            seen.insert(value);
        } else {
            thread::yield_now();
        }
    }

    let all_values = (0..expected).collect::<BTreeSet<_>>();
    assert_eq!(seen, all_values);
}

#[test]
fn multi_queue_unsafe_publish_preserves_single_threaded_fifo() {
    let queue = MultiQueue::<u32>::new(1);
    let mut consumer = queue.add_thread();

    queue.unsafe_publish(7);
    queue.unsafe_publish(9);

    let first = queue
        .try_consume(&mut consumer)
        .expect("first item missing");
    let second = queue
        .try_consume(&mut consumer)
        .expect("second item missing");

    // SAFETY: the returned item is valid until the next consume for this thread id.
    unsafe {
        assert_eq!(*first.as_ref(), 7);
        assert_eq!(*second.as_ref(), 9);
    }
    assert!(queue.try_consume(&mut consumer).is_none());
}

#[test]
fn mpsc_ptr_queue_moves_payloads_through_the_sentinel_nodes() {
    let mut sentinel = Box::new(MpScPtrQueueNode::new());
    let queue = MpScPtrQueue::<64>::new(&mut sentinel);
    let first_payload = Box::into_raw(Box::new(11_usize));
    let second_payload = Box::into_raw(Box::new(17_usize));
    let first = Box::into_raw(Box::new(MpScPtrQueueNode::new()));
    let second = Box::into_raw(Box::new(MpScPtrQueueNode::new()));

    // SAFETY: test owns both nodes and payload allocations.
    unsafe {
        (*first).data = first_payload.cast::<c_void>();
        (*second).data = second_payload.cast::<c_void>();
    }

    assert!(queue.empty());
    unsafe {
        queue.push(first);
        queue.push(second);
    }
    assert!(!queue.empty());

    let popped_first = queue.pop();
    let popped_second = queue.pop();

    assert_eq!(popped_first, ptr::from_mut(&mut *sentinel));
    assert_eq!(popped_second, first);
    assert!(queue.pop().is_null());
    assert!(queue.empty());

    // SAFETY: payload pointers were written into queue nodes above and transferred by `pop`.
    unsafe {
        assert_eq!(*((*popped_first).data.cast::<usize>()), 11);
        assert_eq!(*((*popped_second).data.cast::<usize>()), 17);
        drop(Box::from_raw((*popped_first).data.cast::<usize>()));
        drop(Box::from_raw((*popped_second).data.cast::<usize>()));
        drop(Box::from_raw(first));
        drop(Box::from_raw(second));
    }
}
