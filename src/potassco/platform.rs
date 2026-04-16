//! Rust port of original_clasp/libpotassco/potassco/platform.h and
//! original_clasp/libpotassco/src/platform.cpp.

use core::fmt;

const PLATFORM_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/", file!());

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub function: &'static str,
}

impl SourceLocation {
    pub const fn new(file: &'static str, line: u32, function: &'static str) -> Self {
        Self {
            file,
            line,
            function,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExpressionInfo {
    pub expression: &'static str,
    pub location: SourceLocation,
}

impl ExpressionInfo {
    #[must_use]
    pub fn relative_file_name(location: &SourceLocation) -> &'static str {
        let shared = location
            .file
            .bytes()
            .zip(PLATFORM_FILE.bytes())
            .take_while(|(lhs, rhs)| lhs == rhs)
            .count();
        &location.file[shared..]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformError {
    InvalidArgument,
    InappropriateIoControlOperation,
    FunctionNotSupported,
    OperationNotSupported,
    System(i32),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument => f.write_str("invalid argument"),
            Self::InappropriateIoControlOperation => {
                f.write_str("inappropriate io control operation")
            }
            Self::FunctionNotSupported => f.write_str("function not supported"),
            Self::OperationNotSupported => f.write_str("operation not supported"),
            Self::System(code) => write!(f, "os error {code}"),
        }
    }
}

impl std::error::Error for PlatformError {}

pub type AlarmFunc = fn(i32);

#[repr(C)]
pub struct CFile {
    _private: [u8; 0],
}

unsafe extern "C" {
    static mut stdin: *mut CFile;
    static mut stdout: *mut CFile;
    static mut stderr: *mut CFile;
}

#[must_use]
pub fn stdin_stream() -> *mut CFile {
    unsafe { stdin }
}

#[must_use]
pub fn stdout_stream() -> *mut CFile {
    unsafe { stdout }
}

#[must_use]
pub fn stderr_stream() -> *mut CFile {
    unsafe { stderr }
}

#[must_use]
pub fn init_fpu_precision() -> u32 {
    fpu::init_fpu_precision()
}

pub fn restore_fpu_precision(previous: u32) {
    fpu::restore_fpu_precision(previous);
}

pub fn enable_ansi_color_support(file: *mut CFile) -> Result<(), PlatformError> {
    terminal::enable_ansi_color_support(file)
}

#[must_use]
pub fn is_terminal(file: *mut CFile) -> bool {
    terminal::is_terminal(file)
}

pub fn lock_file(file: *mut CFile) {
    terminal::lock_file(file);
}

pub fn unlock_file(file: *mut CFile) {
    terminal::unlock_file(file);
}

pub fn set_alarm(millis: u32, handler: AlarmFunc) -> Result<(), PlatformError> {
    if millis == 0 {
        return Err(PlatformError::InvalidArgument);
    }
    let _ = kill_alarm();
    alarm::set_alarm(millis, handler)
}

#[must_use]
pub fn kill_alarm() -> bool {
    alarm::kill_alarm()
}

#[cfg(target_os = "emscripten")]
#[must_use]
pub fn get_process_time() -> f64 {
    f64::NAN
}

#[cfg(all(unix, not(target_os = "emscripten")))]
#[must_use]
pub fn get_process_time() -> f64 {
    unix_time::get_process_time()
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
    unix_time::get_thread_time()
}

#[cfg(not(any(unix, target_os = "emscripten")))]
#[must_use]
pub fn get_thread_time() -> f64 {
    0.0
}

#[doc(hidden)]
#[macro_export]
macro_rules! __potassco_function_name {
    () => {{
        fn f() {}
        let name = core::any::type_name_of_val(&f);
        name.strip_suffix("::f").unwrap_or(name)
    }};
}

#[macro_export]
macro_rules! current_location {
    () => {
        $crate::potassco::platform::SourceLocation::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/", file!()),
            line!(),
            $crate::__potassco_function_name!(),
        )
    };
}

#[macro_export]
macro_rules! capture_expression {
    ($expr:expr) => {
        $crate::potassco::platform::ExpressionInfo {
            expression: stringify!($expr),
            location: $crate::current_location!(),
        }
    };
}

#[cfg(all(unix, not(target_os = "emscripten")))]
mod unix_time {
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

mod terminal {
    use super::{CFile, PlatformError};

    #[cfg(all(unix, not(target_os = "emscripten")))]
    mod unix {
        use super::{CFile, PlatformError};
        use core::ffi::c_int;

        unsafe extern "C" {
            fn fileno(stream: *mut CFile) -> c_int;
            fn flockfile(stream: *mut CFile);
            fn funlockfile(stream: *mut CFile);
            fn isatty(fd: c_int) -> c_int;
        }

        pub(super) fn enable_ansi_color_support(file: *mut CFile) -> Result<(), PlatformError> {
            if is_terminal(file) {
                Ok(())
            } else {
                Err(PlatformError::InappropriateIoControlOperation)
            }
        }

        pub(super) fn is_terminal(file: *mut CFile) -> bool {
            if file.is_null() {
                return false;
            }
            unsafe { isatty(fileno(file)) > 0 }
        }

        pub(super) fn lock_file(file: *mut CFile) {
            if !file.is_null() {
                unsafe { flockfile(file) };
            }
        }

        pub(super) fn unlock_file(file: *mut CFile) {
            if !file.is_null() {
                unsafe { funlockfile(file) };
            }
        }
    }

    #[cfg(all(unix, not(target_os = "emscripten")))]
    pub(super) fn enable_ansi_color_support(file: *mut CFile) -> Result<(), PlatformError> {
        unix::enable_ansi_color_support(file)
    }

