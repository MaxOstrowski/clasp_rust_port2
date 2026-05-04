//! Port target for original_clasp/libpotassco/tests/test_options.cpp.

use std::cell::{Cell, RefCell};
use std::io::{self, Cursor, Write};
use std::rc::Rc;

use rust_clasp::potassco::program_opts::{
    AmbiguousOption, AppendSink, COMMAND_LINE_ALLOW_FLAG_VALUE, ContextError, ContextErrorType,
    DefaultFormat, DefaultFormatElement, DefaultParseContext, DescriptionLevel, DuplicateOption,
    Error, FindType, OptState, Option as ProgramOption, OptionContext, OptionGroup,
    OptionGroupInit, ParseContext, Str, SyntaxErrorType, UnknownGroup, UnknownOption, ValueAction,
    ValueActionPtr, ValueActionRelease, ValueDesc, ValueErrorType, action_default, flag,
    make_action, make_custom, parse, parse_cfg_file, parse_command_array, parse_command_string,
    store_to, store_to_with, value, value_with_id,
};

struct RecordingAction {
    calls: Rc<RefCell<Vec<String>>>,
}

impl<'a> ValueAction<'a> for RecordingAction {
    fn assign(&mut self, _opt: &ProgramOption<'a>, value: &str) -> bool {
        self.calls.borrow_mut().push(value.to_owned());
        true
    }
}

struct DefaultReleaseAction {
    drops: Rc<Cell<u32>>,
}

impl Drop for DefaultReleaseAction {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl<'a> ValueAction<'a> for DefaultReleaseAction {
    fn assign(&mut self, _opt: &ProgramOption<'a>, _value: &str) -> bool {
        true
    }
}

struct RetainedAction {
    drops: Rc<Cell<u32>>,
}

impl Drop for RetainedAction {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl<'a> ValueAction<'a> for RetainedAction {
    fn assign(&mut self, _opt: &ProgramOption<'a>, _value: &str) -> bool {
        true
    }

    fn release(&mut self) -> bool {
        false
    }
}

#[test]
fn value_action_release_matches_upstream_deleter_behavior() {
    let releaser = ValueActionRelease;

    let default_drops = Rc::new(Cell::new(0));
    let mut default_ptr: Option<ValueActionPtr<'_>> = Some(Box::new(DefaultReleaseAction {
        drops: default_drops.clone(),
    }));
    releaser.call(&mut default_ptr);
    assert!(default_ptr.is_none());
    assert_eq!(default_drops.get(), 1);

    let retained_drops = Rc::new(Cell::new(0));
    let mut retained_ptr: Option<ValueActionPtr<'_>> = Some(Box::new(RetainedAction {
        drops: retained_drops.clone(),
    }));
    releaser.call(&mut retained_ptr);
    assert!(retained_ptr.is_some());
    assert_eq!(retained_drops.get(), 0);

    drop(retained_ptr);
    assert_eq!(retained_drops.get(), 1);
}

#[test]
fn str_detects_literal_and_dynamic_values_and_supports_prefix_removal() {
    let lit = Str::literal("Hallo");
    assert!(lit.is_lit());
    assert_eq!(lit.str(), "Hallo");

    let empty = Str::default();
    assert!(empty.is_lit());
    assert!(empty.empty());

    let dynamic_source = String::from("Hallo");
    let from_owned = Str::from(dynamic_source.clone());
    let from_ref = Str::from(&dynamic_source);
    let from_slice = Str::from(dynamic_source.as_str());
    assert!(!from_owned.is_lit());
    assert!(!from_ref.is_lit());
    assert!(!from_slice.is_lit());
    assert_eq!(from_owned.str(), "Hallo");
    assert_eq!(from_ref.size(), 5);
    assert_eq!(from_slice.size(), 5);

    let mut lit = Str::literal("Hallo");
    let mut dynamic = Str::from(dynamic_source);
    lit.remove_prefix(2);
    dynamic.remove_prefix(4);
    assert_eq!(lit.str(), "llo");
    assert_eq!(dynamic.str(), "o");
    assert_eq!(lit.size(), 3);
    assert_eq!(dynamic.size(), 1);
}

#[test]
fn str_clone_plus_reassignment_matches_copy_assignment_observables() {
    let literal_source = Str::literal("Hallo");
    let dynamic_source = Str::dynamic("Hallo");

    let mut literal_assigned = Str::default();
    let mut dynamic_assigned = Str::default();
    assert!(literal_assigned.empty());
    assert!(dynamic_assigned.empty());

    literal_assigned = literal_source.clone();
    dynamic_assigned = dynamic_source.clone();

    assert!(literal_assigned.is_lit());
    assert!(!dynamic_assigned.is_lit());
    assert_eq!(literal_assigned.str(), "Hallo");
    assert_eq!(dynamic_assigned.str(), "Hallo");

    literal_assigned.remove_prefix(2);
    dynamic_assigned.remove_prefix(4);

    assert_eq!(literal_source.str(), "Hallo");
    assert_eq!(dynamic_source.str(), "Hallo");
    assert_eq!(literal_assigned.str(), "llo");
    assert_eq!(dynamic_assigned.str(), "o");
}

