//! Rust port of original_clasp/libpotassco/potassco/application.h and
//! original_clasp/libpotassco/src/application.cpp.

use std::any::Any;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use crate::potassco::error::{Error as PotasscoError, RuntimeError};
use crate::potassco::format::{Color, Emphasis, TextStyle, TextStyleSpec};
use crate::potassco::platform::{self, AlarmFunc};
use crate::potassco::program_opts::{
    DefaultFormatElement, DefaultParseContext, DescriptionLevel, Error as ProgramOptionsError,
    Option as Opt, OptionContext, OptionFormatter, OptionGroup, ParsedOptions, flag, parse,
    parse_command_array,
};

const EXIT_SUCCESS_CODE: i32 = 0;
const EXIT_FAILURE_CODE: i32 = 1;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpOpt {
    pub desc: String,
    pub max: u32,
}

impl HelpOpt {
    #[must_use]
    pub fn new(desc: impl Into<String>, max: u32) -> Self {
        Self {
            desc: desc.into(),
            max,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerboseOpt {
    pub default: String,
    pub max: u32,
}

impl VerboseOpt {
    #[must_use]
    pub fn new(default: impl Into<String>, max: u32) -> Self {
        Self {
            default: default.into(),
            max,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MessageType {
    Error,
    Warning,
    Info,
}

#[derive(Default)]
pub struct ApplicationBase {
    exit_code: AtomicI32,
    timeout: AtomicU32,
    verbose: AtomicU32,
    running: AtomicBool,
    fast_exit: AtomicBool,
    blocked: AtomicI32,
    pending: AtomicI32,
    color_msg: AtomicBool,
    color_help: AtomicBool,
}

impl ApplicationBase {
    #[must_use]
    pub fn new() -> Self {
        Self {
            exit_code: AtomicI32::new(EXIT_FAILURE_CODE),
            timeout: AtomicU32::new(0),
            verbose: AtomicU32::new(0),
            running: AtomicBool::new(false),
            fast_exit: AtomicBool::new(false),
            blocked: AtomicI32::new(0),
            pending: AtomicI32::new(0),
            color_msg: AtomicBool::new(false),
            color_help: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub fn col_error() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Red),
            background: None,
        })
    }

    #[must_use]
    pub fn col_warning() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::BrightYellow),
            background: None,
        })
    }

    #[must_use]
    pub fn col_info() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Cyan),
            background: None,
        })
    }

    #[must_use]
    pub fn col_em() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: None,
            background: None,
        })
    }

    #[must_use]
    pub fn col_usage() -> TextStyle {
        Self::col_em()
    }

    #[must_use]
    pub fn col_program() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::BrightYellow),
            background: None,
        })
    }

    #[must_use]
    pub fn col_def_cmd() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::None,
            foreground: Some(Color::BrightMagenta),
            background: None,
        })
    }

    #[must_use]
    pub fn col_opt_group() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::BrightBlue),
            background: None,
        })
    }

    #[must_use]
    pub fn col_opt_short() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Green),
            background: None,
        })
    }

    #[must_use]
    pub fn col_opt_long() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Cyan),
            background: None,
        })
    }

    #[must_use]
    pub fn col_opt_arg() -> TextStyle {
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Yellow),
            background: None,
        })
    }

    fn reset_for_run(&self) {
        self.exit_code.store(EXIT_FAILURE_CODE, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
        self.blocked.store(0, Ordering::SeqCst);
        self.pending.store(0, Ordering::SeqCst);
        self.fast_exit.store(false, Ordering::SeqCst);
    }
}

#[derive(Copy, Clone)]
struct CurrentApp {
    app: *mut dyn Application,
    object: *mut (),
    state: *const ApplicationBase,
    process_signal: unsafe fn(*mut (), i32),
    flush: unsafe fn(*mut ()),
}

unsafe impl Send for CurrentApp {}

