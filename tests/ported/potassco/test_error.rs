use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::enums::EnumTag;
use rust_clasp::potassco::error::{
    Errc, Error, FailureCode, RuntimeError, safe_cast, safe_cast_enum, scope_exit,
    set_abort_handler, size_cast, translate_ec,
};
use rust_clasp::potassco::platform::{ExpressionInfo, SourceLocation};
use rust_clasp::{
    capture_expression, current_location, potassco_assert, potassco_assert_not_reached,
    potassco_check, potassco_check_pre, potassco_debug_assert, potassco_debug_check_pre,
    potassco_fail,
};

#[derive(Debug)]
struct UserError(String);

#[derive(Clone, Copy)]
struct UserErrorCode;

impl FailureCode for UserErrorCode {
    fn fail(self, info: ExpressionInfo, message: Option<String>) -> ! {
        let mut text = message.unwrap_or_default();
        text.push_str(" with failed expression: ");
        text.push_str(info.expression);
        panic::panic_any(UserError(text))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum Tiny {
    Zero = 0,
    Big = 255,
}

impl EnumTag for Tiny {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Zero),
            255 => Some(Self::Big),
            _ => None,
        }
    }
}

fn make_location(location: &SourceLocation, include_file: bool, tail: &str) -> String {
    let mut out = String::new();
    if include_file {
        out.push_str(ExpressionInfo::relative_file_name(location));
    } else {
        out.push_str(location.function);
    }
    out.push(':');
    out.push_str(&location.line.to_string());
    if include_file {
        out.push_str(": ");
        out.push_str(location.function);
    }
    out.push(':');
    if !tail.is_empty() {
        out.push(' ');
        out.push_str(tail);
    }
    out
}

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

fn catch_abort_message<F>(func: F) -> String
where
    F: FnOnce(),
{
    fn raise_message(message: &str) {
        panic::panic_any(message.to_owned())
    }

    let old = set_abort_handler(Some(raise_message));
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    let _ = set_abort_handler(old);
    *payload
        .downcast::<String>()
        .expect("expected abort message")
}

#[test]
fn fail_throw_formats_recoverable_errors() {
    let expression = capture_expression!(expression);
    let location = make_location(&expression.location, false, "");
    let expected_default = Errc::INVALID_ARGUMENT.message();

    let no_message = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(Errc::INVALID_ARGUMENT, expression, None)
    });
    assert_eq!(
        no_message,
        Error::InvalidArgument(format!(
            "{}\n{} check 'expression' failed.",
            expected_default, location
        ))
    );

    let without_expression = ExpressionInfo {
        expression: "",
        ..expression
    };
    let no_expression = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(Errc::INVALID_ARGUMENT, without_expression, None)
    });
    assert_eq!(
        no_expression,
        Error::InvalidArgument(format!("{}\n{} failed.", expected_default, location))
    );

    let with_message = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(
            Errc::INVALID_ARGUMENT,
            expression,
            Some(format!("{} {}", "custom message with args", "1 bla")),
        )
    });
    assert_eq!(
        with_message,
        Error::InvalidArgument(format!(
            "custom message with args 1 bla: {}\n{} check 'expression' failed.",
            expected_default, location
        ))
    );
}

#[test]
fn fail_throw_formats_preconditions_and_runtime_errors() {
    let expression = capture_expression!(expression);
    let location = make_location(&expression.location, false, "");

    let precondition = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(Errc::PRECONDITION_FAIL, expression, None)
    });
    assert_eq!(
        precondition,
        Error::InvalidArgument(format!("{} Precondition 'expression' failed.", location))
    );

    let without_expression = ExpressionInfo {
        expression: "",
        ..expression
    };
    let precondition_without_expression = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(Errc::PRECONDITION_FAIL, without_expression, None)
    });
    assert_eq!(
        precondition_without_expression,
        Error::InvalidArgument(format!("{} Precondition failed.", location))
    );

    let precondition_with_message = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(
            Errc::PRECONDITION_FAIL,
            expression,
            Some("custom message".to_owned()),
        )
    });
    assert_eq!(
        precondition_with_message,
        Error::InvalidArgument(format!(
            "{} Precondition 'expression' failed.\nmessage: custom message",
            location
        ))
    );

    let runtime = catch_error(|| {
        rust_clasp::potassco::error::fail_throw(9_i32, expression, Some("my message".to_owned()))
    });
    match runtime {
        Error::Runtime(RuntimeError { .. }) => {
            assert!(runtime.to_string().starts_with("my message: "));
            assert!(runtime.to_string().contains("check 'expression' failed."));
        }
        other => panic!("expected runtime error, got {other:?}"),
    }
}

