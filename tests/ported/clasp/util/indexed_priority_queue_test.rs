use std::cell::RefCell;
use std::rc::Rc;

use rust_clasp::clasp::util::indexed_priority_queue::{
    IndexedPriorityQueue, make_heap, pop_heap, push_heap, replace_heap, sort_heap,
};

#[derive(Debug, PartialEq, Eq)]
struct HeapCon {
    pos: u32,
    act: u32,
}

#[test]
fn indexed_priority_queue_matches_upstream_behavior() {
    let priorities = Rc::new(RefCell::new((0_i32..20).collect::<Vec<_>>()));
    let cmp_priorities = Rc::clone(&priorities);
    let cmp = move |lhs: u32, rhs: u32| {
        let values = cmp_priorities.borrow();
        values[lhs as usize] < values[rhs as usize]
    };

    let mut queue = IndexedPriorityQueue::new(cmp);
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
    let expected = [
        0_usize, 1, 4, 3, 8, 18, 2, 6, 14, 5, 7, 9, 10, 16, 12, 13, 17, 19, 11, 15,
    ];
    for value in 0_u32..20 {
        assert_eq!(queue.index(value), expected[value as usize]);
    }

    priorities.borrow_mut()[0] = 12;
    queue.decrease(0);
    assert_eq!(queue.size(), priorities.borrow().len());
    assert_eq!(queue.top(), 1);
    assert_eq!(queue.index(0), 9);

    priorities.borrow_mut()[0] = 0;
    queue.increase(0);
    assert_eq!(queue.top(), 0);
    assert_eq!(queue.index(0), 0);

    queue.pop();
    assert_eq!(queue.size() + 1, priorities.borrow().len());
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
    let mut queue_copy_2 = queue.clone();
    let expected_pop_order = [
        2_u32, 4, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 1, 7, 3, 5,
    ];
    for expected_top in expected_pop_order {
        assert!(!queue.empty());
        assert!(!queue_copy.empty());
        assert_eq!(queue.top(), expected_top);
        assert_eq!(queue_copy.top(), expected_top);
        queue.pop();
        queue_copy.pop();
    }
    assert!(queue.empty());
    assert!(queue_copy.empty());

    queue_copy_2.remove(1);
    assert!(!queue_copy_2.contains(1));
    assert_eq!(queue_copy_2.size(), 18);
    queue_copy_2.remove(12);
    assert!(!queue_copy_2.contains(12));
    assert_eq!(queue_copy_2.size(), 17);
}

#[test]
fn update_inserts_absent_key() {
    let cmp = |lhs: u32, rhs: u32| lhs < rhs;
    let mut queue = IndexedPriorityQueue::new(cmp);
    queue.update(4);
    assert!(queue.contains(4));
    assert_eq!(queue.top(), 4);
}

#[test]
fn heap_order_simple_matches_upstream_behavior() {
    let mut values = vec![5];
    push_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    assert_eq!(values, vec![5]);

    values.push(3);
    push_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    assert_eq!(values, vec![5, 3]);

    values.push(1);
    push_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    assert_eq!(values, vec![5, 3, 1]);

    pop_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    assert_eq!(values.last(), Some(&5));
    values.pop();
    assert_eq!(values, vec![3, 1]);

    let seed_values = vec![
        37, 8, 71, 19, 54, 90, 12, 63, 25, 44, 86, 3, 58, 77, 31, 66, 14, 49, 95, 22,
    ];
    let mut expected = seed_values.clone();
    expected.sort();

    let mut made_heap = seed_values.clone();
    make_heap(made_heap.as_mut_slice(), |lhs, rhs| lhs < rhs);
    let mut extracted = Vec::new();
    while !made_heap.is_empty() {
        assert_eq!(made_heap[0], *expected.last().unwrap());
        pop_heap(made_heap.as_mut_slice(), |lhs, rhs| lhs < rhs);
        extracted.push(made_heap.pop().unwrap());
        expected.pop();
    }
    assert!(expected.is_empty());
    assert!(extracted.windows(2).all(|pair| pair[0] >= pair[1]));

    let mut pushed_heap = Vec::new();
    for value in seed_values {
        pushed_heap.push(value);
        push_heap(pushed_heap.as_mut_slice(), |lhs, rhs| lhs < rhs);
    }

    let mut expected = pushed_heap.clone();
    expected.sort();
    while !pushed_heap.is_empty() {
        assert_eq!(pushed_heap[0], *expected.last().unwrap());
        pop_heap(pushed_heap.as_mut_slice(), |lhs, rhs| lhs < rhs);
        assert_eq!(pushed_heap.pop().unwrap(), expected.pop().unwrap());
    }
}

