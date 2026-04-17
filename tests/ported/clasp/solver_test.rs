use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::util::indexed_priority_queue::IndexedPriorityQueue;

#[test]
fn indexed_prio_queue_matches_upstream_heap_order_and_updates() {
    let priorities = Rc::new(RefCell::new((0_i32..20).collect::<Vec<_>>()));
    let mut queue = IndexedPriorityQueue::new({
        let priorities = Rc::clone(&priorities);
        move |lhs: u32, rhs: u32| {
            let priorities = priorities.borrow();
            priorities[lhs as usize] < priorities[rhs as usize]
        }
    });

    assert!(queue.empty());
    assert_eq!(queue.size(), 0);

    for (index, value) in (0_u32..20).rev().enumerate() {
        queue.push(value);
        assert!(!queue.empty());
        assert_eq!(queue.size(), index + 1);
        assert_eq!(queue.top(), value);
        assert!(queue.contains(value));
        assert_eq!(queue.index(value), 0);
    }

    assert_eq!(queue.top(), 0);
    let expected_positions = [
        0, 1, 4, 3, 8, 18, 2, 6, 14, 5, 7, 9, 10, 16, 12, 13, 17, 19, 11, 15,
    ];
    for value in 0_u32..20 {
        assert_eq!(queue.index(value), expected_positions[value as usize]);
    }

    priorities.borrow_mut()[0] = 12;
    queue.decrease(0);
    assert_eq!(queue.size(), 20);
    assert_eq!(queue.top(), 1);
    assert_eq!(queue.index(0), 9);

    priorities.borrow_mut()[0] = 0;
    assert_eq!(queue.size(), 20);
    queue.increase(0);
    assert_eq!(queue.top(), 0);
    assert_eq!(queue.index(0), 0);

    queue.pop();
    assert_eq!(queue.size(), 19);
    assert_eq!(queue.top(), 1);
    assert!(!queue.contains(0));

    priorities.borrow_mut()[1] = 21;
    queue.update(1);
    assert_eq!(queue.top(), 2);

    priorities.borrow_mut()[7] = 22;
    queue.update(7);
    priorities.borrow_mut()[3] = 24;
    queue.update(3);
    priorities.borrow_mut()[5] = 28;
    queue.update(5);
    assert_eq!(queue.index(1), 18);
    assert_eq!(queue.index(7), 14);
    assert_eq!(queue.index(3), 17);
    assert_eq!(queue.index(5), 16);

    let mut queue_copy = queue.clone();
    let queue_copy_2 = queue.clone();
    for value in [
        2_u32, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 1, 7, 3, 5,
    ] {
        assert!(!queue.empty());
        assert!(!queue_copy.empty());
        assert_eq!(queue.top(), value);
        assert_eq!(queue_copy.top(), value);
        queue.pop();
        queue_copy.pop();
    }

    assert!(queue.empty());
    assert!(queue_copy.empty());
    assert!(!queue_copy_2.empty());
}