fn current_app_slot() -> &'static Mutex<Option<CurrentApp>> {
    static SLOT: Mutex<Option<CurrentApp>> = Mutex::new(None);
    &SLOT
}

unsafe fn process_signal_for<T: Application>(object: *mut (), sig: i32) {
    let app = unsafe { &*(object as *const T) };
    app.process_signal(sig);
}

unsafe fn flush_for<T: Application>(object: *mut ()) {
    let app = unsafe { &mut *(object as *mut T) };
    app.flush();
}

fn init_instance<T: Application>(app: &mut T) {
    let app_ptr = app as *mut T as *mut dyn Application;
    let app_ptr: *mut dyn Application = unsafe { std::mem::transmute(app_ptr) };
    let mut slot = current_app_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(CurrentApp {
        app: app_ptr,
        object: app as *mut T as *mut (),
        state: app.base() as *const ApplicationBase,
        process_signal: process_signal_for::<T>,
        flush: flush_for::<T>,
    });
}

fn reset_instance(app: &ApplicationBase) {
    let mut slot = current_app_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot
        .as_ref()
        .is_some_and(|current| std::ptr::eq(current.state, app as *const ApplicationBase))
    {
        *slot = None;
    }
}

fn alarm_handler(sig: i32) {
    let slot = current_app_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(current) = slot.as_ref() {
        unsafe { (current.process_signal)(current.object, sig) };
    }
}

fn flush_instance(app: &ApplicationBase) {
    let current = {
        let slot = current_app_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        slot.as_ref().copied()
    };
    if let Some(current) = current {
        if std::ptr::eq(current.state, app as *const ApplicationBase) {
            unsafe { (current.flush)(current.object) };
        }
    }
}

#[derive(Debug)]
enum AppPanic {
    Stop,
    Failure(String),
}

pub struct Prefix<'a, A: Application> {
    app: &'a A,
    msg: String,
    level: MessageType,
    exception: bool,
}

impl<A: Application> fmt::Display for Prefix<'_, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            &self
                .app
                .render_prefix(self.level, &self.msg, self.exception),
        )
    }
}

pub trait Application {
    fn base(&self) -> &ApplicationBase;

    fn get_name(&self) -> &str;
    fn get_version(&self) -> &str;

    fn get_signals(&self) -> &[i32] {
        &[]
    }

    fn get_usage(&self) -> &str {
        "[options]"
    }

    fn get_help_option(&self) -> HelpOpt {
        HelpOpt::new("Print help information and exit", 1)
    }

    fn get_verbose_option(&self) -> VerboseOpt {
        VerboseOpt::new("", u32::MAX)
    }

    fn get_positional(&self, _value: &str) -> &str {
        ""
    }