#[test]
fn fail_throw_maps_standard_error_families() {
    let expression = capture_expression!(expr);

    assert!(matches!(
        catch_error(|| rust_clasp::potassco::error::fail_throw(
            Errc::LENGTH_ERROR,
            expression,
            Some("x".into())
        )),
        Error::LengthError(_)
    ));
    assert!(matches!(
        catch_error(|| rust_clasp::potassco::error::fail_throw(
            Errc::DOMAIN_ERROR,
            expression,
            Some("x".into())
        )),
        Error::DomainError(_)
    ));
    assert!(matches!(
        catch_error(|| rust_clasp::potassco::error::fail_throw(
            Errc::OUT_OF_RANGE,
            expression,
            Some("x".into())
        )),
        Error::OutOfRange(_)
    ));
    assert!(matches!(
        catch_error(|| rust_clasp::potassco::error::fail_throw(
            Errc::OVERFLOW_ERROR,
            expression,
            Some("x".into())
        )),
        Error::OverflowError(_)
    ));
    assert_eq!(
        catch_error(|| rust_clasp::potassco::error::fail_throw(
            Errc::BAD_ALLOC,
            expression,
            Some("x".into())
        )),
        Error::BadAlloc
    );
}

#[test]
fn fail_abort_formats_assertions_via_abort_handler() {
    let expression = capture_expression!(expression);
    let location = make_location(&expression.location, true, "Assertion 'expression' failed.");

    let no_message = catch_abort_message(|| {
        rust_clasp::potassco::error::fail_abort(expression, None);
    });
    assert_eq!(no_message, location);

    let without_expression = ExpressionInfo {
        expression: "",
        ..expression
    };
    let no_expression = catch_abort_message(|| {
        rust_clasp::potassco::error::fail_abort(without_expression, None);
    });
    assert_eq!(
        no_expression,
        make_location(&expression.location, true, "Assertion failed.")
    );

    let with_message = catch_abort_message(|| {
        rust_clasp::potassco::error::fail_abort(expression, Some("custom message".to_owned()));
    });
    assert_eq!(
        with_message,
        format!("{}\nmessage: custom message", location)
    );
}