#[test]
fn value_desc_and_option_copy_runtime_strings_and_preserve_builder_flags() {
    let mut number = 0;
    let default_value = String::from("defaultValue");
    let arg_name = String::from("<foo,bar>");
    let implicit_value = String::from("1234");

    let desc = store_to(&mut number)
        .defaults_to(&default_value)
        .arg(&arg_name)
        .implicit(&implicit_value)
        .negatable()
        .composing()
        .level(DescriptionLevel::E2);
    assert_eq!(desc.default_value().str(), "defaultValue");
    assert_eq!(desc.arg_name().str(), "<foo,bar>");
    assert_eq!(desc.implicit_value().str(), "1234");
    assert_eq!(desc.desc_level(), DescriptionLevel::E2);
    assert!(desc.is_negatable());
    assert!(desc.is_composing());
    assert!(desc.is_implicit());
    assert!(!desc.is_flag());
    assert!(!desc.is_defaulted());

    drop(desc);
    let mut default_value = default_value;
    let mut arg_name = arg_name;
    let mut implicit_value = implicit_value;
    let option = ProgramOption::new(
        "number",
        String::from("Some option description coming from elsewhere"),
        store_to(&mut number)
            .defaults_to(&default_value)
            .arg(&arg_name)
            .implicit(&implicit_value)
            .negatable()
            .composing()
            .level(DescriptionLevel::E2),
    );
    default_value.clear();
    arg_name.clear();
    implicit_value.clear();
    assert_eq!(option.default_value(), "defaultValue");
    assert_eq!(option.arg_name(), "<foo,bar>");
    assert_eq!(option.implicit_value(), "1234");
    assert_eq!(
        option.description(),
        "Some option description coming from elsewhere"
    );
    assert!(option.negatable());
    assert!(option.composing());
    assert!(option.implicit());
    assert_eq!(option.desc_level(), DescriptionLevel::E2);
}

#[test]
fn flag_keeps_explicit_implicit_value() {
    let mut number = 0;
    let option = ProgramOption::new("flag", "", store_to(&mut number).implicit("2").flag());
    assert_eq!(option.arg_name(), "");
    assert_eq!(option.implicit_value(), "2");
}

#[test]
fn action_factories_support_explicit_ids_for_boxed_and_shared_actions() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let boxed_option = ProgramOption::new(
        "boxed",
        "",
        value_with_id(
            make_action(RecordingAction {
                calls: calls.clone(),
            }),
            17,
        ),
    );
    assert_eq!(boxed_option.id(), 17);
    assert!(boxed_option.assign("42"));

    let shared_calls = Rc::new(RefCell::new(Vec::new()));
    let shared_calls_for_action = shared_calls.clone();
    let shared_action = make_custom(move |_opt: &ProgramOption<'_>, value: &str| {
        shared_calls_for_action.borrow_mut().push(value.to_owned());
        true
    });
    let shared_option = ProgramOption::new("shared", "", value_with_id(shared_action, 23));
    assert_eq!(shared_option.id(), 23);
    assert!(shared_option.assign("99"));

    let desc = ValueDesc::new(
        make_action(RecordingAction {
            calls: calls.clone(),
        }),
        31,
    );
    let explicit_option = ProgramOption::new("explicit", "", desc);
    assert_eq!(explicit_option.id(), 31);
    assert!(explicit_option.assign("7"));

    assert_eq!(calls.borrow().as_slice(), ["42", "7"]);
    assert_eq!(shared_calls.borrow().as_slice(), ["99"]);

    let passthrough_calls = Rc::new(RefCell::new(Vec::new()));
    let passthrough = ProgramOption::new(
        "passthrough",
        "",
        value(make_action(RecordingAction {
            calls: passthrough_calls.clone(),
        })),
    );
    assert!(passthrough.assign("123"));
    assert_eq!(passthrough_calls.borrow().as_slice(), ["123"]);
}