    fn init_options<'a>(&mut self, root: &mut OptionContext<'a>);
    fn validate_options<'a>(
        &mut self,
        root: &OptionContext<'a>,
        parsed: &ParsedOptions,
    ) -> Result<(), ProgramOptionsError>;
    fn on_help(&mut self, help: &str, level: DescriptionLevel);
    fn on_version(&mut self, version: &str);
    fn setup(&mut self);
    fn run(&mut self);
    fn shutdown(&mut self) {}
    fn on_unhandled_exception(&mut self, msg: &str) -> bool;
    fn on_signal(&self, sig: i32) -> bool {
        self.set_exit_code(128 + sig);
        self.base().fast_exit.store(true, Ordering::SeqCst);
        true
    }
    fn flush(&mut self);

    fn fast_exit(&self, exit_code: i32) -> ! {
        flush_instance(self.base());
        std::process::exit(exit_code);
    }

    fn main(&mut self, args: &[&str]) -> i32
    where
        Self: Sized,
    {
        init_instance::<Self>(self);
        self.base().reset_for_run();
        self.base().running.store(true, Ordering::SeqCst);

        let mut should_shutdown = false;
        let mut error: Option<Box<dyn Any + Send>> = None;

        let run_result = catch_unwind(AssertUnwindSafe(|| {
            if self.apply_options(args) {
                should_shutdown = true;
                let timeout_secs = self.base().timeout.load(Ordering::SeqCst);
                if timeout_secs != 0 {
                    match self.set_alarm(timeout_secs) {
                        Ok(()) => {}
                        Err(platform_error) => {
                            self.fail(
                                EXIT_FAILURE_CODE,
                                "Option '--time-limit': could not apply limit",
                                &platform_error,
                            );
                        }
                    }
                }
                self.set_exit_code(EXIT_SUCCESS_CODE);
                self.setup();
                self.run();
            }
        }));

        if let Err(payload) = run_result {
            error = Some(payload);
        }

        if should_shutdown {
            let shutdown_result = catch_unwind(AssertUnwindSafe(|| {
                self.block_signals();
                self.kill_alarm();
                self.shutdown();
            }));
            if let Err(payload) = shutdown_result {
                if error.is_none() {
                    error = Some(payload);
                }
            }
        }

        if let Some(payload) = error {
            self.handle_exception(payload);
        }

        if self.base().fast_exit.load(Ordering::SeqCst) {
            self.fast_exit(self.get_exit_code());
        }

        self.flush();
        self.base().running.store(false, Ordering::SeqCst);
        reset_instance(self.base());
        self.get_exit_code()
    }

    fn main_from_argv(&mut self, argc: i32, argv: Option<&[&str]>) -> i32
    where
        Self: Sized,
    {
        assert!(argc >= 0, "invalid arg count");
        assert!(argc == 0 || argv.is_some(), "invalid arg vector");
        let argc = argc as usize;
        let args = argv.unwrap_or(&[]);
        let args = if argc == 0 { &[][..] } else { &args[1..argc] };
        self.main(args)
    }

    fn set_exit_code(&self, value: i32) {
        self.base().exit_code.store(value, Ordering::SeqCst);
    }

    fn get_exit_code(&self) -> i32 {
        self.base().exit_code.load(Ordering::SeqCst)
    }

    fn get_verbose(&self) -> u32 {
        self.base().verbose.load(Ordering::SeqCst)
    }

    fn get_time_limit(&self) -> u32 {
        self.base().timeout.load(Ordering::SeqCst)
    }

    fn set_verbose(&self, value: u32) {
        self.base().verbose.store(value, Ordering::SeqCst);
    }

    fn enable_colored_messages(&self, enable: bool) {
        self.base().color_msg.store(enable, Ordering::SeqCst);
    }

    fn enable_colored_help(&self, enable: bool) {
        self.base().color_help.store(enable, Ordering::SeqCst);
    }

    fn has_colored_messages(&self) -> bool {
        self.base().color_msg.load(Ordering::SeqCst)
    }

    fn has_colored_help(&self) -> bool {
        self.base().color_help.load(Ordering::SeqCst)
    }

    fn message(
        &self,
        level: MessageType,
        msg: impl Into<String>,
        exception: bool,
    ) -> Prefix<'_, Self>
    where
        Self: Sized,
    {
        Prefix {
            app: self,
            msg: msg.into(),
            level,
            exception,
        }
    }

    fn error(&self, msg: impl Into<String>) -> Prefix<'_, Self>
    where
        Self: Sized,
    {
        self.message(MessageType::Error, msg, false)
    }

    fn warn(&self, msg: impl Into<String>) -> Prefix<'_, Self>
    where
        Self: Sized,
    {
        self.message(MessageType::Warning, msg, false)
    }

    fn info(&self, msg: impl Into<String>) -> Prefix<'_, Self>
    where
        Self: Sized,
    {
        self.message(MessageType::Info, msg, false)
    }

    fn fail(&self, code: i32, message: &str, info: &str) {
        if self.base().running.load(Ordering::SeqCst) {
            self.set_exit_code(code);
            let text = if info.is_empty() {
                message.to_owned()
            } else {
                format!("{message}\n{info}")
            };
            panic_any(AppPanic::Failure(text));
        }
    }

    fn stop(&self, code: i32) {
        if self.base().running.load(Ordering::SeqCst) {
            self.set_exit_code(code);
            panic_any(AppPanic::Stop);
        }
    }

    fn set_alarm_ms(&self, millis: u32) -> Result<(), String> {
        if millis != 0 {
            platform::set_alarm(millis, alarm_handler as AlarmFunc)
                .map_err(platform_error_message)?;
        }
        self.base().timeout.store(millis, Ordering::SeqCst);
        Ok(())
    }

    fn set_alarm(&self, sec: u32) -> Result<(), String> {
        self.set_alarm_ms(sec.saturating_mul(1000))
    }

    fn kill_alarm(&self) {
        if self.base().timeout.swap(0, Ordering::SeqCst) != 0 {
            let _ = platform::kill_alarm();
        }
    }

    fn block_signals(&self) -> i32 {
        self.base().blocked.fetch_add(1, Ordering::SeqCst)
    }

    fn unblock_signals(&self, deliver_pending: bool) {
        if self.base().blocked.fetch_sub(1, Ordering::SeqCst) == 1 {
            let pending = self.base().pending.swap(0, Ordering::SeqCst);
            if pending != 0 && deliver_pending {
                self.process_signal(pending);
            }
        }
    }

    fn process_signal(&self, sig_num: i32) {
        if self.block_signals() == 0 {
            let fast = self.base().fast_exit.swap(true, Ordering::SeqCst);
            let result = catch_unwind(AssertUnwindSafe(|| self.on_signal(sig_num)));
            match result {
                Ok(keep_running) => {
                    if !keep_running {
                        return;
                    }
                    self.base().fast_exit.store(fast, Ordering::SeqCst);
                }
                Err(_) => {
                    self.set_exit_code(EXIT_FAILURE_CODE);
                    self.base().fast_exit.store(true, Ordering::SeqCst);
                }
            }
        } else if self.base().pending.load(Ordering::SeqCst) == 0 {
            self.base().pending.store(sig_num, Ordering::SeqCst);
        }
        self.base().blocked.fetch_sub(1, Ordering::SeqCst);
    }

    fn render_prefix(&self, level: MessageType, msg: &str, exception: bool) -> String {
        let mut out = String::new();
        let mut remaining = msg;
        let mut sep = "";
        let level_style = if self.has_colored_messages() {
            self.level_style(level)
        } else {
            TextStyle::default()
        };
        let message_style = if self.has_colored_messages() && !msg.is_empty() {
            ApplicationBase::col_em()
        } else {
            TextStyle::default()
        };

        loop {
            let line = if exception {
                let end = remaining.find('\n').unwrap_or(remaining.len());
                &remaining[..end]
            } else {
                remaining
            };
            out.push_str(sep);
            let mut line_prefix = String::new();
            line_prefix.push_str(prefix(level));
            line_prefix.push('(');
            line_prefix.push_str(self.get_name());
            line_prefix.push_str("): ");
            append_styled(&mut out, &level_style, &line_prefix);
            self.colorize(&mut out, line, &message_style, exception);
            if !exception || remaining.len() == line.len() {
                break;
            }
            remaining = &remaining[line.len() + 1..];
            sep = "\n";
            if remaining.is_empty() {
                break;
            }
        }
        out
    }

    fn handle_exception(&mut self, payload: Box<dyn Any + Send>) {
        let mut code = EXIT_FAILURE_CODE;
        let mut error = String::new();
        let mut info = String::new();

        match payload.downcast::<AppPanic>() {
            Ok(app_panic) => match *app_panic {
                AppPanic::Stop => {
                    code = EXIT_SUCCESS_CODE;
                }
                AppPanic::Failure(text) => split_message(&text, &mut error, &mut info),
            },
            Err(payload) => match payload.downcast::<ProgramOptionsError>() {
                Ok(err) => {
                    error = err.to_string();
                    info = "Try '--help' for usage information".to_owned();
                }
                Err(payload) => match payload.downcast::<PotasscoError>() {
                    Ok(err) => split_message(&err.to_string(), &mut error, &mut info),
                    Err(payload) => match payload.downcast::<RuntimeError>() {
                        Ok(err) => {
                            error = err.message().to_owned();
                            info = err.details().to_owned();
                        }
                        Err(payload) => match payload.downcast::<String>() {
                            Ok(err) => split_message(&err, &mut error, &mut info),
                            Err(payload) => match payload.downcast::<&'static str>() {
                                Ok(err) => split_message(&err, &mut error, &mut info),
                                Err(payload) => {
                                    self.base().fast_exit.store(true, Ordering::SeqCst);
                                    let _ = payload;
                                    error = "Unknown exception".to_owned();
                                }
                            },
                        },
                    },
                },
            },
        }

        if self.get_exit_code() == EXIT_SUCCESS_CODE {
            self.set_exit_code(code);
        }
        let force_exit = code != EXIT_SUCCESS_CODE && self.unhandled_exception(&error, &info);
        if force_exit {
            self.base().fast_exit.store(true, Ordering::SeqCst);
        }
    }

    fn unhandled_exception(&mut self, error: &str, info: &str) -> bool {
        let mut buffer = self.render_prefix(MessageType::Error, error, true);
        if !info.is_empty() {
            buffer.push('\n');
            buffer.push_str(&self.render_prefix(MessageType::Info, info, true));
        }
        self.on_unhandled_exception(&buffer)
    }

    fn level_style(&self, level: MessageType) -> TextStyle {
        match level {
            MessageType::Error => ApplicationBase::col_error(),
            MessageType::Warning => ApplicationBase::col_warning(),
            MessageType::Info => ApplicationBase::col_info(),
        }
    }

    fn colorize(&self, sink: &mut String, msg: &str, color: &TextStyle, exception: bool) {
        if msg.is_empty() || !exception || color.view().is_empty() {
            append_styled(sink, color, msg);
            return;
        }
        const PRE_KEY: &str = "Precondition ";
        const CHECK_KEY: &str = "check ";
        const POST_KEY: &str = "' failed.";

        let mut remaining = msg;
        while !remaining.is_empty() {
            let Some(start) = remaining.find('\'') else {
                sink.push_str(remaining);
                break;
            };
            let Some(end) = remaining[start + 1..]
                .find('\'')
                .map(|value| value + start + 1)
            else {
                sink.push_str(remaining);
                break;
            };
            let exp = expression_end(remaining, start, PRE_KEY, POST_KEY)
                .or_else(|| expression_end(remaining, start, CHECK_KEY, POST_KEY));
            if let Some(expr_end) = exp {
                if remaining[..start].ends_with(PRE_KEY) {
                    let prefix_len = start - PRE_KEY.len();
                    sink.push_str(&remaining[..prefix_len]);
                    append_styled(sink, &ApplicationBase::col_warning(), PRE_KEY);
                    sink.push('\'');
                    append_styled(sink, color, &remaining[start + 1..expr_end]);
                    sink.push_str("' ");
                    append_styled(sink, &ApplicationBase::col_warning(), &POST_KEY[2..]);
                    remaining = &remaining[expr_end + POST_KEY.len()..];
                } else {
                    sink.push_str(&remaining[..start + 1]);
                    append_styled(sink, color, &remaining[start + 1..expr_end]);
                    sink.push('\'');
                    remaining = &remaining[expr_end + 1..];
                }
            } else {
                sink.push_str(&remaining[..start + 1]);
                append_styled(sink, color, &remaining[start + 1..end]);
                sink.push('\'');
                remaining = &remaining[end + 1..];
            }
        }
    }

    fn apply_options(&mut self, args: &[&str]) -> bool {
        enum Action {
            Continue,
            Help(String, DescriptionLevel),
            Version(String),
        }

        let mut help = 0u32;
        let mut version = false;
        let mut verbose = 0u32;
        let mut timeout = 0u32;
        let mut fast_exit = false;
        let action = {
            let mut all_opts =
                OptionContext::new(format!("<{}>", self.get_name()), DescriptionLevel::Default);
            let help_ptr = &mut help as *mut u32;
            let version_ptr = &mut version as *mut bool;
            let verbose_ptr = &mut verbose as *mut u32;
            let timeout_ptr = &mut timeout as *mut u32;
            let fast_exit_ptr = &mut fast_exit as *mut bool;
            let mut basic = OptionGroup::new("Basic Options", DescriptionLevel::Default);
            {
                let mut init = basic.add_options();
                let help_opt = self.get_help_option();
                if help_opt.max > 0 {
                    let value = if help_opt.max == 1 {
                        flag(move |enabled| unsafe {
                            *help_ptr = u32::from(enabled);
                        })
                    } else {
                        parse(move |input: &str| {
                            input.parse::<u32>().is_ok_and(|value| {
                                if value == 0 || value > help_opt.max {
                                    return false;
                                }
                                unsafe {
                                    *help_ptr = value;
                                }
                                true
                            })
                        })
                        .arg("<n>")
                        .implicit("1")
                    };
                    init.add("-h,help", value, help_opt.desc.clone()).unwrap();
                }

                let verbose_opt = self.get_verbose_option();
                if verbose_opt.max > 0 {
                    let max = verbose_opt.max;
                    let mut value = parse(move |input: &str| {
                        if input == "umax" {
                            unsafe {
                                *verbose_ptr = max;
                            }
                            true
                        } else {
                            input.parse::<u32>().is_ok_and(|value| {
                                if value > max {
                                    return false;
                                }
                                unsafe {
                                    *verbose_ptr = value;
                                }
                                true
                            })
                        }
                    })
                    .arg("<n>")
                    .implicit("umax");
                    if !verbose_opt.default.is_empty() {
                        value = value.defaults_to(verbose_opt.default.clone());
                    }
                    init.add("-V,verbose", value, "Set verbosity level to %A")
                        .unwrap();
                }

                init.add(
                    "-v,version",
                    flag(move |enabled| unsafe {
                        *version_ptr = enabled;
                    }),
                    "Print version information and exit",
                )
                .unwrap();
                init.add(
                    "time-limit",
                    parse(move |input: &str| {
                        input.parse::<u32>().is_ok_and(|value| {
                            unsafe {
                                *timeout_ptr = value;
                            }
                            true
                        })
                    })
                    .arg("<n>"),
                    "Set time limit to %A seconds (0=no limit)",
                )
                .unwrap();
                init.add(
                    "@1,fast-exit",
                    flag(move |enabled| unsafe {
                        *fast_exit_ptr = enabled;
                    }),
                    "Force fast exit (do not call dtors)",
                )
                .unwrap();
            }
            all_opts.add(basic).unwrap();
            self.init_options(&mut all_opts);

            let parsed = {
                let self_ptr = self as *const Self;
                let mut positional = move |value: &str, opt: &mut String| {
                    let name = unsafe { (*self_ptr).get_positional(value) };
                    if name.is_empty() {
                        false
                    } else {
                        opt.clear();
                        opt.push_str(name);
                        true
                    }
                };
                let mut parse_context = DefaultParseContext::new(&all_opts);
                if let Err(err) =
                    parse_command_array(&mut parse_context, args, Some(&mut positional), 0)
                {
                    panic_any(err);
                }
                parse_context.parsed().clone()
            };
            if let Err(err) = all_opts.assign_defaults(&parsed) {
                panic_any(err);
            }

            if help != 0 || version {
                let mut msg = String::new();
                msg.push_str(self.get_name());
                msg.push_str(" version ");
                msg.push_str(self.get_version());
                msg.push('\n');
                if help != 0 {
                    let level = match help.saturating_sub(1) {
                        0 => DescriptionLevel::Default,
                        1 => DescriptionLevel::E1,
                        2 => DescriptionLevel::E2,
                        3 => DescriptionLevel::E3,
                        _ => DescriptionLevel::All,
                    };
                    let formatter = HelpFormatter::new(self.has_colored_help());
                    formatter.format_usage(&mut msg, self.get_name(), self.get_usage(), "");
                    all_opts.set_active_desc_level(level);
                    msg.push_str(&all_opts.format_description(&formatter));
                    msg.push('\n');
                    let formatter = HelpFormatter::new(self.has_colored_help());
                    let defaults = all_opts.defaults(self.get_name().len() + 1);
                    formatter.format_usage(&mut msg, self.get_name(), self.get_usage(), &defaults);
                    Action::Help(msg, level)
                } else {
                    msg.push_str("Address model: ");
                    msg.push_str(&(std::mem::size_of::<usize>() * 8).to_string());
                    msg.push_str("-bit");
                    Action::Version(msg)
                }
            } else {
                if let Err(err) = self.validate_options(&all_opts, &parsed) {
                    panic_any(err);
                }
                Action::Continue
            }
        };

        self.set_verbose(verbose);
        self.base().timeout.store(timeout, Ordering::SeqCst);
        self.base().fast_exit.store(fast_exit, Ordering::SeqCst);

        match action {
            Action::Continue => true,
            Action::Help(msg, level) => {
                self.set_exit_code(EXIT_SUCCESS_CODE);
                self.on_help(&msg, level);
                false
            }
            Action::Version(msg) => {
                self.set_exit_code(EXIT_SUCCESS_CODE);
                self.on_version(&msg);
                false
            }
        }
    }
}

