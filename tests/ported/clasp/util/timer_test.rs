use std::cell::RefCell;
use std::collections::VecDeque;

use rust_clasp::clasp::util::timer::{
    ProcessTime, RealTime, ThreadTime, TimeSource, Timer, diff_time_unchecked, is_valid_time,
};

thread_local! {
    static MOCK_TIMES: RefCell<VecDeque<f64>> = const { RefCell::new(VecDeque::new()) };
}

#[derive(Clone, Copy, Debug, Default)]
struct MockTime;

impl TimeSource for MockTime {
    fn get_time() -> f64 {
        MOCK_TIMES.with(|times| {
            times
                .borrow_mut()
                .pop_front()
                .expect("mock timer queue exhausted")
        })
    }

    fn diff_time(t_end: f64, t_start: f64) -> f64 {
        diff_time_unchecked(t_end, t_start)
    }
}

fn set_mock_times(values: &[f64]) {
    MOCK_TIMES.with(|times| {
        *times.borrow_mut() = values.iter().copied().collect();
    });
}

#[test]
fn diff_time_helpers_match_upstream_semantics() {
    assert_eq!(diff_time_unchecked(5.0, 3.0), 2.0);
    assert_eq!(diff_time_unchecked(3.0, 5.0), 0.0);
    assert!(!is_valid_time(f64::NAN));
    assert!(!is_valid_time(f64::INFINITY));
    assert!(is_valid_time(0.0));
    assert_eq!(RealTime::diff_time(f64::NAN, 1.0), 0.0);
}

#[test]
fn process_and_thread_diff_time_preserve_invalid_values() {
    assert!(ProcessTime::diff_time(f64::NAN, 1.0).is_nan());
    assert_eq!(ProcessTime::diff_time(1.0, f64::INFINITY), f64::INFINITY);
    assert!(ThreadTime::diff_time(f64::NAN, 1.0).is_nan());
    assert_eq!(
        ThreadTime::diff_time(1.0, f64::NEG_INFINITY),
        f64::NEG_INFINITY
    );
}

#[test]
fn timer_accumulates_elapsed_time_across_starts_and_stops() {
    set_mock_times(&[1.0, 3.5, 5.0, 8.0]);
    let mut timer = Timer::<MockTime>::new();

    timer.start();
    timer.stop();
    assert_eq!(timer.elapsed(), 2.5);
    assert_eq!(timer.total(), 2.5);

    timer.start();
    timer.stop();
    assert_eq!(timer.elapsed(), 3.0);
    assert_eq!(timer.total(), 5.5);
}

#[test]
fn timer_lap_restarts_from_the_split_point() {
    set_mock_times(&[1.0, 2.0, 5.0]);
    let mut timer = Timer::<MockTime>::new();

    timer.start();
    timer.lap();
    assert_eq!(timer.elapsed(), 1.0);
    assert_eq!(timer.total(), 1.0);

    timer.stop();
    assert_eq!(timer.elapsed(), 3.0);
    assert_eq!(timer.total(), 4.0);
}

#[test]
fn timer_reset_restores_the_default_state() {
    set_mock_times(&[10.0, 12.0]);
    let mut timer = Timer::<MockTime>::new();
    timer.start();
    timer.stop();
    timer.reset();

    assert_eq!(timer.elapsed(), 0.0);
    assert_eq!(timer.total(), 0.0);
}

#[test]
fn process_and_thread_time_are_available_in_this_environment() {
    assert!(ProcessTime::get_time() >= 0.0);
    assert!(ThreadTime::get_time() >= 0.0);
}

#[test]
fn real_time_get_time_is_finite_and_monotonic() {
    let first = RealTime::get_time();
    let second = RealTime::get_time();

    assert!(first.is_finite());
    assert!(second.is_finite());
    assert!(second >= first);
}
