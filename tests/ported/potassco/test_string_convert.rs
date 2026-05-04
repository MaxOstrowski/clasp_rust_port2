//! Port target for original_clasp/libpotassco/tests/test_string_convert.cpp.

use rust_clasp::clasp::cli::clasp_app::PreFormat;
use rust_clasp::clasp::cli::clasp_cli_options::{asp_logic_program, context_params, solve_options};
use rust_clasp::clasp::cli::clasp_options::{ConfigKey, OffType};
use rust_clasp::clasp::solver_strategies::{OptParams, SatPreParams};
use rust_clasp::potassco::program_opts::{
    Errc, ParseChars, extract, from_chars, from_chars_str_ref, string_convert::parse,
    string_to_errc,
};

fn string_cast<T>(input: &str) -> Option<T>
where
    T: ParseChars + Default,
{
    let mut out = T::default();
    (string_to_errc(input, &mut out) == Errc::Success).then_some(out)
}

#[test]
fn integers_require_digits_and_report_overflow() {
    let mut signed = 0i64;
    let mut unsigned = 0u64;

    assert_eq!(string_to_errc("", &mut signed), Errc::InvalidArgument);
    assert_eq!(string_to_errc("", &mut unsigned), Errc::InvalidArgument);
    assert_eq!(from_chars("", &mut signed).ec, Errc::InvalidArgument);
    assert_eq!(from_chars("", &mut unsigned).ec, Errc::InvalidArgument);

    assert_eq!(string_to_errc("+", &mut signed), Errc::InvalidArgument);
    assert_eq!(string_to_errc("+", &mut unsigned), Errc::InvalidArgument);
    assert_eq!(from_chars("+", &mut signed).ec, Errc::InvalidArgument);
    assert_eq!(from_chars("+", &mut unsigned).ec, Errc::InvalidArgument);

    let mut view_value = 0i32;
    assert_eq!(string_to_errc("12", &mut view_value), Errc::Success);
    assert_eq!(view_value, 12);

    assert_eq!(
        string_to_errc("18446744073709551616", &mut signed),
        Errc::ResultOutOfRange
    );
    assert_eq!(
        string_to_errc("18446744073709551616", &mut unsigned),
        Errc::ResultOutOfRange
    );
    assert_eq!(
        from_chars("18446744073709551616", &mut signed).ec,
        Errc::ResultOutOfRange
    );
    assert_eq!(
        from_chars("18446744073709551616", &mut unsigned).ec,
        Errc::ResultOutOfRange
    );
}

#[test]
fn named_integer_limits_and_unsigned_minus_one_match_upstream() {
    assert_eq!(string_cast::<u32>("umax"), Some(u32::MAX));
    assert_eq!(string_cast::<u64>("umax"), Some(u64::MAX));
    assert_eq!(string_cast::<u64>("imax"), Some(u64::MAX >> 1));
    assert_eq!(string_cast::<u32>("-1"), Some(u32::MAX));
    assert_eq!(string_cast::<u64>("-1"), Some(u64::MAX));
    assert_eq!(string_to_errc("-2", &mut 0u64), Errc::InvalidArgument);

    assert_eq!(string_cast::<i32>("imax"), Some(i32::MAX));
    assert_eq!(string_cast::<i32>("imin"), Some(i32::MIN));
    assert_eq!(string_cast::<i64>("imax"), Some(i64::MAX));
    assert_eq!(string_cast::<i64>("imin"), Some(i64::MIN));
    assert_eq!(string_cast::<i32>("umax"), None);
}