impl dyn Application {
    #[must_use]
    pub fn get_instance() -> Option<*mut dyn Application> {
        current_app_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|current| current.app)
    }
}

struct HelpFormatter {
    colored: bool,
}

impl HelpFormatter {
    fn new(colored: bool) -> Self {
        Self { colored }
    }

    fn style(&self, elem: DefaultFormatElement) -> TextStyle {
        if !self.colored {
            return TextStyle::default();
        }
        match elem {
            DefaultFormatElement::Caption => ApplicationBase::col_opt_group(),
            DefaultFormatElement::Alias => ApplicationBase::col_opt_short(),
            DefaultFormatElement::Name => ApplicationBase::col_opt_long(),
            DefaultFormatElement::Arg => ApplicationBase::col_opt_arg(),
            DefaultFormatElement::Description => TextStyle::default(),
        }
    }

    fn format_usage(&self, buffer: &mut String, program: &str, usage: &str, defaults: &str) {
        if self.colored {
            append_styled(buffer, &ApplicationBase::col_usage(), "usage:");
            buffer.push(' ');
            append_styled(buffer, &ApplicationBase::col_program(), program);
            buffer.push(' ');
            buffer.push_str(usage);
            buffer.push('\n');
            if !defaults.is_empty() {
                append_styled(
                    buffer,
                    &ApplicationBase::col_usage(),
                    "Default command-line:\n",
                );
                append_styled(buffer, &ApplicationBase::col_program(), program);
                buffer.push(' ');
                append_styled(buffer, &ApplicationBase::col_def_cmd(), defaults);
            }
        } else {
            buffer.push_str("usage: ");
            buffer.push_str(program);
            buffer.push(' ');
            buffer.push_str(usage);
            buffer.push('\n');
            if !defaults.is_empty() {
                buffer.push_str("Default command-line:\n");
                buffer.push_str(program);
                buffer.push(' ');
                buffer.push_str(defaults);
            }
        }
    }
}

