use std::sync::Arc;
use std::time::Duration;

use rust_clasp::clasp::mt::thread::{
    ConditionVariable, DEFER_LOCK, Mutex, Thread, UniqueLock, this_thread, to_millis,
};

struct EventVar {
    fired: Mutex<bool>,
    cond: ConditionVariable,
}

impl EventVar {
    fn new() -> Self {
        Self {
            fired: Mutex::new(false),
            cond: ConditionVariable::new(),
        }
    }

    fn fire(&self) {
        {
            let mut lock = UniqueLock::new(&self.fired);
            *lock = true;
        }
        self.cond.notify_all();
    }

    fn wait(&self) {
        let mut lock = UniqueLock::new(&self.fired);
        while !*lock {
            self.cond.wait(&mut lock);
        }
    }
}

#[test]
fn to_millis_truncates_fractional_seconds() {
    assert_eq!(to_millis(0.0), Duration::ZERO);
    assert_eq!(to_millis(0.0009), Duration::ZERO);
    assert_eq!(to_millis(0.001), Duration::from_millis(1));
    assert_eq!(to_millis(1.999), Duration::from_millis(1999));
}

#[test]
fn unique_lock_can_defer_and_reacquire() {
    let mutex = Mutex::new(7_u32);
    let mut lock = UniqueLock::with_defer_lock(&mutex, DEFER_LOCK);

    assert!(!lock.owns_lock());

    lock.lock();
    assert!(lock.owns_lock());
    assert_eq!(*lock, 7);

    *lock = 9;
    lock.unlock();
    assert!(!lock.owns_lock());

    lock.lock();
    assert_eq!(*lock, 9);
}

#[test]
fn condition_variable_wait_and_notify_match_event_pattern() {
    let event = Arc::new(EventVar::new());
    let waiter = Arc::clone(&event);
    let mut thread = Thread::spawn(move || {
        waiter.wait();
    });

    event.fire();

    thread
        .join()
        .expect("thread should be joinable")
        .expect("waiter thread should finish without panic");
}

#[test]
fn condition_variable_wait_for_reports_timeout_and_notification() {
    let event = Arc::new(EventVar::new());
    let waiter = Arc::clone(&event);
    let mut thread = Thread::spawn(move || {
        let mut lock = UniqueLock::new(&waiter.fired);
        let timeout = waiter.cond.wait_for(&mut lock, Duration::from_millis(10));
        let timed_out = timeout.timed_out();
        if timed_out {
            waiter.cond.wait(&mut lock);
        }
        (*lock, timed_out)
    });

    std::thread::sleep(Duration::from_millis(20));
    event.fire();

    let (fired, timed_out) = thread
        .join()
        .expect("thread should be joinable")
        .expect("waiter thread should finish without panic");
    assert!(timed_out);
    assert!(fired);
}

#[test]
fn thread_wrapper_exposes_joinability_and_hardware_concurrency() {
    let mut thread = Thread::spawn(|| 42_u32);

    assert!(thread.joinable());
    assert!(Thread::<()>::hardware_concurrency() >= 1);
    this_thread::yield_now();
    assert_eq!(
        thread
            .join()
            .expect("thread should be joinable")
            .expect("worker thread should finish without panic"),
        42
    );
    assert!(!thread.joinable());
}
