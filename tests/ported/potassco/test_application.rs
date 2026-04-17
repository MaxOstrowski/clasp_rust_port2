//! Rust port of original_clasp/libpotassco/tests/test_application.cpp.

use std::collections::BTreeMap;
use std::panic::panic_any;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rust_clasp::capture_expression;
use rust_clasp::potassco::application::{Application, ApplicationBase, HelpOpt, VerboseOpt};
use rust_clasp::potassco::error::{Errc, fail_throw};
use rust_clasp::potassco::format::{
    Color, Emphasis, TextStyle, TextStyleParseError, TextStyleSpec,
};
use rust_clasp::potassco::platform;
use rust_clasp::potassco::program_opts::{
    DescriptionLevel, Error as ProgramOptionsError, OptionContext, OptionGroup, ParsedOptions,
    action_default, parse,
};

type RunHook = Box<dyn FnMut(&mut MyApp) -> i32 + Send>;

fn style(msg: &str, ts: TextStyle) -> String {
    format!("{}{}{}", ts.view(), msg, ts.reset_view())
}

fn alarm_test_guard() -> &'static Mutex<()> {
    static GUARD: Mutex<()> = Mutex::new(());
    &GUARD
}

fn alarm_stop() -> &'static AtomicI32 {
    static STOP: AtomicI32 = AtomicI32::new(0);
    &STOP
}

fn record_alarm(sig: i32) {
    alarm_stop().store(sig, Ordering::SeqCst);
}

struct MyApp {
    base: ApplicationBase,
    foo: i32,
    input: Vec<String>,
    do_run: Option<RunHook>,
    messages: BTreeMap<String, String>,
}

impl Default for MyApp {
    fn default() -> Self {
        let mut messages = BTreeMap::new();
        messages.insert("error".to_owned(), String::new());
        messages.insert("help".to_owned(), String::new());
        messages.insert("version".to_owned(), String::new());
        Self {
            base: ApplicationBase::new(),
            foo: 0,
            input: Vec::new(),
            do_run: None,
            messages,
        }
    }
}

impl Application for MyApp {
    fn base(&self) -> &ApplicationBase {
        &self.base
    }

    fn get_name(&self) -> &str {
        "TestApp"
    }

    fn get_version(&self) -> &str {
        "1.0"
    }

    fn get_usage(&self) -> &str {
        "[options] [files]"
    }

    fn get_help_option(&self) -> HelpOpt {
        HelpOpt::new("Print {1=basic|2=extended} help and exit", 2)
    }

    fn get_positional(&self, _value: &str) -> &str {
        "file"
    }

    fn init_options<'a>(&mut self, root: &mut OptionContext<'a>) {
        let foo_ptr = &mut self.foo as *mut i32;
        let input_ptr = &mut self.input as *mut Vec<String>;
        let mut basic = OptionGroup::new("Basic Options", DescriptionLevel::Default);
        basic
            .add_options()
            .add(
                "-x,foo",
                parse(move |input: &str| {
                    input.parse::<i32>().is_ok_and(|value| {
                        unsafe {
                            *foo_ptr = value;
                        }
                        true
                    })
                })
                .defaults_to("2"),
                "Some option with default [%D]",
            )
            .unwrap();
        root.add(basic).unwrap();

        let mut e1 = OptionGroup::new("E1 Options", DescriptionLevel::E1);
        e1.add_options()
            .add(
                "-f+,file",
                action_default(move |value: String| unsafe {
                    (*input_ptr).push(value);
                }),
                "Input files",
            )
            .unwrap();
        root.add(e1).unwrap();
    }

    fn validate_options<'a>(
        &mut self,
        _root: &OptionContext<'a>,
        _parsed: &ParsedOptions,
    ) -> Result<(), ProgramOptionsError> {
        Ok(())
    }

    fn on_help(&mut self, help: &str, _level: DescriptionLevel) {
        self.messages.insert("help".to_owned(), help.to_owned());
    }

    fn on_version(&mut self, version: &str) {
        self.messages
            .insert("version".to_owned(), version.to_owned());
    }

    fn setup(&mut self) {}

    fn run(&mut self) {
        if let Some(mut action) = self.do_run.take() {
            let code = action(self);
            self.set_exit_code(code);
            self.do_run = Some(action);
        } else {
            self.set_exit_code(0);
        }
    }

    fn on_unhandled_exception(&mut self, msg: &str) -> bool {
        self.messages.insert("error".to_owned(), msg.to_owned());
        false
    }

    fn flush(&mut self) {}
}