#[test]
fn value_desc_action_accessor_reflects_default_boxed_and_shared_actions() {
    let empty_desc = ValueDesc::default();
    assert!(empty_desc.action_ref().is_none());
    assert_eq!(empty_desc.id(), 0);

    let boxed_desc = ValueDesc::new(
        make_action(RecordingAction {
            calls: Rc::new(RefCell::new(Vec::new())),
        }),
        31,
    );
    assert!(boxed_desc.action_ref().is_some());
    assert_eq!(boxed_desc.id(), 31);

    let shared_action = make_custom(|_opt: &ProgramOption<'_>, _value: &str| true);
    let shared_desc = value_with_id(shared_action, 23);
    assert!(shared_desc.action_ref().is_some());
    assert_eq!(shared_desc.id(), 23);
}

#[test]
fn value_desc_defaults_to_assigned_matches_upstream_defaulted_construction() {
    let mut assigned = Cell::new(0);
    let desc = store_to_with(&mut assigned, |value: &str, slot: &mut Cell<i32>| {
        value.parse::<i32>().map(|parsed| slot.set(parsed)).is_ok()
    })
    .defaults_to_assigned("123")
    .arg("<n>");
    assert_eq!(desc.default_value().str(), "123");
    assert!(desc.is_defaulted());

    {
        let option = ProgramOption::new("some-int", "some integer", desc);
        assert_eq!(option.default_value(), "123");
        assert!(option.defaulted());
        assert!(option.assign("82"));
        assert!(!option.defaulted());
    }
    assert_eq!(assigned.get(), 82);
}

fn negatable_int(input: &str, out: &mut i32) -> bool {
    if input == "no" {
        *out = 0;
        true
    } else {
        input.parse::<i32>().map(|value| *out = value).is_ok()
    }
}

fn assert_context_error(error: Error, kind: ContextErrorType) {
    match error {
        Error::Context(error) => assert_eq!(error.kind(), kind),
        other => panic!("expected context error, got {other:?}"),
    }
}

fn assert_syntax_error(error: Error, kind: SyntaxErrorType) {
    match error {
        Error::Syntax(error) => assert_eq!(error.kind(), kind),
        other => panic!("expected syntax error, got {other:?}"),
    }
}

fn assert_value_error(error: Error, kind: ValueErrorType) {
    match error {
        Error::Value(error) => assert_eq!(error.kind(), kind),
        other => panic!("expected value error, got {other:?}"),
    }
}

#[test]
fn context_error_wrappers_preserve_types_and_messages() {
    let duplicate = DuplicateOption::new("ctx", "opt");
    assert_eq!(duplicate.r#type(), ContextErrorType::DuplicateOption);
    assert_eq!(duplicate.kind(), ContextErrorType::DuplicateOption);
    assert_eq!(duplicate.ctx(), "ctx");
    assert_eq!(duplicate.key(), "opt");
    assert_eq!(
        duplicate.to_string(),
        "In context 'ctx': duplicate option: 'opt'"
    );

    let unknown = UnknownOption::new("", "flag");
    assert_eq!(unknown.kind(), ContextErrorType::UnknownOption);
    assert_eq!(unknown.to_string(), "unknown option: 'flag'");

    let ambiguous = AmbiguousOption::new("ctx", "he", "  help\n  hello");
    assert_eq!(ambiguous.kind(), ContextErrorType::AmbiguousOption);
    assert_eq!(
        ambiguous.to_string(),
        "In context 'ctx': ambiguous option: 'he' could be:\n  help\n  hello"
    );

    let unknown_group = UnknownGroup::new("ctx", "hidden");
    assert_eq!(unknown_group.kind(), ContextErrorType::UnknownGroup);
    assert_eq!(
        unknown_group.to_string(),
        "In context 'ctx': unknown group: 'hidden'"
    );

    let duplicate_context: ContextError = duplicate.clone().into();
    assert_eq!(duplicate_context.kind(), ContextErrorType::DuplicateOption);

    let duplicate_error: Error = duplicate.into();
    assert_context_error(duplicate_error, ContextErrorType::DuplicateOption);
}

#[test]
fn syntax_error_exposes_cpp_style_type_accessor() {
    let error =
        rust_clasp::potassco::program_opts::SyntaxError::new(SyntaxErrorType::MissingValue, "flag");
    assert_eq!(error.r#type(), SyntaxErrorType::MissingValue);
    assert_eq!(error.key(), "flag");
    assert_eq!(error.to_string(), "SyntaxError: 'flag' requires a value!");
}

#[test]
fn value_error_exposes_cpp_style_type_accessor() {
    let error = rust_clasp::potassco::program_opts::ValueError::new(
        "ctx",
        ValueErrorType::InvalidValue,
        "flag",
        "abc",
        "",
    );
    assert_eq!(error.r#type(), ValueErrorType::InvalidValue);
    assert_eq!(error.ctx(), "ctx");
    assert_eq!(error.key(), "flag");
    assert_eq!(error.value(), "abc");
    assert_eq!(
        error.to_string(),
        "In context 'ctx': 'abc' invalid value for: 'flag'"
    );
}

#[test]
fn option_group_init_applies_specs_and_context_manages_groups_aliases_and_prefixes() {
    let mut desc = ValueDesc::default();
    let mut alias = '\0';
    assert!(OptionGroupInit::apply_spec(
        "+!*@3-f", &mut desc, &mut alias
    ));
    assert_eq!(alias, 'f');
    assert!(desc.is_composing());
    assert!(desc.is_negatable());
    assert!(desc.is_flag());
    assert_eq!(desc.desc_level(), DescriptionLevel::E3);
    assert!(!OptionGroupInit::apply_spec("-fo", &mut desc, &mut alias));

    let mut flag_a = false;
    let mut flag_b = false;
    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    let mut base = OptionGroup::new("Base", DescriptionLevel::E1);
    {
        let mut init = base.add_options();
        init.add("@2,opt1", flag(&mut flag_a), "option 1").unwrap();
    }
    ctx.add(base).unwrap();
    {
        let mut init = ctx.add_options("Base", DescriptionLevel::Default);
        init.add("-o,opt2", flag(&mut flag_b), "option 2").unwrap();
    }

    assert_eq!(ctx.groups(), 1);
    let base = ctx.group("Base").unwrap();
    assert_eq!(base.size(), 2);
    assert_eq!(base.desc_level(), DescriptionLevel::Default);
    assert_eq!(ctx.option("-o", FindType::ALIAS).unwrap().name(), "opt2");
    assert_eq!(ctx.option("opt1", FindType::NAME).unwrap().name(), "opt1");

    ctx.add_alias(ctx.index_of("opt1", FindType::NAME).unwrap(), "Hilfe")
        .unwrap();
    assert_eq!(ctx.option("Hilfe", FindType::NAME).unwrap().name(), "opt1");

    match ctx.option("opt", FindType::PREFIX) {
        Err(error) => assert_context_error(error, ContextErrorType::AmbiguousOption),
        Ok(_) => panic!("expected ambiguous prefix lookup"),
    }
}

#[test]
fn default_format_column_width_matches_cpp_rules() {
    let long_only = ProgramOption::new("number", "a number", ValueDesc::default());
    assert_eq!(DefaultFormat::column_width(&long_only), 16);

    let with_alias = ProgramOption::with_alias("number", "a number", ValueDesc::default(), 'n');
    assert_eq!(DefaultFormat::column_width(&with_alias), 19);

    let flag_option =
        ProgramOption::with_alias("number", "a number", ValueDesc::default().flag(), 'n');
    assert_eq!(DefaultFormat::column_width(&flag_option), 13);

    let negatable =
        ProgramOption::with_alias("number", "a number", ValueDesc::default().negatable(), 'n');
    assert_eq!(DefaultFormat::column_width(&negatable), 22);

    let negatable_flag = ProgramOption::with_alias(
        "number",
        "a number",
        ValueDesc::default().negatable().flag(),
        'n',
    );
    assert_eq!(DefaultFormat::column_width(&negatable_flag), 18);
}

#[test]
fn default_format_formats_options_and_group_captions_like_cpp() {
    let mut out = String::new();

    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::new("number", "a number", ValueDesc::default()),
        0,
        None,
    );
    assert_eq!(out, "  --number=<arg>: a number\n");

    out.clear();
    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::with_alias("number", "a number", ValueDesc::default(), 'n'),
        0,
        None,
    );
    assert_eq!(out, "  -n,--number <arg>: a number\n");

    out.clear();
    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::with_alias("number", "a number", ValueDesc::default().flag(), 'n'),
        0,
        None,
    );
    assert_eq!(out, "  -n,--number: a number\n");

    out.clear();
    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::with_alias("number", "a number", ValueDesc::default().negatable(), 'n'),
        0,
        None,
    );
    assert_eq!(out, "  -n,--number <arg>|no: a number\n");

    out.clear();
    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::with_alias(
            "number",
            "a number",
            ValueDesc::default().negatable().flag(),
            'n',
        ),
        0,
        None,
    );
    assert_eq!(out, "  -n,--[no-]number: a number\n");

    out.clear();
    DefaultFormat::format_option(
        &mut out,
        &ProgramOption::new("number", "a number", ValueDesc::default()),
        20,
        None,
    );
    assert_eq!(out, "  --number=<arg>    : a number\n");

    let empty = OptionGroup::new("", DescriptionLevel::Default);
    out.clear();
    DefaultFormat::format_group(&mut out, &empty, None);
    assert!(out.is_empty());

    let basic = OptionGroup::new("Basic Options", DescriptionLevel::Default);
    out.clear();
    DefaultFormat::format_group(&mut out, &basic, None);
    assert_eq!(out, "\nBasic Options:\n\n");
}

