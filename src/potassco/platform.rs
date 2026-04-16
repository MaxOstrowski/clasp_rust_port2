//! Rust port of the timing helpers required from
//! original_clasp/libpotassco/potassco/platform.h and
//! original_clasp/libpotassco/src/platform.cpp.

#[cfg(all(unix, not(target_os = "emscripten")))]
mod unix {
    use core::ffi::{c_int, c_long};

    #[repr(C)]
    struct Timespec {
        tv_sec: c_long,
        tv_nsec: c_long,
    }

    unsafe extern "C" {
        fn clock_gettime(clock_id: c_int, tp: *mut Timespec) -> c_int;
    }

    const CLOCK_PROCESS_CPUTIME_ID: c_int = 2;
    const CLOCK_THREAD_CPUTIME_ID: c_int = 3;

    pub(super) fn get_process_time() -> f64 {
        cpu_time_from_clock(CLOCK_PROCESS_CPUTIME_ID).unwrap_or(0.0)
    }

    pub(super) fn get_thread_time() -> f64 {
        cpu_time_from_clock(CLOCK_THREAD_CPUTIME_ID).unwrap_or_else(get_process_time)
    }

    fn cpu_time_from_clock(clock_id: c_int) -> Option<f64> {
        let mut ts = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let rc = unsafe { clock_gettime(clock_id, &mut ts) };
        if rc == 0 {
            Some((ts.tv_sec as f64) + (ts.tv_nsec as f64 / 1_000_000_000.0))
        } else {
            None
        }
    }
}

#[cfg(target_os = "emscripten")]
#[must_use]
pub fn get_process_time() -> f64 {
    f64::NAN
}

#[cfg(all(unix, not(target_os = "emscripten")))]
#[must_use]
pub fn get_process_time() -> f64 {
    unix::get_process_time()
}

#[cfg(not(any(unix, target_os = "emscripten")))]
#[must_use]
pub fn get_process_time() -> f64 {
    0.0
}

#[cfg(target_os = "emscripten")]
#[must_use]
pub fn get_thread_time() -> f64 {
    get_process_time()
}

#[cfg(all(unix, not(target_os = "emscripten")))]
#[must_use]
pub fn get_thread_time() -> f64 {
    unix::get_thread_time()
}

#[cfg(not(any(unix, target_os = "emscripten")))]
#[must_use]
pub fn get_thread_time() -> f64 {
    0.0
}
