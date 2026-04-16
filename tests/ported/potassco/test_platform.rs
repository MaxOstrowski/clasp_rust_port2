use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rust_clasp::potassco::platform::{
    CFile, ExpressionInfo, PlatformError, enable_ansi_color_support, get_process_time,
    get_thread_time, init_fpu_precision, is_terminal, kill_alarm, lock_file, restore_fpu_precision,
    set_alarm,
};
use rust_clasp::{capture_expression, current_location};

unsafe extern "C" {
    fn fclose(file: *mut CFile) -> i32;
    fn tmpfile() -> *mut CFile;
}

fn make_temp_file() -> *mut CFile {
    let file = unsafe { tmpfile() };
    assert!(!file.is_null());
    file
}

fn alarm_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn current_location_captures_call_site() {
    let location = current_location!();
    assert!(
        location
            .file
            .ends_with("tests/ported/potassco/test_platform.rs")
    );
    assert!(location.line > 0);
    assert!(
        location
            .function
            .contains("current_location_captures_call_site")
    );
}

#[test]
fn capture_expression_records_expression_and_location() {
    let expression = capture_expression!(1 + 2 == 3);
    assert_eq!(expression.expression, "1 + 2 == 3");
    assert!(
        expression
            .location
            .file
            .ends_with("tests/ported/potassco/test_platform.rs")
    );
}

#[test]
fn relative_file_name_strips_shared_source_root() {
    let info = capture_expression!(true);
    let relative = ExpressionInfo::relative_file_name(&info.location);
    assert_eq!(relative, "tests/ported/potassco/test_platform.rs");
}

#[test]
fn process_and_thread_times_are_non_negative() {
    let process = get_process_time();
    let thread = get_thread_time();
    assert!(process.is_nan() || process >= 0.0);
    assert!(thread.is_nan() || thread >= 0.0);
}

#[test]
fn temp_files_are_not_terminals_and_color_enable_reports_ioctl_error() {
    let file = make_temp_file();
    assert!(!is_terminal(file));
    assert_eq!(
        enable_ansi_color_support(file),
        Err(PlatformError::InappropriateIoControlOperation)
    );
    lock_file(file);
    rust_clasp::potassco::platform::unlock_file(file);
    assert_eq!(unsafe { fclose(file) }, 0);
}

#[test]
fn invalid_alarm_arguments_are_rejected() {
    fn ignored(_: i32) {}
    assert_eq!(set_alarm(0, ignored), Err(PlatformError::InvalidArgument));
}

#[cfg(all(unix, not(target_os = "emscripten")))]
#[test]
fn alarm_fires_and_clears_pending_state() {
    let _guard = alarm_test_guard();
    static STOP: AtomicI32 = AtomicI32::new(0);

    fn on_alarm(signal_value: i32) {
        STOP.store(signal_value, Ordering::SeqCst);
    }

    STOP.store(0, Ordering::SeqCst);
    assert_eq!(set_alarm(50, on_alarm), Ok(()));

    let deadline = Instant::now() + Duration::from_secs(2);
    while STOP.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(STOP.load(Ordering::SeqCst), 14);
    assert!(!kill_alarm());
}

#[cfg(all(unix, not(target_os = "emscripten")))]
#[test]
fn kill_alarm_cancels_or_observes_completed_alarm() {
    let _guard = alarm_test_guard();
    static STOP: AtomicI32 = AtomicI32::new(0);

    fn on_alarm(signal_value: i32) {
        STOP.store(signal_value, Ordering::SeqCst);
    }

    STOP.store(0, Ordering::SeqCst);
    assert_eq!(set_alarm(100, on_alarm), Ok(()));
    thread::sleep(Duration::from_millis(10));
    if kill_alarm() {
        thread::sleep(Duration::from_millis(150));
        assert_eq!(STOP.load(Ordering::SeqCst), 0);
    } else {
        assert_eq!(STOP.load(Ordering::SeqCst), 14);
    }
    assert!(!kill_alarm());
}

#[cfg(not(all(unix, not(target_os = "emscripten"))))]
#[test]
fn alarm_reports_unsupported_platforms() {
    fn ignored(_: i32) {}
    assert_eq!(
        set_alarm(50, ignored),
        Err(PlatformError::OperationNotSupported)
    );
    assert!(!kill_alarm());
}

#[test]
fn fpu_precision_round_trip_is_a_smoke_test() {
    let previous = init_fpu_precision();
    restore_fpu_precision(previous);
}