#[test]
fn application_formatting_and_styles_match_upstream_behavior() {
    let app = MyApp::default();
    assert!(TextStyle::default().view().is_empty());
    assert!(TextStyle::default().reset_view().is_empty());
    assert_eq!(
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::None,
            foreground: Some(Color::Black),
            background: None,
        })
        .view(),
        "\u{1b}[0;30m"
    );
    assert_eq!(
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Black),
            background: None,
        })
        .view(),
        "\u{1b}[1;30m"
    );
    assert_eq!(ApplicationBase::col_em().view(), "\u{1b}[1m");
    assert_eq!(ApplicationBase::col_warning().view(), "\u{1b}[1;93m");
    assert_eq!(ApplicationBase::col_error().view(), "\u{1b}[1;31m");
    assert_eq!(
        TextStyle::from_string("01;31", 0).unwrap().view(),
        "\u{1b}[1;31m"
    );
    assert_eq!(
        TextStyle::from_string("32", 0).unwrap().view(),
        "\u{1b}[0;32m"
    );
    assert_eq!(
        TextStyle::from_string("warning=1;35", 8).unwrap().view(),
        "\u{1b}[1;35m"
    );
    assert_eq!(TextStyle::from_string("", 0).unwrap().view(), "");
    assert!(matches!(
        TextStyle::from_string("7;31", 0),
        Err(TextStyleParseError::DomainError)
    ));

    let mut plain = String::new();
    plain.push_str(&format!("{}\n", app.error("An error")));
    plain.push_str(&format!("{}\n", app.warn("A warning")));
    plain.push_str(&format!("{}\n", app.info("Some info")));
    assert_eq!(
        plain,
        "*** ERROR: (TestApp): An error\n*** Warn : (TestApp): A warning\n*** Info : (TestApp): Some info\n"
    );

    app.enable_colored_messages(true);
    let colored = format!("{}\n{}\n", app.error("An error"), app.warn("A warning"));
    assert_eq!(
        colored,
        style("*** ERROR: (TestApp): ", ApplicationBase::col_error())
            + &style("An error", ApplicationBase::col_em())
            + "\n"
            + &style("*** Warn : (TestApp): ", ApplicationBase::col_warning())
            + &style("A warning", ApplicationBase::col_em())
            + "\n"
    );
}

#[test]
fn fail_and_stop_are_noops_when_not_running_and_raise_when_running() {
    let app = MyApp::default();
    app.fail(79, "Something is not right!", "Info line 1\nInfo line 2");
    assert_eq!(app.get_exit_code(), 1);
    app.stop(79);
    assert_eq!(app.get_exit_code(), 1);

    let mut fail_app = MyApp {
        do_run: Some(Box::new(|app| {
            app.fail(79, "Something is not right!", "Info line 1\nInfo line 2");
            0
        })),
        ..MyApp::default()
    };
    assert_eq!(fail_app.main(&[]), 79);
    assert_eq!(
        fail_app.messages["error"],
        "*** ERROR: (TestApp): Something is not right!\n*** Info : (TestApp): Info line 1\n*** Info : (TestApp): Info line 2"
    );

    let mut stop_app = MyApp {
        do_run: Some(Box::new(|app| {
            app.stop(12);
            0
        })),
        ..MyApp::default()
    };
    assert_eq!(stop_app.main(&[]), 12);
    assert_eq!(stop_app.messages["error"], "");
}

#[test]
fn other_errors_are_formatted_like_upstream_messages() {
    let mut app = MyApp::default();
    app.enable_colored_messages(true);
    app.do_run = Some(Box::new(|_| {
        fail_throw(
            Errc::OVERFLOW_ERROR,
            capture_expression!(true == false),
            Some("kaputt".to_owned()),
        );
    }));
    assert_eq!(app.main(&[]), 1);
    assert!(app.messages["error"].contains("kaputt: Value too large for defined data type"));
    assert!(app.messages["error"].contains("check '"));

    let mut pre = MyApp::default();
    pre.enable_colored_messages(true);
    pre.do_run = Some(Box::new(|_| {
        fail_throw(
            Errc::PRECONDITION_FAIL,
            capture_expression!('x' == 'y'),
            Some("kaputt".to_owned()),
        );
    }));
    assert_eq!(pre.main(&[]), 1);
    assert!(pre.messages["error"].contains("Precondition "));
    assert!(pre.messages["error"].contains("message: kaputt"));

    let mut other = MyApp {
        do_run: Some(Box::new(|_| {
            panic_any(String::from("line1\nline2"));
        })),
        ..MyApp::default()
    };
    assert_eq!(other.main(&[]), 1);
    assert_eq!(
        other.messages["error"],
        "*** ERROR: (TestApp): line1\n*** Info : (TestApp): line2"
    );
}

