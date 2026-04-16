//! Rust port of `original_clasp/clasp/util/multi_queue.h`.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::{MaybeUninit, size_of};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

#[derive(Debug, Default)]
pub struct LockFreeStackNode {
    next: AtomicPtr<LockFreeStackNode>,
}

impl LockFreeStackNode {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

#[derive(Debug, Default)]
pub struct LockFreeStack {
    top: AtomicPtr<LockFreeStackNode>,
}

impl LockFreeStack {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            top: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// # Safety
    ///
    /// `node` must point to a valid detached node that remains allocated while
    /// it is reachable from this stack.
    pub unsafe fn push(&self, node: *mut LockFreeStackNode) {
        let mut assumed_top = self.top.load(Ordering::Acquire);
        loop {
            // SAFETY: `node` is owned by the caller and linked only through this stack.
            unsafe {
                (*node).next.store(assumed_top, Ordering::Relaxed);
            }
            match self.top.compare_exchange_weak(
                assumed_top,
                node,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => assumed_top = actual,
            }
        }
    }

    #[must_use]
    pub fn try_pop(&self) -> *mut LockFreeStackNode {
        let mut node = self.top.load(Ordering::Acquire);
        loop {
            if node.is_null() {
                return ptr::null_mut();
            }
            // SAFETY: `node` came from the stack head and remains valid until CAS succeeds.
            let next = unsafe { (*node).next.load(Ordering::Relaxed) };
            match self
                .top
                .compare_exchange_weak(node, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return node,
                Err(actual) => node = actual,
            }
        }
    }

    #[must_use]
    pub fn release(&self) -> *mut LockFreeStackNode {
        self.top.swap(ptr::null_mut(), Ordering::AcqRel)
    }
}

#[derive(Debug)]
#[repr(C)]
struct SharedNode<T> {
    base: LockFreeStackNode,
    refs: AtomicU32,
    value: MaybeUninit<T>,
}

impl<T> SharedNode<T> {
    fn new_uninit() -> Self {
        Self {
            base: LockFreeStackNode::new(),
            refs: AtomicU32::new(0),
            value: MaybeUninit::uninit(),
        }
    }

    unsafe fn value_ptr(node: *mut SharedNode<T>) -> *mut T {
        // SAFETY: caller ensures `node` points to an initialized `SharedNode<T>`.
        unsafe { (*node).value.as_mut_ptr() }
    }
}

#[derive(Debug)]
pub struct MultiQueue<T: Send + Sync> {
    head: *mut LockFreeStackNode,
    tail: AtomicPtr<LockFreeStackNode>,
    free: LockFreeStack,
    max_q: u32,
    _marker: PhantomData<T>,
}

pub type ThreadId = *mut LockFreeStackNode;

impl<T: Send + Sync> MultiQueue<T> {
    #[must_use]
    pub fn new(max_threads: u32) -> Self {
        let head = Box::into_raw(Box::new(LockFreeStackNode::new()));
        Self {
            head,
            tail: AtomicPtr::new(head),
            free: LockFreeStack::new(),
            max_q: max_threads,
            _marker: PhantomData,
        }
    }

    pub fn reserve(&self, n: u32) {
        for _ in 0..n {
            let node = Box::into_raw(Box::new(SharedNode::<T>::new_uninit()));
            // SAFETY: reserved nodes are detached and owned by this queue.
            unsafe {
                self.free.push(Self::base_ptr(node));
            }
        }
    }

    #[must_use]
    pub const fn max_threads(&self) -> u32 {
        self.max_q
    }

    #[must_use]
    pub fn add_thread(&self) -> ThreadId {
        self.head
    }