#[test]
fn heap_order_complex_matches_upstream_behavior() {
    let mut values = vec![
        HeapCon { pos: 0, act: 236 },
        HeapCon { pos: 1, act: 232 },
        HeapCon { pos: 2, act: 230 },
        HeapCon { pos: 3, act: 234 },
        HeapCon { pos: 4, act: 236 },
        HeapCon { pos: 5, act: 238 },
        HeapCon { pos: 6, act: 246 },
        HeapCon { pos: 7, act: 248 },
        HeapCon { pos: 8, act: 375 },
        HeapCon { pos: 9, act: 238 },
        HeapCon { pos: 10, act: 246 },
        HeapCon { pos: 11, act: 236 },
        HeapCon { pos: 12, act: 242 },
        HeapCon { pos: 13, act: 244 },
        HeapCon { pos: 14, act: 240 },
        HeapCon { pos: 15, act: 244 },
        HeapCon { pos: 16, act: 248 },
        HeapCon { pos: 17, act: 248 },
        HeapCon { pos: 18, act: 248 },
        HeapCon { pos: 19, act: 366 },
        HeapCon { pos: 20, act: 236 },
        HeapCon { pos: 21, act: 244 },
        HeapCon { pos: 22, act: 348 },
        HeapCon { pos: 23, act: 230 },
        HeapCon { pos: 24, act: 354 },
        HeapCon { pos: 25, act: 230 },
        HeapCon { pos: 26, act: 248 },
        HeapCon { pos: 27, act: 248 },
        HeapCon { pos: 28, act: 248 },
        HeapCon { pos: 29, act: 248 },
        HeapCon { pos: 30, act: 357 },
        HeapCon { pos: 31, act: 236 },
    ];

    let mut heap_len = 0_usize;
    let mut max_heap = 25_u32;
    for index in 0..values.len() {
        if max_heap != 0 {
            values.swap(index, heap_len);
            heap_len += 1;
            max_heap -= 1;
            if max_heap == 0 {
                make_heap(&mut values[..heap_len], |lhs, rhs| lhs.act < rhs.act);
            }
        } else if values[index].act > values[0].act {
            let replacement = core::mem::replace(
                &mut values[index],
                HeapCon {
                    pos: u32::MAX,
                    act: u32::MAX,
                },
            );
            values[index] = replace_heap(&mut values[..heap_len], replacement, |lhs, rhs| {
                lhs.act < rhs.act
            });
            assert!(values[index].act >= values[0].act);
        }
    }

    assert_eq!(max_heap, 0);
    let expected_heap = [
        8_u32, 19, 24, 18, 22, 12, 6, 16, 17, 9, 10, 5, 2, 13, 14, 15, 7, 1, 3, 4, 20, 21, 0, 23,
        11,
    ];
    let expected_rest = [25_u32, 26, 27, 28, 29, 30, 31];
    assert_eq!(expected_heap.len() + expected_rest.len(), values.len());

    for (position, expected_pos) in expected_heap.iter().enumerate() {
        assert_eq!(values[position].pos, *expected_pos);
    }

    for expected_pos in expected_rest.iter().rev() {
        assert_eq!(values.last().map(|value| value.pos), Some(*expected_pos));
        values.pop();
    }

    let mut extracted_activity = Vec::new();
    while !values.is_empty() {
        extracted_activity.push(values[0].act);
        pop_heap(values.as_mut_slice(), |lhs, rhs| lhs.act < rhs.act);
        values.pop();
    }
    assert_eq!(extracted_activity.len(), 25);
    assert!(extracted_activity.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn sort_heap_orders_ascending_for_less_comparator() {
    let mut values = vec![8, 3, 5, 1, 9, 2, 6, 4, 7];
    make_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    sort_heap(values.as_mut_slice(), |lhs, rhs| lhs < rhs);
    assert_eq!(values, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}