#[test]
fn default_parse_context_lifecycle_matches_cpp_behavior() {
    let assigned = Rc::new(Cell::new(0));
    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let assigned_target = assigned.clone();
        let mut init = group.add_options();
        init.add(
            "value",
            action_default::<i32, _>(move |value| assigned_target.set(value)),
            "an int",
        )
        .unwrap();
    }

    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    ctx.add(group).unwrap();

    let opt = ctx.option("value", FindType::NAME).unwrap();
    let mut parse = DefaultParseContext::new(&ctx);
    assert!(parse.parsed().empty());
    assert_eq!(parse.state(&opt), OptState::Open);

    let fetched = parse.get_option("value", FindType::NAME).unwrap();
    assert_eq!(fetched.name(), "value");

    parse.set_value(&opt, "7").unwrap();
    assert_eq!(assigned.get(), 7);
    assert_eq!(parse.state(&opt), OptState::Seen);
    assert!(!parse.parsed().contains("value"));

    parse.finish(None);
    assert!(parse.parsed().contains("value"));
    assert_eq!(parse.state(&opt), OptState::Skip);

    parse.clear_parsed();
    assert!(parse.parsed().empty());
    assert_eq!(parse.state(&opt), OptState::Open);
}

#[test]
fn option_assignment_and_default_state_match_cpp_behavior() {
    let assigned = Rc::new(Cell::new(0));
    let option = ProgramOption::with_alias(
        "some-int",
        "some integer",
        action_default::<i32, _>({
            let assigned = assigned.clone();
            move |value| assigned.set(value)
        })
        .defaults_to("123")
        .arg("<n>")
        .implicit("77")
        .composing()
        .negatable()
        .level(DescriptionLevel::E2),
        's',
    );

    assert_eq!(option.name(), "some-int");
    assert_eq!(option.alias(), 's');
    assert_eq!(option.description(), "some integer");
    assert_eq!(option.arg_name(), "<n>");
    assert_eq!(option.default_value(), "123");
    assert_eq!(option.implicit_value(), "77");
    assert_eq!(option.desc_level(), DescriptionLevel::E2);
    assert!(option.composing());
    assert!(option.negatable());
    assert!(option.implicit());
    assert!(!option.flag());
    assert!(!option.defaulted());

    assert!(option.assign_default());
    assert_eq!(assigned.get(), 123);
    assert!(option.defaulted());

    assert!(option.assign("82"));
    assert_eq!(assigned.get(), 82);
    assert!(!option.defaulted());

    assert!(option.assign(""));
    assert_eq!(assigned.get(), 77);
    assert!(!option.defaulted());

    assert!(option.assign_default_with("923"));
    assert_eq!(assigned.get(), 923);
    assert_eq!(option.default_value(), "923");
    assert!(option.defaulted());

    assert!(option.assign_default_with(""));
    assert_eq!(assigned.get(), 923);
    assert_eq!(option.default_value(), "");
    assert!(!option.defaulted());

    let mut enabled = false;
    {
        let flag_option = ProgramOption::new("flag", "", flag(&mut enabled));
        assert!(flag_option.flag());
        assert!(flag_option.implicit());
        assert_eq!(flag_option.arg_name(), "");
        assert_eq!(flag_option.implicit_value(), "1");
        assert!(flag_option.assign(""));
    }
    assert!(enabled);
}

