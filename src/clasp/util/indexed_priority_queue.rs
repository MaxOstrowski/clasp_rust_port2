//! Rust port of original_clasp/clasp/util/indexed_priority_queue.h.

use crate::clasp::util::pod_vector::PodVector;

pub const NO_POS: usize = usize::MAX;

pub trait QueueKey: Copy + Eq {
    fn to_index(self) -> usize;
}

macro_rules! impl_queue_key {
	($($ty:ty),+ $(,)?) => {
		$(
			impl QueueKey for $ty {
				fn to_index(self) -> usize {
					usize::try_from(self).expect("queue key does not fit into usize")
				}
			}
		)+
	};
}

impl_queue_key!(u8, u16, u32, u64, usize, u128);

pub mod detail {
    use std::ptr;

    pub const fn heap_parent(index: usize) -> usize {
        (index - 1) >> 1
    }

    pub const fn heap_left(index: usize) -> usize {
        (index << 1) + 1
    }

    pub const fn heap_right(index: usize) -> usize {
        (index << 1) + 2
    }

    pub const fn heap_last_non_leaf(len: usize) -> usize {
        (len - 2) >> 1
    }

    pub fn push_heap<T, F>(
        heap: &mut [T],
        top_index: usize,
        value: T,
        mut hole_index: usize,
        cmp: &mut F,
    ) where
        F: FnMut(&T, &T) -> bool,
    {
        let ptr_base = heap.as_mut_ptr();
        while hole_index > top_index {
            let parent = heap_parent(hole_index);
            if !cmp(unsafe { &*ptr_base.add(parent) }, &value) {
                break;
            }
            unsafe {
                ptr::write(ptr_base.add(hole_index), ptr::read(ptr_base.add(parent)));
            }
            hole_index = parent;
        }
        unsafe {
            ptr::write(ptr_base.add(hole_index), value);
        }
    }

    pub fn adjust_heap<T, F>(
        heap: &mut [T],
        len: usize,
        value: T,
        mut hole_index: usize,
        cmp: &mut F,
    ) where
        F: FnMut(&T, &T) -> bool,
    {
        let top_index = hole_index;
        let ptr_base = heap.as_mut_ptr();
        let mut child = hole_index;

        while child < heap_parent(len) {
            child = heap_right(child);
            if cmp(unsafe { &*ptr_base.add(child) }, unsafe {
                &*ptr_base.add(child - 1)
            }) {
                child -= 1;
            }
            unsafe {
                ptr::write(ptr_base.add(hole_index), ptr::read(ptr_base.add(child)));
            }
            hole_index = child;
        }
        if (len & 1) == 0 && child == heap_last_non_leaf(len) {
            child = heap_right(child) - 1;
            unsafe {
                ptr::write(ptr_base.add(hole_index), ptr::read(ptr_base.add(child)));
            }
            hole_index = child;
        }
        push_heap(heap, top_index, value, hole_index, cmp);
    }
}

#[derive(Clone)]
pub struct IndexedPriorityQueue<K, Cmp>
where
    K: QueueKey,
    Cmp: Fn(K, K) -> bool,
{
    indices: PodVector<usize>,
    heap: PodVector<K>,
    compare: Cmp,
}