impl OptionFormatter for HelpFormatter {
    fn format_context<'a, 'b>(
        &self,
        buffer: &'b mut String,
        _ctx: &OptionContext<'a>,
    ) -> &'b mut String {
        buffer
    }

    fn format_group<'a, 'b>(
        &self,
        buffer: &'b mut String,
        group: &OptionGroup<'a>,
    ) -> &'b mut String {
        if !group.caption().is_empty() {
            buffer.push('\n');
            append_styled(
                buffer,
                &self.style(DefaultFormatElement::Caption),
                group.caption(),
            );
            buffer.push_str(":\n\n");
        }
        buffer
    }

    fn format_option<'a, 'b>(
        &self,
        buffer: &'b mut String,
        option: &Opt<'a>,
        max_width: usize,
    ) -> &'b mut String {
        let width = self.column_width(option);
        let arg = option.arg_name();
        let neg_name = if arg.is_empty() && option.negatable() {
            "[no-]"
        } else {
            ""
        };
        buffer.push_str("  ");
        if option.alias() != '\0' {
            let alias = format!("-{}", option.alias());
            append_styled(buffer, &self.style(DefaultFormatElement::Alias), &alias);
            buffer.push(',');
        }
        let mut long_name = String::from("--");
        long_name.push_str(neg_name);
        long_name.push_str(option.name());
        append_styled(buffer, &self.style(DefaultFormatElement::Name), &long_name);
        if !arg.is_empty() {
            if option.implicit() {
                buffer.push_str("[=");
            } else if option.alias() != '\0' {
                buffer.push(' ');
            } else {
                buffer.push('=');
            }
            let mut arg_text = arg.to_owned();
            if option.negatable() {
                arg_text.push_str("|no");
            }
            append_styled(buffer, &self.style(DefaultFormatElement::Arg), &arg_text);
            if option.implicit() {
                buffer.push(']');
            }
        }
        if width < max_width {
            buffer.push_str(&" ".repeat(max_width - width));
        }
        if !option.description().is_empty() {
            buffer.push_str(": ");
            option.format_description(buffer);
        }
        buffer.push('\n');
        buffer
    }

    fn column_width<'a>(&self, option: &Opt<'a>) -> usize {
        let mut width = 2usize;
        if option.alias() != '\0' {
            width += 3;
        }
        width += option.name().len() + 2;
        let arg = option.arg_name();
        if !arg.is_empty() {
            width += arg.len() + 1;
            if option.implicit() {
                width += 2;
            }
            if option.negatable() {
                width += 3;
            }
        } else if option.negatable() {
            width += 5;
        }
        width
    }
}