#[test]
fn default_format_expands_placeholders_and_formats_negatable_options() {
    let mut flag_value = false;
    let mut number = 0;
    let option = ProgramOption::with_alias(
        "number",
        "Some int %A (Default: %D, Implicit: %I) in %%",
        store_to(&mut number)
            .arg("<n>")
            .defaults_to("99")
            .implicit("12")
            .negatable(),
        'n',
    );
    assert!(option.assign_default());
    let mut out = String::new();
    DefaultFormat::format_option(&mut out, &option, 0, None);
    assert!(out.contains("-n,--number[=<n>|no]"));
    assert!(out.contains("Some int <n> (Default: 99, Implicit: 12) in %"));

    let mut group = OptionGroup::new("Basic Options", DescriptionLevel::Default);
    {
        let mut init = group.add_options();
        init.add("!-f,flag", flag(&mut flag_value), "some negatable flag")
            .unwrap();
    }
    let help = {
        let mut ctx = OptionContext::new("", DescriptionLevel::Default);
        ctx.add(group).unwrap();
        format!("{ctx}")
    };
    assert!(help.contains("Basic Options:"));
    assert!(help.contains("--[no-]flag"));

    let mut styled = String::new();
    DefaultFormat::format_option(
        &mut styled,
        &ProgramOption::with_alias("flag", "a number", ValueDesc::default().negatable(), 'f'),
        0,
        Some(&|element, open| match (element, open) {
            (DefaultFormatElement::Alias, true) => "<alias>",
            (DefaultFormatElement::Alias, false) => "</alias>",
            (DefaultFormatElement::Name, true) => "<name>",
            (DefaultFormatElement::Name, false) => "</name>",
            (DefaultFormatElement::Arg, true) => "<arg>",
            (DefaultFormatElement::Arg, false) => "</arg>",
            (DefaultFormatElement::Description, true) => "<desc>",
            (DefaultFormatElement::Description, false) => "</desc>",
            _ => "",
        }),
    );
    assert_eq!(
        styled,
        "  <alias>-f</alias>,<name>--flag</name> <arg><arg>|no</arg>: <desc>a number</desc>\n"
    );
}