impl<K, Cmp> IndexedPriorityQueue<K, Cmp>
where
    K: QueueKey,
    Cmp: Fn(K, K) -> bool,
{
    pub fn new(compare: Cmp) -> Self {
        Self {
            indices: PodVector::new(),
            heap: PodVector::new(),
            compare,
        }
    }

    pub fn key_compare(&self) -> &Cmp {
        &self.compare
    }

    pub fn empty(&self) -> bool {
        self.heap.empty()
    }

    pub fn size(&self) -> usize {
        self.heap.size()
    }

    pub fn top(&self) -> K {
        assert!(!self.empty());
        self.heap[0]
    }

    pub fn contains(&self, key: K) -> bool {
        self.index(key) != NO_POS
    }

    pub fn index(&self, key: K) -> usize {
        self.indices.get(key.to_index()).copied().unwrap_or(NO_POS)
    }

    pub fn reserve(&mut self, n: usize) {
        self.indices.reserve(n);
    }

    pub fn push(&mut self, key: K) {
        assert!(!self.contains(key));
        let key_index = key.to_index();
        if key_index >= self.indices.size() {
            if key_index >= self.indices.capacity() {
                self.indices.reserve(((key_index + 1) * 3) >> 1);
            }
            self.indices.resize(key_index + 1, NO_POS);
        }
        self.indices[key_index] = self.heap.size();
        self.heap.push_back(key);
        self.sift_up(self.indices[key_index]);
    }

    pub fn pop(&mut self) {
        assert!(!self.empty());
        let removed = self.heap[0];
        let last = *self.heap.back();
        self.heap[0] = last;
        self.indices[last.to_index()] = 0;
        self.indices[removed.to_index()] = NO_POS;
        self.heap.pop_back();
        if self.heap.size() > 1 {
            self.sift_down(0);
        }
    }

    pub fn remove(&mut self, key: K) {
        let pos = self.index(key);
        if pos != NO_POS {
            let last = *self.heap.back();
            self.assign(pos, last);
            self.indices[key.to_index()] = NO_POS;
            self.heap.pop_back();
            if pos < self.heap.size() {
                self.sift_up(pos);
                self.sift_down(pos);
            }
        }
    }

    pub fn clear(&mut self) {
        self.heap.clear();
        self.indices.clear();
    }

    pub fn update(&mut self, key: K) {
        if !self.contains(key) {
            self.push(key);
        } else {
            let pos = self.indices[key.to_index()];
            self.sift_up(pos);
            self.sift_down(pos);
        }
    }

    pub fn increase(&mut self, key: K) {
        assert!(self.contains(key));
        self.sift_up(self.indices[key.to_index()]);
    }

    pub fn decrease(&mut self, key: K) {
        assert!(self.contains(key));
        self.sift_down(self.indices[key.to_index()]);
    }

    fn assign(&mut self, pos: usize, value: K) {
        self.heap[pos] = value;
        self.indices[value.to_index()] = pos;
    }

    fn sift_up(&mut self, mut index: usize) {
        let value = self.heap[index];
        while index != 0 {
            let parent = detail::heap_parent(index);
            if !(self.compare)(value, self.heap[parent]) {
                break;
            }
            let parent_value = self.heap[parent];
            self.assign(index, parent_value);
            index = parent;
        }
        self.assign(index, value);
    }

    fn sift_down(&mut self, mut index: usize) {
        let value = self.heap[index];
        let size = self.heap.size();
        loop {
            let left = detail::heap_left(index);
            if left >= size {
                break;
            }
            let mut child = left;
            let right = child + 1;
            if right < size && (self.compare)(self.heap[right], self.heap[child]) {
                child = right;
            }
            if !(self.compare)(self.heap[child], value) {
                break;
            }
            let child_value = self.heap[child];
            self.assign(index, child_value);
            index = child;
        }
        self.assign(index, value);
    }
}

pub fn push_heap<T, F>(heap: &mut [T], mut cmp: F)
where
    F: FnMut(&T, &T) -> bool,
{
    assert!(!heap.is_empty());
    let last = heap.len() - 1;
    let value = unsafe { std::ptr::read(heap.as_ptr().add(last)) };
    detail::push_heap(heap, 0, value, last, &mut cmp);
}

pub fn pop_heap<T, F>(heap: &mut [T], mut cmp: F)
where
    F: FnMut(&T, &T) -> bool,
{
    assert!(!heap.is_empty());
    if heap.len() > 1 {
        let end = heap.len() - 1;
        let value = unsafe { std::ptr::read(heap.as_ptr().add(end)) };
        unsafe {
            std::ptr::write(heap.as_mut_ptr().add(end), std::ptr::read(heap.as_ptr()));
        }
        detail::adjust_heap(&mut heap[..end], end, value, 0, &mut cmp);
    }
}

pub fn replace_heap<T, F>(heap: &mut [T], value: T, mut cmp: F) -> T
where
    F: FnMut(&T, &T) -> bool,
{
    assert!(!heap.is_empty());
    let old = unsafe { std::ptr::read(heap.as_ptr()) };
    detail::adjust_heap(heap, heap.len(), value, 0, &mut cmp);
    old
}

pub fn make_heap<T, F>(heap: &mut [T], mut cmp: F)
where
    F: FnMut(&T, &T) -> bool,
{
    if heap.len() <= 1 {
        return;
    }
    let mut parent = detail::heap_last_non_leaf(heap.len());
    loop {
        let value = unsafe { std::ptr::read(heap.as_ptr().add(parent)) };
        detail::adjust_heap(heap, heap.len(), value, parent, &mut cmp);
        if parent == 0 {
            break;
        }
        parent -= 1;
    }
}

pub fn sort_heap<T, F>(heap: &mut [T], mut cmp: F)
where
    F: FnMut(&T, &T) -> bool,
{
    let mut len = heap.len();
    while len > 1 {
        len -= 1;
        let value = unsafe { std::ptr::read(heap.as_ptr().add(len)) };
        unsafe {
            std::ptr::write(heap.as_mut_ptr().add(len), std::ptr::read(heap.as_ptr()));
        }
        detail::adjust_heap(&mut heap[..len], len, value, 0, &mut cmp);
    }
}