    #[cfg(not(any(unix, target_os = "emscripten")))]
    pub(super) fn enable_ansi_color_support(_file: *mut CFile) -> Result<(), PlatformError> {
        Err(PlatformError::FunctionNotSupported)
    }

    #[cfg(target_os = "emscripten")]
    pub(super) fn enable_ansi_color_support(_file: *mut CFile) -> Result<(), PlatformError> {
        Err(PlatformError::FunctionNotSupported)
    }

    #[cfg(all(unix, not(target_os = "emscripten")))]
    pub(super) fn is_terminal(file: *mut CFile) -> bool {
        unix::is_terminal(file)
    }

    #[cfg(not(any(unix, target_os = "emscripten")))]
    pub(super) fn is_terminal(_file: *mut CFile) -> bool {
        false
    }

    #[cfg(target_os = "emscripten")]
    pub(super) fn is_terminal(_file: *mut CFile) -> bool {
        false
    }

    #[cfg(all(unix, not(target_os = "emscripten")))]
    pub(super) fn lock_file(file: *mut CFile) {
        unix::lock_file(file);
    }

    #[cfg(not(any(unix, target_os = "emscripten")))]
    pub(super) fn lock_file(_file: *mut CFile) {}

    #[cfg(target_os = "emscripten")]
    pub(super) fn lock_file(_file: *mut CFile) {}

    #[cfg(all(unix, not(target_os = "emscripten")))]
    pub(super) fn unlock_file(file: *mut CFile) {
        unix::unlock_file(file);
    }

    #[cfg(not(any(unix, target_os = "emscripten")))]
    pub(super) fn unlock_file(_file: *mut CFile) {}

    #[cfg(target_os = "emscripten")]
    pub(super) fn unlock_file(_file: *mut CFile) {}
}

mod alarm {
    use super::{AlarmFunc, PlatformError};

    #[cfg(not(target_os = "emscripten"))]
    mod threaded {
        use super::{AlarmFunc, PlatformError};
        use std::sync::{Mutex, OnceLock};
        use std::thread;
        use std::time::Duration;

        const SIGALRM: i32 = 14;

        #[derive(Default)]
        struct AlarmState {
            generation: u64,
            active_generation: Option<u64>,
        }

        fn state() -> &'static Mutex<AlarmState> {
            static STATE: OnceLock<Mutex<AlarmState>> = OnceLock::new();
            STATE.get_or_init(|| Mutex::new(AlarmState::default()))
        }

        fn lock_state() -> std::sync::MutexGuard<'static, AlarmState> {
            state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        pub(super) fn set_alarm(millis: u32, handler: AlarmFunc) -> Result<(), PlatformError> {
            let generation = {
                let mut shared = lock_state();
                shared.generation += 1;
                shared.active_generation = Some(shared.generation);
                shared.generation
            };

            let _ = thread::Builder::new()
                .name("potassco-alarm".to_owned())
                .spawn(move || {
                    thread::sleep(Duration::from_millis(u64::from(millis)));
                    let should_fire = {
                        let mut shared = lock_state();
                        if shared.active_generation == Some(generation) {
                            shared.active_generation = None;
                            true
                        } else {
                            false
                        }
                    };
                    if should_fire {
                        handler(SIGALRM);
                    }
                })
                .map_err(|_| PlatformError::OperationNotSupported)?;
            Ok(())
        }

        pub(super) fn kill_alarm() -> bool {
            let mut shared = lock_state();
            let had_alarm = shared.active_generation.is_some();
            if had_alarm {
                shared.generation += 1;
                shared.active_generation = None;
            }
            had_alarm
        }
    }

    #[cfg(not(target_os = "emscripten"))]
    pub(super) fn set_alarm(millis: u32, handler: AlarmFunc) -> Result<(), PlatformError> {
        threaded::set_alarm(millis, handler)
    }

    #[cfg(not(target_os = "emscripten"))]
    pub(super) fn kill_alarm() -> bool {
        threaded::kill_alarm()
    }

    #[cfg(target_os = "emscripten")]
    pub(super) fn set_alarm(_millis: u32, _handler: AlarmFunc) -> Result<(), PlatformError> {
        Err(PlatformError::OperationNotSupported)
    }

    #[cfg(target_os = "emscripten")]
    pub(super) fn kill_alarm() -> bool {
        false
    }
}

mod fpu {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub(super) fn init_fpu_precision() -> u32 {
        let current = unsafe { get_control_word() };
        let next = (current & !0x0300) | 0x0200;
        if next != current {
            unsafe { set_control_word(next) };
        }
        u32::from(current)
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub(super) fn init_fpu_precision() -> u32 {
        0
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub(super) fn restore_fpu_precision(previous: u32) {
        unsafe { set_control_word(previous as u16) };
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub(super) fn restore_fpu_precision(_previous: u32) {}

    #[cfg(target_arch = "x86")]
    unsafe fn get_control_word() -> u16 {
        let mut control: u16 = 0;
        unsafe {
            core::arch::asm!("fnstcw [{0}]", in(reg) &mut control, options(nostack, preserves_flags))
        };
        control
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn get_control_word() -> u16 {
        let mut control: u16 = 0;
        unsafe {
            core::arch::asm!("fnstcw [{0}]", in(reg) &mut control, options(nostack, preserves_flags))
        };
        control
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe fn set_control_word(control: u16) {
        unsafe {
            core::arch::asm!("fldcw [{0}]", in(reg) &control, options(nostack, preserves_flags))
        };
    }
}