#[test]
fn booleans_and_chars_follow_upstream_prefix_rules() {
    assert_eq!(string_cast::<bool>("1"), Some(true));
    assert_eq!(string_cast::<bool>("true"), Some(true));
    assert_eq!(string_cast::<bool>("on"), Some(true));
    assert_eq!(string_cast::<bool>("yes"), Some(true));
    assert_eq!(string_cast::<bool>("0"), Some(false));
    assert_eq!(string_cast::<bool>("false"), Some(false));
    assert_eq!(string_cast::<bool>("off"), Some(false));
    assert_eq!(string_cast::<bool>("no"), Some(false));
    assert_eq!(string_cast::<bool>("TRUE"), None);

    assert_eq!(string_cast::<char>("\\t"), Some('\t'));
    assert_eq!(string_cast::<char>("\\r"), Some('\r'));
    assert_eq!(string_cast::<char>("\\n"), Some('\n'));
    assert_eq!(string_cast::<char>("\\v"), Some('\u{000b}'));
    assert_eq!(string_cast::<char>("\\f"), Some('\u{000c}'));
    assert_eq!(string_cast::<char>("x"), Some('x'));
    assert_eq!(
        string_cast::<char>("49").map(|value| value as u32),
        Some(49)
    );
    assert_eq!(
        string_cast::<char>("127").map(|value| value as u32),
        Some(127)
    );
    assert_eq!(string_cast::<char>("128"), None);
    assert_eq!(string_cast::<char>("256"), None);
    assert_eq!(string_cast::<char>("\\a"), None);
}

#[test]
fn strings_append_and_string_views_borrow_the_full_input() {
    let mut text = String::from("Hello ");
    let result = from_chars("World", &mut text);
    assert_eq!(result.ec, Errc::Success);
    assert_eq!(result.ptr, 5);
    assert_eq!(text, "Hello World");

    let mut borrowed = "";
    let result = from_chars_str_ref("123", &mut borrowed);
    assert_eq!(result.ec, Errc::Success);
    assert_eq!(result.ptr, 3);
    assert_eq!(borrowed, "123");
}

#[test]
fn pairs_can_be_parsed_incrementally_and_nested() {
    let mut value = (0i32, false);
    assert_eq!(string_to_errc("10,false", &mut value), Errc::Success);
    assert_eq!(value, (10, false));

    let mut ints = (0i32, 0i32);
    assert_eq!(string_to_errc("(1,2)", &mut ints), Errc::Success);
    assert_eq!(ints, (1, 2));
    assert_eq!(string_to_errc("7", &mut ints), Errc::Success);
    assert_eq!(ints, (7, 2));
    assert_ne!(string_to_errc("9,", &mut ints), Errc::Success);
    assert_eq!(ints, (7, 2));

    let mut nested = ((0i32, 0i32), (0i32, 0i32));
    assert_eq!(string_to_errc("((1,2),(3,4))", &mut nested), Errc::Success);
    assert_eq!(nested, ((1, 2), (3, 4)));
    assert_eq!(string_to_errc("3,4,5,6", &mut nested), Errc::Success);
    assert_eq!(nested, ((3, 4), (5, 6)));
    assert_eq!(string_to_errc("99", &mut nested), Errc::Success);
    assert_eq!(nested, ((99, 4), (5, 6)));
}

#[test]
fn floating_point_parsing_is_partial_and_range_checked() {
    let mut value = 0.0f64;
    let result = from_chars("1233.22foo", &mut value);
    assert_eq!(value, 1233.22);
    assert_eq!(result.ec, Errc::Success);
    assert_eq!(result.remaining("1233.22foo"), "foo");

    let result = from_chars("1233.22,foo", &mut value);
    assert_eq!(value, 1233.22);
    assert_eq!(result.remaining("1233.22,foo"), ",foo");

    let result = from_chars("1Eblub", &mut value);
    assert_eq!(value, 1.0);
    assert_eq!(result.remaining("1Eblub"), "Eblub");

    assert_eq!(string_cast::<f64>("0"), Some(0.0));
    assert_eq!(string_cast::<f64>("0.000"), Some(0.0));
    assert_eq!(string_cast::<f64>("-12.32"), Some(-12.32));
    assert_eq!(string_to_errc("1e40", &mut 0f32), Errc::ResultOutOfRange);
}