    #[must_use]
    pub fn has_items(&self, consumer_id: &ThreadId) -> bool {
        *consumer_id != self.tail.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn try_consume(&self, consumer_id: &mut ThreadId) -> Option<NonNull<T>> {
        if *consumer_id == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let old = *consumer_id;
        // SAFETY: `consumer_id` was obtained from `add_thread` or a prior successful consume.
        let next = unsafe { (*old).next.load(Ordering::Acquire) };
        assert!(!next.is_null(), "MultiQueue is corrupted!");
        *consumer_id = next;
        self.release_node(old);
        let node = Self::to_node(next);
        // SAFETY: `next` is a shared data node whose value was initialized during publish.
        Some(unsafe { NonNull::new_unchecked(SharedNode::value_ptr(node)) })
    }

    pub fn publish(&self, value: T) {
        self.publish_safe(self.allocate(value));
    }

    pub fn unsafe_publish(&self, value: T) {
        self.publish_unsafe(self.allocate(value));
    }

    fn base_ptr(node: *mut SharedNode<T>) -> *mut LockFreeStackNode {
        node.cast::<LockFreeStackNode>()
    }

    fn to_node(node: *mut LockFreeStackNode) -> *mut SharedNode<T> {
        node.cast::<SharedNode<T>>()
    }

    fn destroy(&self, mut head: *mut LockFreeStackNode, destruct_value: bool) {
        while !head.is_null() {
            let node = Self::to_node(head);
            // SAFETY: `head` points to a node in one of the intrusive lists owned by the queue.
            head = unsafe { (*head).next.load(Ordering::Relaxed) };
            if destruct_value {
                // SAFETY: active-list nodes always hold initialized values.
                unsafe {
                    ptr::drop_in_place(SharedNode::value_ptr(node));
                }
            }
            // SAFETY: node was allocated via `Box::into_raw` and is no longer reachable.
            unsafe {
                drop(Box::from_raw(node));
            }
        }
    }

    fn allocate(&self, value: T) -> *mut LockFreeStackNode {
        let node = match NonNull::new(Self::to_node(self.free.try_pop())) {
            Some(node) => node.as_ptr(),
            None => Box::into_raw(Box::new(SharedNode::<T>::new_uninit())),
        };
        // SAFETY: `node` is uniquely owned here, either freshly allocated or reclaimed from free list.
        unsafe {
            (*node).base.next.store(ptr::null_mut(), Ordering::Relaxed);
            (*node).refs.store(self.max_q, Ordering::Relaxed);
            SharedNode::value_ptr(node).write(value);
        }
        Self::base_ptr(node)
    }

    fn release_node(&self, node: *mut LockFreeStackNode) {
        if node == self.head {
            return;
        }
        let shared = Self::to_node(node);
        // SAFETY: `shared` belongs to this queue and `refs` was initialized in `allocate`.
        let last = unsafe { (*shared).refs.fetch_sub(1, Ordering::AcqRel) == 1 };
        if last {
            // SAFETY: `node` remains valid until it is pushed onto the free stack below.
            let next = unsafe { (*node).next.load(Ordering::Relaxed) };
            // SAFETY: `head` is a permanent sentinel node.
            unsafe {
                (*self.head).next.store(next, Ordering::Relaxed);
                ptr::drop_in_place(SharedNode::value_ptr(shared));
            }
            // SAFETY: reclaimed queue nodes are detached before returning to the free stack.
            unsafe {
                self.free.push(node);
            }
        }
    }

    fn publish_safe(&self, new_node: *mut LockFreeStackNode) {
        // SAFETY: `new_node` was freshly allocated and not linked yet.
        unsafe {
            debug_assert!((*new_node).next.load(Ordering::Relaxed).is_null());
        }
        loop {
            let assumed_tail = self.tail.load(Ordering::Acquire);
            // SAFETY: `assumed_tail` always points to the sentinel or a published node.
            let assumed_next = unsafe { (*assumed_tail).next.load(Ordering::Acquire) };
            if assumed_tail != self.tail.load(Ordering::Acquire) {
                continue;
            }
            if !assumed_next.is_null() {
                let _ = self.tail.compare_exchange_weak(
                    assumed_tail,
                    assumed_next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                continue;
            }
            // SAFETY: `assumed_tail` is still current and linking happens via CAS on its next pointer.
            let linked = unsafe {
                (*assumed_tail).next.compare_exchange_weak(
                    ptr::null_mut(),
                    new_node,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
            };
            if linked.is_ok() {
                let _ = self.tail.compare_exchange(
                    assumed_tail,
                    new_node,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                break;
            }
        }
    }

    fn publish_unsafe(&self, new_node: *mut LockFreeStackNode) {
        let tail = self.tail.load(Ordering::Acquire);
        // SAFETY: caller upholds the non-concurrent requirement of `unsafe_publish`.
        unsafe {
            (*tail).next.store(new_node, Ordering::Relaxed);
        }
        self.tail.store(new_node, Ordering::Release);
    }
}

impl<T: Send + Sync> Drop for MultiQueue<T> {
    fn drop(&mut self) {
        self.destroy(
            // SAFETY: `head` is the queue's permanent sentinel.
            unsafe { (*self.head).next.load(Ordering::Relaxed) },
            true,
        );
        self.destroy(self.free.release(), false);
        // SAFETY: `head` was allocated with `Box::into_raw` in `new`.
        unsafe {
            drop(Box::from_raw(self.head));
        }
    }
}

// SAFETY: all shared mutation happens through atomics; values are published immutably.
unsafe impl<T: Send + Sync> Send for MultiQueue<T> {}
// SAFETY: concurrent producers and independent consumers only observe immutable payloads.
unsafe impl<T: Send + Sync> Sync for MultiQueue<T> {}

#[derive(Debug)]
#[repr(C)]
pub struct MpScPtrQueueNode {
    base: LockFreeStackNode,
    pub data: *mut c_void,
}

impl Default for MpScPtrQueueNode {
    fn default() -> Self {
        Self::new()
    }
}

impl MpScPtrQueueNode {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base: LockFreeStackNode::new(),
            data: ptr::null_mut(),
        }
    }
}

#[derive(Debug)]
pub struct MpScPtrQueue<const ALIGN_SIZE: usize = { size_of::<*const ()>() }> {
    head: AtomicPtr<MpScPtrQueueNode>,
    tail: AtomicPtr<MpScPtrQueueNode>,
    _align: PhantomData<[u8; ALIGN_SIZE]>,
}

impl<const ALIGN_SIZE: usize> MpScPtrQueue<ALIGN_SIZE> {
    #[must_use]
    pub fn new(sentinel: &mut MpScPtrQueueNode) -> Self {
        sentinel.base.next.store(ptr::null_mut(), Ordering::Relaxed);
        sentinel.data = ptr::null_mut();
        let ptr = ptr::from_mut(sentinel);
        Self {
            head: AtomicPtr::new(ptr),
            tail: AtomicPtr::new(ptr),
            _align: PhantomData,
        }
    }

    #[must_use]
    pub fn to_node(node: *mut LockFreeStackNode) -> *mut MpScPtrQueueNode {
        node.cast::<MpScPtrQueueNode>()
    }

    #[must_use]
    pub fn empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        // SAFETY: `tail` always points to the sentinel or the last consumer node.
        unsafe { (*tail).base.next.load(Ordering::Acquire).is_null() }
    }

    /// # Safety
    ///
    /// `node` must point to a valid detached queue node that remains allocated
    /// until the consumer pops it again.
    pub unsafe fn push(&self, node: *mut MpScPtrQueueNode) {
        // SAFETY: producers only link detached nodes.
        unsafe {
            (*node).base.next.store(ptr::null_mut(), Ordering::Relaxed);
        }
        let previous = self.head.swap(node, Ordering::AcqRel);
        // SAFETY: `previous` is the former producer head and remains valid until linked.
        unsafe {
            (*previous)
                .base
                .next
                .store(node.cast::<LockFreeStackNode>(), Ordering::Release);
        }
    }

    #[must_use]
    pub fn pop(&self) -> *mut MpScPtrQueueNode {
        let tail = self.tail.load(Ordering::Acquire);
        // SAFETY: single-consumer semantics ensure exclusive updates to `tail` and node payload shuffling.
        unsafe {
            let next = Self::to_node((*tail).base.next.load(Ordering::Acquire));
            if next.is_null() {
                return ptr::null_mut();
            }
            self.tail.store(next, Ordering::Release);
            (*tail).data = (*next).data;
            (*next).data = ptr::null_mut();
            tail
        }
    }
}

// SAFETY: producers synchronize through atomics and the single consumer updates `tail`.
unsafe impl<const ALIGN_SIZE: usize> Send for MpScPtrQueue<ALIGN_SIZE> {}
// SAFETY: shared access is restricted to the MPSC algorithm's atomic operations.
unsafe impl<const ALIGN_SIZE: usize> Sync for MpScPtrQueue<ALIGN_SIZE> {}