#[test]
fn default_parse_context_tracks_seen_and_parsed_values_and_assigns_defaults() {
    let int1 = Rc::new(Cell::new(0));
    let int2 = Rc::new(Cell::new(0));
    let flag_value = Rc::new(Cell::new(false));
    let int3 = Rc::new(Cell::new(0));
    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let int1_target = int1.clone();
        let int2_target = int2.clone();
        let flag_target = flag_value.clone();
        let int3_target = int3.clone();
        let mut init = group.add_options();
        init.add(
            "int1",
            action_default::<i32, _>(move |value| int1_target.set(value)),
            "an int",
        )
        .unwrap();
        init.add(
            "int2",
            action_default::<i32, _>(move |value| int2_target.set(value)).defaults_to("10"),
            "another int",
        )
        .unwrap();
        init.add(
            "!,flag",
            action_default::<bool, _>(move |value| flag_target.set(value)).flag(),
            "a flag",
        )
        .unwrap();
        init.add(
            "int3",
            action_default::<i32, _>(move |value| int3_target.set(value)),
            "yet another int",
        )
        .unwrap();
    }
    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    ctx.add(group).unwrap();
    let mut parse = DefaultParseContext::new(&ctx);

    parse_command_string(
        &mut parse,
        "--int1=2 --flag --int3=3",
        None,
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    ctx.assign_defaults(parse.parsed()).unwrap();
    assert_eq!(int1.get(), 2);
    assert_eq!(int2.get(), 10);
    assert!(flag_value.get());
    assert_eq!(int3.get(), 3);
    assert!(parse.parsed().contains("int1"));
    assert!(!parse.parsed().contains("int2"));

    parse_command_string(
        &mut parse,
        "--int1=3 --no-flag --int2=4 --int3=5",
        None,
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    assert_eq!(int1.get(), 2);
    assert_eq!(int2.get(), 4);
    assert!(flag_value.get());
    assert_eq!(int3.get(), 3);

    parse.clear_parsed();
    parse_command_string(
        &mut parse,
        "--int1=3 --no-flag --int2=5 --int3=5",
        None,
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    assert_eq!(int1.get(), 3);
    assert_eq!(int2.get(), 5);
    assert!(!flag_value.get());
    assert_eq!(int3.get(), 5);
}

#[test]
fn parse_command_array_and_string_match_upstream_short_long_and_positional_cases() {
    let help = Rc::new(Cell::new(false));
    let version = Rc::new(Cell::new(0));
    let int_value = Rc::new(Cell::new(0));
    let other = Rc::new(Cell::new(0));
    let files = Rc::new(RefCell::new(Vec::new()));
    let negated = Rc::new(Cell::new(false));
    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let help_target = help.clone();
        let version_target = version.clone();
        let int_target = int_value.clone();
        let negated_target = negated.clone();
        let files_target = files.clone();
        let other_target = other.clone();
        let mut init = group.add_options();
        init.add(
            "-h,help",
            action_default::<bool, _>(move |value| help_target.set(value)).flag(),
            "",
        )
        .unwrap()
        .add(
            "-V,version",
            action_default::<i32, _>(move |value| version_target.set(value)),
            "",
        )
        .unwrap()
        .add(
            "-i,int",
            action_default::<i32, _>(move |value| int_target.set(value)),
            "",
        )
        .unwrap()
        .add("-x,flag", ValueDesc::default().flag(), "")
        .unwrap()
        .add(
            "-f,foo",
            action_default::<bool, _>(move |value| negated_target.set(value))
                .flag()
                .implicit("1"),
            "",
        )
        .unwrap()
        .add(
            "+,file",
            parse(move |value| {
                files_target.borrow_mut().push(value.to_owned());
                true
            }),
            "",
        )
        .unwrap()
        .add(
            "other",
            action_default::<i32, _>(move |value| other_target.set(value)),
            "",
        )
        .unwrap();
    }
    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    ctx.add(group).unwrap();

    let argv = ["-h", "-V3", "--other", "6"];
    let mut parse = DefaultParseContext::new(&ctx);
    parse_command_array(&mut parse, &argv, None, 0).unwrap();
    assert!(help.get());
    assert_eq!(version.get(), 3);
    assert_eq!(other.get(), 6);

    parse.clear_parsed();
    parse_command_string(&mut parse, "-xfi10", None, COMMAND_LINE_ALLOW_FLAG_VALUE).unwrap();
    assert_eq!(int_value.get(), 10);
    assert!(negated.get());

    parse.clear_parsed();
    let mut positional = |token: &str, out: &mut String| {
        *out = "file".to_owned();
        token == "-" || !token.starts_with('-')
    };
    parse_command_string(
        &mut parse,
        "foo bar \"foo bar\" '\\foo bar' \\\"foo bar\\\" - -i 11 -- --ignored",
        Some(&mut positional),
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    assert_eq!(int_value.get(), 11);
    let files = files.borrow();
    assert!(files.contains(&"foo".to_owned()));
    assert!(files.contains(&"bar".to_owned()));
    assert!(files.contains(&"foo bar".to_owned()));
    assert!(files.contains(&"\\foo bar".to_owned()));
    assert!(files.contains(&"-".to_owned()));
}

#[test]
fn parse_command_string_reports_upstream_error_cases() {
    let flag_value = Rc::new(Cell::new(true));
    let number = Rc::new(Cell::new(0));
    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let flag_target = flag_value.clone();
        let number_target = number.clone();
        let mut init = group.add_options();
        init.add(
            "!,flag",
            action_default::<bool, _>(move |value| flag_target.set(value)).flag(),
            "",
        )
        .unwrap()
        .add(
            "!,value",
            parse(move |value| {
                let mut parsed = number_target.get();
                if negatable_int(value, &mut parsed) {
                    number_target.set(parsed);
                    true
                } else {
                    false
                }
            })
            .arg("<n>")
            .negatable(),
            "",
        )
        .unwrap();
    }
    ctx.add(group).unwrap();
    let mut parse = DefaultParseContext::new(&ctx);

    parse_command_string(
        &mut parse,
        "--flag --no-value",
        None,
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    assert!(flag_value.get());
    assert_eq!(number.get(), 0);

    assert_context_error(
        parse_command_string(
            &mut parse,
            "--no-value=2",
            None,
            COMMAND_LINE_ALLOW_FLAG_VALUE,
        )
        .unwrap_err(),
        ContextErrorType::UnknownOption,
    );
    parse.clear_parsed();
    assert_value_error(
        parse_command_string(
            &mut parse,
            "--no-value --value=2",
            None,
            COMMAND_LINE_ALLOW_FLAG_VALUE,
        )
        .unwrap_err(),
        ValueErrorType::MultipleOccurrences,
    );

    let mut strict_flag = false;
    let mut strict_ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    let mut strict_group = OptionGroup::new("", DescriptionLevel::Default);
    strict_group
        .add_options()
        .add("flag", flag(&mut strict_flag), "")
        .unwrap();
    strict_ctx.add(strict_group).unwrap();
    let mut strict_parse = DefaultParseContext::new(&strict_ctx);
    assert_syntax_error(
        parse_command_string(&mut strict_parse, "--flag=false", None, 0).unwrap_err(),
        SyntaxErrorType::ExtraValue,
    );
}

#[test]
fn parse_cfg_file_collects_multiline_values() {
    let first = Rc::new(Cell::new(0));
    let path = Rc::new(RefCell::new(String::new()));
    let mut ctx = OptionContext::new("cfg", DescriptionLevel::Default);
    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let first_target = first.clone();
        let path_target = path.clone();
        let mut init = group.add_options();
        init.add(
            "int1",
            action_default::<i32, _>(move |value| first_target.set(value)),
            "",
        )
        .unwrap()
        .add(
            "path",
            parse(move |value| {
                *path_target.borrow_mut() = value.to_owned();
                true
            }),
            "",
        )
        .unwrap();
    }
    ctx.add(group).unwrap();
    let mut parse = DefaultParseContext::new(&ctx);
    let mut input = Cursor::new("int1 = 7\npath = foo\n  bar\n\n# comment\n");
    parse_cfg_file(&mut parse, &mut input).unwrap();
    assert_eq!(first.get(), 7);
    assert_eq!(&*path.borrow(), "foo bar");
}

#[test]
fn option_parser_supports_custom_parse_contexts() {
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));

    struct GroupContext<'a, 'g> {
        name: &'static str,
        group: &'g OptionGroup<'a>,
    }

    impl<'a, 'g> ParseContext<'a> for GroupContext<'a, 'g> {
        fn name(&self) -> &str {
            self.name
        }

        fn state(&self, _opt: &ProgramOption<'a>) -> OptState {
            OptState::Open
        }

        fn do_get_option(
            &self,
            name: &str,
            find_type: FindType,
        ) -> Result<rust_clasp::potassco::program_opts::SharedOption<'a>, Error> {
            match find_type {
                FindType::ALIAS => self
                    .group
                    .find_by_alias(name.trim_start_matches('-').chars().next().unwrap())
                    .ok_or_else(|| {
                        Error::from(rust_clasp::potassco::program_opts::ContextError::new(
                            self.name,
                            ContextErrorType::UnknownOption,
                            name,
                            "",
                        ))
                    }),
                _ => self.group.find_by_name(name).ok_or_else(|| {
                    Error::from(rust_clasp::potassco::program_opts::ContextError::new(
                        self.name,
                        ContextErrorType::UnknownOption,
                        name,
                        "",
                    ))
                }),
            }
        }

        fn do_set_value(
            &mut self,
            opt: &rust_clasp::potassco::program_opts::SharedOption<'a>,
            value: &str,
        ) -> Result<bool, Error> {
            Ok(opt.assign(value))
        }

        fn do_finish(&mut self, _error: Option<&Error>) {}
    }

    let mut group = OptionGroup::new("", DescriptionLevel::Default);
    {
        let first_target = first.clone();
        let second_target = second.clone();
        let mut init = group.add_options();
        init.add(
            "int1",
            action_default::<i32, _>(move |value| first_target.set(value)),
            "",
        )
        .unwrap()
        .add(
            "int2",
            action_default::<i32, _>(move |value| second_target.set(value)),
            "",
        )
        .unwrap();
    }

    let mut ctx = GroupContext {
        name: "dummy",
        group: &group,
    };
    parse_command_string(
        &mut ctx,
        "--int1=10 --int2 22",
        None,
        COMMAND_LINE_ALLOW_FLAG_VALUE,
    )
    .unwrap();
    assert_eq!(first.get(), 10);
    assert_eq!(second.get(), 22);
}

