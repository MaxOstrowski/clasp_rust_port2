use std::fmt;
use std::ops::{Add, Sub};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

pub mod parallel_solve;
pub mod thread;

pub type MemoryOrder = Ordering;
pub type AtomicType<T> = ThreadSafe<T, true>;

#[allow(non_upper_case_globals)]
pub const memory_order_acq_rel: MemoryOrder = MemoryOrder::AcqRel;
#[allow(non_upper_case_globals)]
pub const memory_order_acquire: MemoryOrder = MemoryOrder::Acquire;
#[allow(non_upper_case_globals)]
pub const memory_order_relaxed: MemoryOrder = MemoryOrder::Relaxed;
#[allow(non_upper_case_globals)]
pub const memory_order_release: MemoryOrder = MemoryOrder::Release;
#[allow(non_upper_case_globals)]
pub const memory_order_seq_cst: MemoryOrder = MemoryOrder::SeqCst;

#[must_use]
pub const fn has_threads() -> bool {
    crate::clasp::config::CLASP_HAS_THREADS != 0
}

#[allow(non_snake_case)]
#[must_use]
pub const fn hasThreads() -> bool {
    has_threads()
}

pub struct ThreadSafe<T, const MT: bool = true> {
    value: Mutex<T>,
}

impl<T, const MT: bool> ThreadSafe<T, MT> {
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            value: Mutex::new(value),
        }
    }

    pub fn store(&self, new_value: T, _order: MemoryOrder) {
        *self.value.lock().expect("thread-safe value mutex poisoned") = new_value;
    }

    #[must_use]
    pub fn exchange(&self, new_value: T, _order: MemoryOrder) -> T {
        let mut guard = self.value.lock().expect("thread-safe value mutex poisoned");
        std::mem::replace(&mut *guard, new_value)
    }

    #[must_use]
    pub fn compare_exchange_weak(&self, expected: &mut T, new_value: T, order: MemoryOrder) -> bool
    where
        T: Copy + PartialEq,
    {
        self.compare_exchange_strong(expected, new_value, order)
    }

    #[must_use]
    pub fn compare_exchange_strong(
        &self,
        expected: &mut T,
        new_value: T,
        _order: MemoryOrder,
    ) -> bool
    where
        T: Copy + PartialEq,
    {
        let mut guard = self.value.lock().expect("thread-safe value mutex poisoned");
        if *guard == *expected {
            *guard = new_value;
            true
        } else {
            *expected = *guard;
            false
        }
    }

    #[must_use]
    pub fn r#ref(&self) -> &Mutex<T> {
        &self.value
    }
}

impl<T: Copy, const MT: bool> ThreadSafe<T, MT> {
    #[must_use]
    pub fn load(&self, _order: MemoryOrder) -> T {
        *self.value.lock().expect("thread-safe value mutex poisoned")
    }
}

impl<T, const MT: bool> ThreadSafe<T, MT>
where
    T: Copy + Add<Output = T>,
{
    #[must_use]
    pub fn add(&self, delta: T, _order: MemoryOrder) -> T {
        let mut guard = self.value.lock().expect("thread-safe value mutex poisoned");
        *guard = *guard + delta;
        *guard
    }
}

impl<T, const MT: bool> ThreadSafe<T, MT>
where
    T: Copy + Sub<Output = T>,
{
    #[must_use]
    pub fn sub(&self, delta: T, _order: MemoryOrder) -> T {
        let mut guard = self.value.lock().expect("thread-safe value mutex poisoned");
        *guard = *guard - delta;
        *guard
    }
}

impl<T: Default, const MT: bool> Default for ThreadSafe<T, MT> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug, const MT: bool> fmt::Debug for ThreadSafe<T, MT> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadSafe")
            .field(
                "value",
                &self.value.lock().expect("thread-safe value mutex poisoned"),
            )
            .finish()
    }
}

impl<T, const MT: bool> From<T> for ThreadSafe<T, MT> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Copy + PartialEq, const MT: bool> PartialEq<T> for ThreadSafe<T, MT> {
    fn eq(&self, other: &T) -> bool {
        self.load(memory_order_seq_cst) == *other
    }
}