#[test]
fn help_version_and_argv_overload_work() {
    let mut app = MyApp::default();
    assert_eq!(app.main(&["-h", "-V3", "--vers", "hallo"]), 0);
    assert_eq!(app.get_verbose(), 3);
    assert_eq!(app.input[0], "hallo");
    let help = &app.messages["help"];
    assert!(help.starts_with("TestApp version 1.0\nusage: TestApp [options] [files]\n"));
    assert!(help.contains("Basic Options:"));
    assert!(
        help.contains("-V,--verbose[=<n>]: Set verbosity level to <n>")
            || help.contains("-V,--verbose[=<n>]   : Set verbosity level to <n>")
    );
    assert!(help.contains("--time-limit=<n>"));
    assert!(help.contains("Some option with default [2]"));
    assert!(help.contains("Default command-line:\nTestApp --foo=2"));

    let mut colored = MyApp::default();
    colored.enable_colored_help(true);
    assert_eq!(colored.main(&["-h"]), 0);
    let help = &colored.messages["help"];
    assert!(help.contains(&style("-V", ApplicationBase::col_opt_short())));
    assert!(help.contains(&style("--verbose", ApplicationBase::col_opt_long())));
    assert!(help.contains(&style("<n>", ApplicationBase::col_opt_arg())));

    let mut version = MyApp::default();
    assert_eq!(version.main(&["--version"]), 0);
    assert!(version.messages["version"].starts_with("TestApp version 1.0\nAddress model: "));

    let mut argv = MyApp::default();
    let argv_data = ["app", "--version"];
    assert_eq!(argv.main_from_argv(2, Some(&argv_data)), 0);
    assert!(argv.messages["version"].starts_with("TestApp version 1.0\nAddress model: "));
    assert_eq!(MyApp::default().main_from_argv(0, None), 0);
}

#[test]
fn invalid_help_argument_matches_upstream_messages() {
    let mut app = MyApp::default();
    assert_eq!(app.main(&["-h3"]), 1);
    assert_eq!(
        app.messages["error"],
        "*** ERROR: (TestApp): In context '<TestApp>': '3' invalid value for: 'help'\n*** Info : (TestApp): Try '--help' for usage information"
    );

    let mut colored = MyApp::default();
    colored.enable_colored_messages(true);
    assert_eq!(colored.main(&["-h3"]), 1);
    assert_eq!(
        colored.messages["error"],
        style("*** ERROR: (TestApp): ", ApplicationBase::col_error())
            + "In context '"
            + &style("<TestApp>", ApplicationBase::col_em())
            + "': '"
            + &style("3", ApplicationBase::col_em())
            + "' invalid value for: '"
            + &style("help", ApplicationBase::col_em())
            + "'\n"
            + &style("*** Info : (TestApp): ", ApplicationBase::col_info())
            + "Try '"
            + &style("--help", ApplicationBase::col_em())
            + "' for usage information"
    );
}

#[test]
fn platform_alarm_and_application_alarm_work() {
    let _guard = alarm_test_guard().lock().unwrap();
    let stop = alarm_stop();
    stop.store(0, Ordering::SeqCst);
    assert!(platform::set_alarm(100, record_alarm).is_ok());
    for _ in 0..40 {
        if stop.load(Ordering::SeqCst) != 0 {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(stop.load(Ordering::SeqCst), 14);
    assert!(!platform::kill_alarm());

    stop.store(0, Ordering::SeqCst);
    assert!(platform::set_alarm(100, record_alarm).is_ok());
    thread::sleep(Duration::from_millis(10));
    if platform::kill_alarm() {
        thread::sleep(Duration::from_millis(200));
        assert_eq!(stop.load(Ordering::SeqCst), 0);
    } else {
        assert_eq!(stop.load(Ordering::SeqCst), 14);
    }
    assert!(!platform::kill_alarm());

    struct TimedApp {
        base: ApplicationBase,
        stop: Arc<AtomicI32>,
    }

    impl Application for TimedApp {
        fn base(&self) -> &ApplicationBase {
            &self.base
        }
        fn get_name(&self) -> &str {
            "TimedApp"
        }
        fn get_version(&self) -> &str {
            "1.0"
        }
        fn get_verbose_option(&self) -> VerboseOpt {
            VerboseOpt::new("", 0)
        }
        fn init_options<'a>(&mut self, _root: &mut OptionContext<'a>) {}
        fn validate_options<'a>(
            &mut self,
            _root: &OptionContext<'a>,
            _parsed: &ParsedOptions,
        ) -> Result<(), ProgramOptionsError> {
            Ok(())
        }
        fn on_help(&mut self, _help: &str, _level: DescriptionLevel) {}
        fn on_version(&mut self, _version: &str) {}
        fn setup(&mut self) {}
        fn run(&mut self) {
            assert_eq!(self.get_time_limit(), 5_000);
            self.set_alarm_ms(100).unwrap();
            assert_eq!(self.get_time_limit(), 100);
            while self.stop.load(Ordering::SeqCst) == 0 {
                thread::sleep(Duration::from_millis(10));
            }
        }
        fn on_unhandled_exception(&mut self, _msg: &str) -> bool {
            false
        }
        fn on_signal(&self, sig: i32) -> bool {
            self.stop.store(sig, Ordering::SeqCst);
            true
        }
        fn flush(&mut self) {}
    }

    let stop = Arc::new(AtomicI32::new(0));
    let mut app = TimedApp {
        base: ApplicationBase::new(),
        stop: stop.clone(),
    };
    let start = Instant::now();
    assert_eq!(app.main(&["--time-limit=5"]), 0);
    assert_eq!(stop.load(Ordering::SeqCst), 14);
    assert!(start.elapsed() < Duration::from_secs(2));
}