#[test]
fn option_output_keeps_formatting_when_sink_writes_fail() {
    #[derive(Default)]
    struct RecordingFormatter {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl rust_clasp::potassco::program_opts::OptionFormatter for RecordingFormatter {
        fn format_context<'a, 'b>(
            &self,
            buffer: &'b mut String,
            _ctx: &OptionContext<'a>,
        ) -> &'b mut String {
            self.calls.borrow_mut().push("context");
            buffer.push_str("ctx");
            buffer
        }

        fn format_group<'a, 'b>(
            &self,
            buffer: &'b mut String,
            _group: &OptionGroup<'a>,
        ) -> &'b mut String {
            self.calls.borrow_mut().push("group");
            buffer.push_str("group");
            buffer
        }

        fn format_option<'a, 'b>(
            &self,
            buffer: &'b mut String,
            _option: &ProgramOption<'a>,
            _max_width: usize,
        ) -> &'b mut String {
            self.calls.borrow_mut().push("option");
            buffer.push_str("option");
            buffer
        }

        fn column_width<'a>(&self, _option: &ProgramOption<'a>) -> usize {
            0
        }
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("expected failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut writer = FailingWriter;
    let mut enabled = false;
    let mut group = OptionGroup::new("Visible", DescriptionLevel::Default);
    group
        .add_options()
        .add("flag", flag(&mut enabled), "desc")
        .unwrap();
    let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
    ctx.add(group).unwrap();

    let calls = Rc::new(RefCell::new(Vec::new()));
    let formatter = RecordingFormatter {
        calls: calls.clone(),
    };
    let mut output =
        rust_clasp::potassco::program_opts::OptionOutputImpl::with_writer(&mut writer, formatter);

    ctx.description(&mut output);

    assert_eq!(&*calls.borrow(), &["context", "group", "option"]);
}

