//! Rust port of original_clasp/clasp/util/timer.h and
//! original_clasp/src/timer.cpp.

use std::marker::PhantomData;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::potassco::platform;

#[must_use]
pub const fn diff_time_unchecked(t_end: f64, t_start: f64) -> f64 {
    let diff = t_end - t_start;
    if diff >= 0.0 { diff } else { 0.0 }
}

#[must_use]
pub fn is_valid_time(value: f64) -> bool {
    value.is_finite()
}

pub trait TimeSource {
    fn get_time() -> f64;

    fn diff_time(t_end: f64, t_start: f64) -> f64;

    fn diff_since(t_start: f64) -> f64 {
        Self::diff_time(Self::get_time(), t_start)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessTime;

impl ProcessTime {
    #[must_use]
    pub fn get_time() -> f64 {
        <Self as TimeSource>::get_time()
    }

    #[must_use]
    pub fn diff_time(t_end: f64, t_start: f64) -> f64 {
        <Self as TimeSource>::diff_time(t_end, t_start)
    }

    #[must_use]
    pub fn diff_since(t_start: f64) -> f64 {
        <Self as TimeSource>::diff_since(t_start)
    }
}

impl TimeSource for ProcessTime {
    fn get_time() -> f64 {
        platform::get_process_time()
    }

    fn diff_time(t_end: f64, t_start: f64) -> f64 {
        diff_time_checked::<Self>(t_start, Some(t_end))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadTime;

impl ThreadTime {
    #[must_use]
    pub fn get_time() -> f64 {
        <Self as TimeSource>::get_time()
    }

    #[must_use]
    pub fn diff_time(t_end: f64, t_start: f64) -> f64 {
        <Self as TimeSource>::diff_time(t_end, t_start)
    }

    #[must_use]
    pub fn diff_since(t_start: f64) -> f64 {
        <Self as TimeSource>::diff_since(t_start)
    }
}

impl TimeSource for ThreadTime {
    fn get_time() -> f64 {
        platform::get_thread_time()
    }

    fn diff_time(t_end: f64, t_start: f64) -> f64 {
        diff_time_checked::<Self>(t_start, Some(t_end))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RealTime;

impl RealTime {
    #[must_use]
    pub fn get_time() -> f64 {
        <Self as TimeSource>::get_time()
    }

    #[must_use]
    pub fn diff_time(t_end: f64, t_start: f64) -> f64 {
        <Self as TimeSource>::diff_time(t_end, t_start)
    }

    #[must_use]
    pub fn diff_since(t_start: f64) -> f64 {
        <Self as TimeSource>::diff_since(t_start)
    }
}

impl TimeSource for RealTime {
    fn get_time() -> f64 {
        static START: OnceLock<Instant> = OnceLock::new();
        to_seconds(START.get_or_init(Instant::now).elapsed())
    }

    fn diff_time(t_end: f64, t_start: f64) -> f64 {
        diff_time_unchecked(t_end, t_start)
    }
}

#[derive(Debug)]
pub struct Timer<T: TimeSource> {
    start: f64,
    split: f64,
    total: f64,
    _marker: PhantomData<fn() -> T>,
}

impl<T: TimeSource> Default for Timer<T> {
    fn default() -> Self {
        Self {
            start: 0.0,
            split: 0.0,
            total: 0.0,
            _marker: PhantomData,
        }
    }
}

impl<T: TimeSource> Timer<T> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) {
        self.start = T::get_time();
    }

    pub fn stop(&mut self) {
        self.split(T::get_time());
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn lap(&mut self) {
        let now = T::get_time();
        self.split(now);
        self.start = now;
    }

    #[must_use]
    pub const fn elapsed(&self) -> f64 {
        self.split
    }

    #[must_use]
    pub const fn total(&self) -> f64 {
        self.total
    }

    fn split(&mut self, now: f64) {
        self.split = T::diff_time(now, self.start);
        self.total += self.split;
    }
}

#[must_use]
pub(crate) fn to_seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

fn diff_time_checked<T: TimeSource>(t_start: f64, t_end: Option<f64>) -> f64 {
    if !is_valid_time(t_start) {
        return t_start;
    }
    let end = t_end.unwrap_or_else(T::get_time);
    if is_valid_time(end) {
        diff_time_unchecked(end, t_start)
    } else {
        end
    }
}
