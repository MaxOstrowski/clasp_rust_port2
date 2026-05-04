use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use rust_clasp::potassco::program_opts::{
    IntrusiveRefCounted, IntrusiveSharedPtr, Option as ProgramOption, ParseValues, Parser, action,
    action_default, action_with_option_default, flag, flag_with, flag_with_init, make_custom,
    make_shared, parse, parse_with_option, store_false, store_to, store_to_init, store_to_with,
    value, values,
};

struct Foo {
    x: Cell<i32>,
    live_count: Rc<Cell<i32>>,
    rc: Cell<i32>,
}

impl Foo {
    fn new(live_count: Rc<Cell<i32>>) -> Self {
        live_count.set(live_count.get() + 1);
        Self {
            x: Cell::new(12),
            live_count,
            rc: Cell::new(1),
        }
    }
}

impl Drop for Foo {
    fn drop(&mut self) {
        self.live_count.set(self.live_count.get() - 1);
    }
}

impl IntrusiveRefCounted for Foo {
    fn intrusive_add_ref(&self) {
        self.rc.set(self.rc.get() + 1);
    }

    fn intrusive_release(&self) -> i32 {
        let next = self.rc.get() - 1;
        self.rc.set(next);
        next
    }

    fn intrusive_count(&self) -> i32 {
        self.rc.get()
    }
}

#[test]
fn intrusive_pointer_copy_move_and_reset_match_upstream_behavior() {
    let count = Rc::new(Cell::new(0));

    let mut ptr = make_shared(Foo::new(count.clone()));
    assert_eq!(ptr.count(), 1);
    assert!(ptr.unique());
    assert_eq!(count.get(), 1);

    {
        let ptr2 = ptr.clone();
        assert_eq!(ptr2.count(), 2);

        let ptr3 = ptr2.clone();
        assert_eq!(ptr3.count(), 3);
        assert!(!ptr3.unique());

        let ptr4 = ptr3.clone();
        assert_eq!(ptr4.count(), 4);
        ptr2.x.set(77);
    }

    assert_eq!(count.get(), 1);
    assert_eq!(ptr.x.get(), 77);

    let moved = ptr;
    assert_eq!(moved.count(), 1);
    assert!(moved.get().is_some());

    ptr = IntrusiveSharedPtr::default();
    assert!(ptr.get().is_none());

    let mut moved = moved;
    moved.reset();
    assert_eq!(count.get(), 0);
    assert!(moved.unique());
}

#[test]
fn store_to_uses_default_and_custom_parsers() {
    let mut number = 0;
    {
        let option = ProgramOption::new("foo", "bar", store_to(&mut number));
        assert!(option.assign("12"));
        assert!(!option.assign("13foo"));
    }
    assert_eq!(number, 12);

    let mut parsed = false;
    {
        let option = ProgramOption::new(
            "foo",
            "bar",
            store_to_with(&mut parsed, |value: &str, out: &mut bool| {
                *out = value == "bla";
                true
            }),
        );
        assert!(option.assign("bla"));
    }
    assert!(parsed);
}

#[test]
fn action_and_parse_factories_invoke_expected_callables() {
    let values = Rc::new(RefCell::new(BTreeMap::new()));
    {
        let values_for_o1 = values.clone();
        let values_for_o2 = values.clone();
        let values_for_o3 = values.clone();
        let option1 = ProgramOption::new(
            "foo",
            "",
            action_with_option_default::<i32, _>(move |opt, value| {
                values_for_o1
                    .borrow_mut()
                    .insert(opt.name().to_owned(), value);
            }),
        );
        let option2 = ProgramOption::new(
            "bar",
            "",
            action_default::<i32, _>(move |value| {
                values_for_o2.borrow_mut().insert("v2".to_owned(), value);
            }),
        );
        let option3 = ProgramOption::new(
            "baz",
            "",
            action::<i32, _, _>(
                move |value| {
                    values_for_o3.borrow_mut().insert("v3".to_owned(), value);
                },
                |input: &str, out: &mut i32| {
                    *out = input.parse().unwrap_or_default();
                    true
                },
            ),
        );

        assert!(option1.assign("123"));
        assert!(option2.assign("342"));
        assert!(option3.assign("456"));
    }
    let values = values.borrow();
    assert_eq!(values.get("foo"), Some(&123));
    assert_eq!(values.get("v2"), Some(&342));
    assert_eq!(values.get("v3"), Some(&456));

    let custom_values = Rc::new(RefCell::new(BTreeMap::new()));
    {
        let values_for_o1 = custom_values.clone();
        let values_for_o2 = custom_values.clone();
        let option1 = ProgramOption::new(
            "foo",
            "",
            parse_with_option(move |opt, value| {
                if let Ok(parsed) = value.parse::<i32>() {
                    values_for_o1
                        .borrow_mut()
                        .insert(opt.name().to_owned(), parsed);
                    true
                } else {
                    false
                }
            }),
        );
        let option2 = ProgramOption::new(
            "bar",
            "",
            parse(move |value| {
                if let Ok(parsed) = value.parse::<i32>() {
                    values_for_o2.borrow_mut().insert("v2".to_owned(), parsed);
                    true
                } else {
                    false
                }
            }),
        );

        assert!(option1.assign("123"));
        assert!(option2.assign("342"));
        assert!(!option1.assign("x12"));
    }
    let custom_values = custom_values.borrow();
    assert_eq!(custom_values.get("foo"), Some(&123));
    assert_eq!(custom_values.get("v2"), Some(&342));
}