fn prefix(level: MessageType) -> &'static str {
    match level {
        MessageType::Error => "*** ERROR: ",
        MessageType::Warning => "*** Warn : ",
        MessageType::Info => "*** Info : ",
    }
}

fn append_styled(buffer: &mut String, style: &TextStyle, text: &str) {
    if style.view().is_empty() {
        buffer.push_str(text);
    } else {
        buffer.push_str(style.view());
        buffer.push_str(text);
        buffer.push_str(style.reset_view());
    }
}

fn split_message(input: &str, error: &mut String, info: &mut String) {
    if let Some((head, tail)) = input.split_once('\n') {
        error.clear();
        error.push_str(head);
        info.clear();
        info.push_str(tail);
    } else {
        error.clear();
        error.push_str(input);
        info.clear();
    }
}

fn expression_end(input: &str, pos: usize, key: &str, post_key: &str) -> Option<usize> {
    if pos >= key.len() && input[pos - key.len()..].starts_with(key) {
        input[pos + 1..].find(post_key).map(|value| value + pos + 1)
    } else {
        None
    }
}

fn platform_error_message(error: platform::PlatformError) -> String {
    match error {
        platform::PlatformError::OperationNotSupported => {
            "Operation not supported on this platform".to_owned()
        }
        _ => error.to_string(),
    }
}
