//! Rust port of `original_clasp/clasp/mt/thread.h`.

use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

pub use std::mem::swap;
pub use std::sync::Mutex;

pub type LockGuard<'a, T> = MutexGuard<'a, T>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeferLock;

pub const DEFER_LOCK: DeferLock = DeferLock;

fn recover_lock<T>(result: Result<T, PoisonError<T>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => error.into_inner(),
    }
}

#[derive(Debug)]
pub struct UniqueLock<'a, T> {
    mutex: &'a Mutex<T>,
    guard: Option<MutexGuard<'a, T>>,
}

impl<'a, T> UniqueLock<'a, T> {
    #[must_use]
    pub fn new(mutex: &'a Mutex<T>) -> Self {
        Self {
            mutex,
            guard: Some(recover_lock(mutex.lock())),
        }
    }

    #[must_use]
    pub fn with_defer_lock(mutex: &'a Mutex<T>, _: DeferLock) -> Self {
        Self { mutex, guard: None }
    }

    #[must_use]
    pub fn mutex(&self) -> &'a Mutex<T> {
        self.mutex
    }

    #[must_use]
    pub fn owns_lock(&self) -> bool {
        self.guard.is_some()
    }

    pub fn lock(&mut self) {
        if self.guard.is_none() {
            self.guard = Some(recover_lock(self.mutex.lock()));
        }
    }

    pub fn unlock(&mut self) {
        self.guard.take();
    }

    fn take_guard(&mut self) -> MutexGuard<'a, T> {
        self.guard
            .take()
            .expect("UniqueLock must own the mutex before waiting")
    }

    fn replace_guard(&mut self, guard: MutexGuard<'a, T>) {
        self.guard = Some(guard);
    }
}

impl<T> Deref for UniqueLock<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_deref()
            .expect("UniqueLock must own the mutex before dereferencing")
    }
}

impl<T> DerefMut for UniqueLock<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_deref_mut()
            .expect("UniqueLock must own the mutex before mutably dereferencing")
    }
}

#[derive(Debug, Default)]
pub struct ConditionVariable {
    inner: Condvar,
}

impl ConditionVariable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    pub fn notify_all(&self) {
        self.inner.notify_all();
    }

    pub fn wait<'a, T>(&self, lock: &mut UniqueLock<'a, T>) {
        let guard = recover_lock(self.inner.wait(lock.take_guard()));
        lock.replace_guard(guard);
    }

    pub fn wait_for<'a, T>(
        &self,
        lock: &mut UniqueLock<'a, T>,
        timeout: Duration,
    ) -> std::sync::WaitTimeoutResult {
        let (guard, timeout_result) =
            recover_lock(self.inner.wait_timeout(lock.take_guard(), timeout));
        lock.replace_guard(guard);
        timeout_result
    }
}

#[derive(Debug, Default)]
pub struct Thread<T = ()> {
    handle: Option<JoinHandle<T>>,
}

impl<T> Thread<T> {
    #[must_use]
    pub fn from_handle(handle: JoinHandle<T>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    #[must_use]
    pub fn joinable(&self) -> bool {
        self.handle.is_some()
    }

    pub fn join(&mut self) -> Option<std::thread::Result<T>> {
        self.handle.take().map(JoinHandle::join)
    }

    pub fn detach(&mut self) {
        self.handle.take();
    }

    #[must_use]
    pub fn hardware_concurrency() -> u32 {
        std::thread::available_parallelism()
            .map(|count| count.get().min(u32::MAX as usize) as u32)
            .unwrap_or(0)
    }
}

impl<T: Send + 'static> Thread<T> {
    #[must_use]
    pub fn spawn<F>(f: F) -> Self
    where
        F: FnOnce() -> T + Send + 'static,
    {
        Self::from_handle(std::thread::spawn(f))
    }
}

pub mod this_thread {
    pub use std::thread::yield_now;
}

#[must_use]
pub fn to_millis(seconds: f64) -> Duration {
    let millis = (seconds * 1000.0).trunc();
    if !millis.is_finite() || millis <= 0.0 {
        Duration::ZERO
    } else if millis >= u64::MAX as f64 {
        Duration::from_millis(u64::MAX)
    } else {
        Duration::from_millis(millis as u64)
    }
}