#[test]
fn shared_custom_actions_keep_intrusive_counts_and_validate_values() {
    let values = Rc::new(RefCell::new(BTreeMap::new()));
    let values_for_action = values.clone();
    let action = make_custom(move |opt: &ProgramOption<'_>, value: &str| {
        if !value.is_empty() {
            values_for_action
                .borrow_mut()
                .insert(opt.name().to_owned(), value.to_owned());
            true
        } else {
            false
        }
    });
    assert_eq!(action.count(), 1);

    {
        let option1 = ProgramOption::new("foo", "", value(action.clone()));
        let option2 = ProgramOption::new("bar", "", value(action.clone()).implicit("234"));
        assert_eq!(action.count(), 3);
        assert!(option1.assign("123"));
        assert!(option2.assign(""));
        assert!(!option1.assign(""));
    }

    let values = values.borrow();
    assert_eq!(values.get("foo"), Some(&"123".to_owned()));
    assert_eq!(values.get("bar"), Some(&"234".to_owned()));

    assert!(action.unique());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Color {
    Red = 2,
    Green = 10,
    Blue = 20,
}

#[test]
fn parse_values_constructor_and_parser_allow_convertible_output_types() {
    let mut parser = ParseValues::new(vec![
        ("Red".to_owned(), 2_i32),
        ("Green".to_owned(), 10_i32),
        ("Blue".to_owned(), 20_i32),
    ]);
    let mut widened = 0_i64;

    assert!(parser.parse("GREEN", &mut widened));
    assert_eq!(widened, 10_i64);
    assert!(!parser.parse("Blu", &mut widened));
}

#[test]
fn values_factory_matches_allowed_values_case_insensitively() {
    let mut color = Color::Blue;
    {
        let option = ProgramOption::new(
            "foo",
            "",
            store_to_with(
                &mut color,
                values([
                    ("Red", Color::Red),
                    ("Green", Color::Green),
                    ("Blue", Color::Blue),
                ]),
            ),
        );

        assert!(option.assign("Red"));
        assert!(option.assign("GREEN"));
        assert!(!option.assign("Blu"));
    }
    assert_eq!(color, Color::Green);
}

#[test]
fn default_values_and_implicit_values_follow_upstream_rules() {
    let mut number = 0;
    {
        let option = ProgramOption::new(
            "some-int",
            "",
            store_to(&mut number).defaults_to("123").arg("<n>"),
        );
        assert_eq!(option.default_value(), "123");
        assert!(!option.defaulted());
        assert_eq!(option.arg_name(), "<n>");
        assert!(option.assign_default());
        assert!(option.defaulted());
    }
    assert_eq!(number, 123);

    {
        let option = ProgramOption::new(
            "some-int",
            "",
            store_to(&mut number).defaults_to("123").arg("<n>"),
        );
        assert!(option.assign_default_with("923"));
        assert!(option.defaulted());
        assert_eq!(option.default_value(), "923");
        assert!(option.assign_default_with("20"));
        assert!(option.assign_default_with(""));
        assert!(!option.defaulted());
        assert!(option.default_value().is_empty());
    }
    assert_eq!(number, 20);

    {
        let option = ProgramOption::new(
            "some-int",
            "",
            store_to(&mut number).defaults_to_assigned("123").arg("<n>"),
        );
        assert!(option.defaulted());
        assert!(option.assign("82"));
        assert!(!option.defaulted());
    }
    assert_eq!(number, 82);

    {
        let option = ProgramOption::new(
            "other-int",
            "",
            store_to(&mut number).defaults_to("123Hallo?"),
        );
        assert!(!option.assign_default());
        assert!(!option.defaulted());
    }

    {
        let option = ProgramOption::new("foo", "", store_to(&mut number).implicit("102"));
        assert!(option.assign(""));
        assert!(option.assign("456"));
    }
    assert_eq!(number, 456);
}

#[test]
fn flag_factories_preserve_boolean_semantics() {
    let mut loud = false;
    {
        let option = ProgramOption::new("foo", "bar", flag(&mut loud));
        assert!(option.implicit());
        assert!(option.flag());
        assert_eq!(option.implicit_value(), "1");
        assert!(option.assign(""));
    }
    assert!(loud);

    let mut loud = false;
    {
        let option = ProgramOption::new("foo", "bar", flag_with(&mut loud, store_false));
        assert!(option.assign(""));
        assert!(option.assign("0"));
    }
    assert!(loud);

    let got = Rc::new(Cell::new(true));
    {
        let got_for_option = got.clone();
        let option = ProgramOption::new(
            "quiet",
            "bar",
            flag_with(move |value| got_for_option.set(value), store_false),
        );
        assert!(option.assign(""));
        assert!(!got.get());
        assert!(option.assign("off"));
    }
    assert!(got.get());

    let mut flag_value = true;
    let _ = flag_with_init(&mut flag_value, false, |value: &str, out: &mut bool| {
        *out = value == "1";
        true
    });
    assert!(!flag_value);

    let _ = flag_with_init(&mut flag_value, true, store_false);
    assert!(flag_value);
}

#[test]
fn store_to_init_assigns_initial_value_before_building_description() {
    let mut value = 7;
    let arg_name = {
        let option = ProgramOption::new("foo", "", store_to_init(&mut value, 99));
        option.arg_name().to_owned()
    };
    assert_eq!(value, 99);
    assert_eq!(arg_name, "<arg>");
}