#[test]
fn option_group_set_description_level_assigns_exact_level() {
    let mut group = OptionGroup::new("Base", DescriptionLevel::E1);
    group.set_description_level(DescriptionLevel::Hidden);
    assert_eq!(group.desc_level(), DescriptionLevel::Hidden);

    let mut left = OptionContext::new("ctx", DescriptionLevel::Default);
    left.add(group).unwrap();

    let right_group = OptionGroup::new("Base", DescriptionLevel::Default);
    left.add(right_group).unwrap();
    assert_eq!(
        left.group("Base").unwrap().desc_level(),
        DescriptionLevel::Default
    );
}

#[test]
fn option_output_accepts_custom_append_sinks() {
    #[derive(Default)]
    struct Buffer(String);

    impl AppendSink for Buffer {
        fn append(&mut self, value: &str) {
            self.0.push_str(value);
        }
    }

    let mut buffer = Buffer::default();

    {
        let mut enabled = false;
        let mut group = OptionGroup::new("Visible", DescriptionLevel::Default);
        group
            .add_options()
            .add("flag", flag(&mut enabled), "desc")
            .unwrap();
        let mut ctx = OptionContext::new("ctx", DescriptionLevel::Default);
        ctx.add(group).unwrap();

        let mut output = rust_clasp::potassco::program_opts::OptionPrinter::new(
            rust_clasp::potassco::program_opts::OutputSink::from_append_sink(&mut buffer),
        );

        ctx.description(&mut output);
    }

    assert!(buffer.0.contains("Visible:"));
    assert!(buffer.0.contains("--flag"));
}