#[test]
fn macros_preserve_failure_behavior() {
    assert_eq!(translate_ec(-22).raw(), Errc::INVALID_ARGUMENT.raw());
    assert_eq!(
        translate_ec(Errc::DOMAIN_ERROR).raw(),
        Errc::DOMAIN_ERROR.raw()
    );

    assert_eq!(
        catch_error(|| potassco_fail!(Errc::BAD_ALLOC)),
        Error::BadAlloc
    );
    assert!(matches!(
        catch_error(|| potassco_fail!(Errc::LENGTH_ERROR, "at most {} allowed", 3)),
        Error::LengthError(message) if message.contains("at most 3 allowed")
    ));

    potassco_check!(true, Errc::INVALID_ARGUMENT);
    potassco_check!(true, Errc::INVALID_ARGUMENT, "foo");
    potassco_check_pre!(true);
    potassco_check_pre!(true, "custom message");
    potassco_debug_check_pre!(true);
    potassco_debug_assert!(true);

    assert!(matches!(
        catch_error(|| potassco_check!(false, Errc::DOMAIN_ERROR)),
        Error::DomainError(message) if message.contains("check 'false' failed")
    ));
    assert!(matches!(
        catch_error(|| potassco_check!(false, 84_i32, "Message {}", 2)),
        Error::Runtime(message) if message.what().contains("Message 2")
    ));
    assert_eq!(
        catch_error(|| potassco_check!(false, Errc::BAD_ALLOC, "Message {}", 2)),
        Error::BadAlloc
    );
    assert!(matches!(
        catch_error(|| potassco_check!(1 != 1, -22_i32)),
        Error::InvalidArgument(message) if message.contains("check '1 != 1' failed")
    ));
    let payload = panic::catch_unwind(AssertUnwindSafe(|| {
        potassco_check!(1 != 1, UserErrorCode, "found via trait")
    }))
    .expect_err("expected custom panic");
    assert_eq!(
        payload
            .downcast::<UserError>()
            .expect("custom panic payload")
            .0,
        "found via trait with failed expression: 1 != 1"
    );

    assert!(matches!(
        catch_error(|| potassco_check_pre!(false)),
        Error::InvalidArgument(message) if message.contains("Precondition 'false' failed")
    ));
    assert!(matches!(
        catch_error(|| potassco_check_pre!(false, "{} {}", "foo", 2)),
        Error::InvalidArgument(message) if message.contains("foo 2")
    ));

    assert!(catch_abort_message(|| potassco_assert!(false)).contains("Assertion 'false' failed."));
    assert!(
        catch_abort_message(|| potassco_assert!(false, "Fail {}", 123))
            .contains("message: Fail 123")
    );
    assert!(
        catch_abort_message(|| potassco_assert_not_reached!("foo"))
            .contains("Assertion 'not reached' failed.")
    );
}

#[test]
fn runtime_error_accessors_split_message_and_details() {
    let location = current_location!();
    let error = RuntimeError::new(
        Errc::from_raw(4),
        location,
        "head line\ntrailing details".to_owned(),
    );
    assert_eq!(error.errc().raw(), 4);
    assert_eq!(error.location(), &location);
    assert_eq!(error.message(), "head line");
    assert_eq!(error.details(), "trailing details");
}

#[test]
fn scope_exit_runs_on_exit_and_during_unwind() {
    let called = Cell::new(false);
    {
        let _guard = scope_exit(|| called.set(true));
        assert!(!called.get());
    }
    assert!(called.get());

    let called = Cell::new(false);
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        let _guard = scope_exit(|| called.set(true));
        panic!("boom");
    }));
    assert!(called.get());
}

#[test]
fn scope_exit_can_panic_and_nest() {
    let payload = panic::catch_unwind(AssertUnwindSafe(|| {
        let _guard = scope_exit(|| panic!("foo"));
    }))
    .expect_err("expected panic");
    assert_eq!(
        *payload.downcast::<&'static str>().expect("panic payload"),
        "foo"
    );

    let output = RefCell::new(String::new());
    {
        let _outer = scope_exit(|| {
            output.borrow_mut().push('1');
            let _nested = scope_exit(|| output.borrow_mut().push_str("nest"));
            output.borrow_mut().push('1');
        });
        let _second = scope_exit(|| output.borrow_mut().push('2'));
    }
    assert_eq!(output.into_inner(), "211nest");
}

#[test]
fn safe_cast_and_size_cast_follow_out_of_range_rules() {
    let narrowed: u8 = safe_cast(255_u16);
    assert_eq!(narrowed, 255);
    let enum_value: u8 = safe_cast_enum(Tiny::Big);
    assert_eq!(enum_value, 255);
    let queue = VecDeque::from([1_u8, 2, 3]);
    let sized: u8 = size_cast(&queue);
    assert_eq!(sized, 3);

    assert!(matches!(
        catch_error(|| {
            let _: u8 = safe_cast(256_u16);
        }),
        Error::OutOfRange(message) if message.contains("check 'from' failed")
    ));
}