#[test]
fn vectors_can_be_parsed_and_nested() {
    let mut values = Vec::<i32>::new();
    assert_eq!(string_to_errc("[1,2,3,4]", &mut values), Errc::Success);
    assert_eq!(values, vec![1, 2, 3, 4]);

    values.clear();
    assert_eq!(string_to_errc("1,2,3", &mut values), Errc::Success);
    assert_eq!(values, vec![1, 2, 3]);
    assert_ne!(string_to_errc("1,2,", &mut values), Errc::Success);

    let mut nested = Vec::<Vec<i32>>::new();
    assert_eq!(string_to_errc("[[1,2],[3,4]]", &mut nested), Errc::Success);
    assert_eq!(nested, vec![vec![1, 2], vec![3, 4]]);
}

#[test]
fn variadic_to_string_macro_matches_upstream_sequence_helper() {
    assert_eq!(rust_clasp::potassco_to_string!(1, 2, 3), "1,2,3");
    assert_eq!(rust_clasp::potassco_to_string!(1, "Hallo"), "1,Hallo");
    assert_eq!(rust_clasp::potassco_to_string!(vec![1, 2, 3]), "1,2,3");
}

#[test]
fn extract_and_case_insensitive_helpers_match_upstream() {
    let mut input = "1,off";
    let mut left = false;
    let mut right = true;
    assert_eq!(extract(&mut input, &mut left), Errc::Success);
    assert!(left);
    assert_eq!(input, ",off");
    input = &input[1..];
    assert_eq!(extract(&mut input, &mut right), Errc::Success);
    assert!(!right);
    assert_eq!(input, "");

    assert!(parse::eq_ignore_case("", ""));
    assert!(!parse::eq_ignore_case("", "H"));
    assert!(!parse::eq_ignore_case("H", ""));
    assert!(parse::eq_ignore_case("H", "H"));
    assert!(parse::eq_ignore_case("h", "H"));
    assert!(parse::eq_ignore_case("haLlO", "HALLO"));
    assert!(!parse::eq_ignore_case("haLlO_", "HALLO"));

    assert!(!parse::eq_ignore_case_n("", "", 1));
    assert!(parse::eq_ignore_case_n("", "", 0));
    assert!(!parse::eq_ignore_case_n("", "H", 1));
    assert!(parse::eq_ignore_case_n("H", "H", 1));
    assert!(!parse::eq_ignore_case_n("H", "H", 2));
    assert!(parse::eq_ignore_case_n("haL", "HALx", 3));
}

#[test]
fn clasp_specializations_use_string_convert_adapters() {
    let mut config = ConfigKey::Default;
    assert_eq!(string_to_errc("trendy", &mut config), Errc::Success);
    assert_eq!(config, ConfigKey::Trendy);

    let mut pre = PreFormat::default();
    let mut view = "smodels,rest";
    assert_eq!(extract(&mut view, &mut pre), Errc::Success);
    assert_eq!(pre, PreFormat::Smodels);
    assert_eq!(view, ",rest");

    let mut off = OffType;
    assert_eq!(string_to_errc("no", &mut off), Errc::Success);
    assert_eq!(string_to_errc("true", &mut off), Errc::InvalidArgument);

    let mut sat = SatPreParams::default();
    assert_eq!(
        string_to_errc("2,iter=10,frozen=4", &mut sat),
        Errc::Success
    );
    assert_eq!(sat.type_, 2);
    assert_eq!(sat.lim_iters, 10);
    assert_eq!(sat.lim_frozen, 4);

    let mut opt = OptParams::default();
    assert_eq!(
        string_to_errc("usc,pmres,disjoint", &mut opt),
        Errc::Success
    );
    assert_eq!(opt.type_, 1);

    let mut share = context_params::ShareMode::ShareNo;
    assert_eq!(string_to_errc("all", &mut share), Errc::Success);
    assert_eq!(share, context_params::ShareMode::ShareAll);

    let mut extended = asp_logic_program::ExtendedRuleMode::ModeNative;
    assert_eq!(string_to_errc("dynamic", &mut extended), Errc::Success);
    assert_eq!(
        extended,
        asp_logic_program::ExtendedRuleMode::ModeTransformDynamic
    );

    let mut enum_type = solve_options::EnumType::EnumBt;
    assert_eq!(string_to_errc("auto", &mut enum_type), Errc::Success);
    assert_eq!(enum_type, solve_options::EnumType::EnumAuto);
}
