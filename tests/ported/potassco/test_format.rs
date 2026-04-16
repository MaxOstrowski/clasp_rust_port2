use rust_clasp::potassco::format::{
    BasicCharBuffer, Color, Emphasis, TextStyle, TextStyleSpec, float_field, int_field, keyed,
    quoted, str_field, styled, to_string, uint_field,
};
use sprintf::Printf;

#[test]
fn tuple_and_vector_to_string_match_upstream_sequence_cases() {
    assert_eq!(to_string(&(10, false)), "10,false");
    assert_eq!(to_string(&vec![1, 2, 3]), "1,2,3");
}

#[test]
fn field_rendering_matches_expected_padding_and_precision() {
    assert_eq!(to_string(&int_field(42, -4, None)), "42  ");
    assert_eq!(to_string(&uint_field(4711u32, 8, Some('s'))), "   4711s");
    assert_eq!(to_string(&uint_field(7u32, -3, Some('%'))), "7% ");
    assert_eq!(to_string(&float_field(0.12345, 0, Some(3), None)), "0.123");
    assert_eq!(to_string(&float_field(0.12345, 6, Some(3), None)), " 0.123");
    assert_eq!(
        to_string(&float_field(0.12345, -7, Some(2), Some('s'))),
        "0.12s  "
    );
    assert_eq!(to_string(&str_field("4711", 0)), "4711");
    assert_eq!(to_string(&str_field("4711", -8)), "4711    ");
    assert_eq!(to_string(&str_field("4711", 6)), "  4711");
}

#[test]
fn quoted_keyed_and_styled_wrappers_preserve_text_order() {
    assert_eq!(to_string(&quoted("Hallo", "\"")), "\"Hallo\"");
    assert_eq!(to_string(&quoted("Hallo", "'")), "'Hallo'");
    assert_eq!(to_string(&quoted(42, "'")), "'42'");
    assert_eq!(
        to_string(&keyed("Hello", float_field(0.12345, 0, Some(3), None))),
        "Hello: 0.123"
    );
    assert_eq!(
        to_string(&keyed("Foo", quoted("Bar", "\""))),
        "Foo: \"Bar\""
    );
    assert_eq!(to_string(&keyed("", 23)), "23");

    let bold = TextStyle::new(TextStyleSpec {
        emphasis: Emphasis::Bold,
        foreground: None,
        background: None,
    });
    assert_eq!(to_string(&styled("Hallo", bold)), "\u{1b}[1mHallo\u{1b}[0m");

    let red_italic = TextStyle::new(TextStyleSpec {
        emphasis: Emphasis::Italic,
        foreground: Some(Color::Red),
        background: None,
    });
    assert_eq!(
        to_string(&styled(quoted("Hallo", "'"), red_italic)),
        "\u{1b}[3;31m'Hallo'\u{1b}[0m"
    );
}

#[test]
fn text_style_building_and_parsing_match_upstream_cases() {
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
            emphasis: Emphasis::Italic,
            foreground: Some(Color::BrightCyan),
            background: Some(Color::Black),
        })
        .view(),
        "\u{1b}[3;96;40m"
    );
    assert_eq!(
        TextStyle::from_string("01;31", 0).unwrap().view(),
        "\u{1b}[1;31m"
    );
    assert_eq!(
        TextStyle::from_string("32", 0).unwrap().view(),
        "\u{1b}[0;32m"
    );
    assert_eq!(
        TextStyle::from_string("4;96;46", 0).unwrap().view(),
        "\u{1b}[4;96;46m"
    );
    assert_eq!(
        TextStyle::from_string("warning=1;35", 8).unwrap().view(),
        "\u{1b}[1;35m"
    );
    assert!(TextStyle::from_string("7;31", 0).is_err());
    assert!(TextStyle::from_string("50", 0).is_err());
    assert!(TextStyle::from_string("1;", 0).is_err());
}

#[test]
fn basic_char_buffer_supports_open_append_and_close() {
    let mut buffer = BasicCharBuffer::default();
    let red = TextStyle::new(TextStyleSpec {
        emphasis: Emphasis::None,
        foreground: Some(Color::Red),
        background: None,
    });
    buffer.open(red.clone(), Some('\n'));
    buffer.append("Hello");
    assert_eq!(buffer.close(), "\u{1b}[0;31mHello\u{1b}[0m\n");

    buffer.clear();
    let green = TextStyle::new(TextStyleSpec {
        emphasis: Emphasis::None,
        foreground: Some(Color::Green),
        background: None,
    });
    buffer.open(green, None);
    buffer.append("World");
    assert_eq!(buffer.close(), "\u{1b}[0;32mWorld\u{1b}[0m");

    buffer.clear();
    buffer.open(TextStyle::default(), Some(';'));
    buffer.append("World");
    assert_eq!(buffer.close(), "World;");

    buffer.clear();
    buffer
        .open(red, Some(' '))
        .append("Hello")
        .open(TextStyle::default(), Some('!'))
        .append("World");
    assert_eq!(buffer.close(), "\u{1b}[0;31mHello\u{1b}[0m World!");
}

#[test]
fn basic_char_buffer_append_sep_and_repeat_skip_empty_values() {
    let mut buffer = BasicCharBuffer::default();

    let spaced = [Some(1), None, Some(2)];
    buffer.append_sep("<>", &spaced);
    assert_eq!(buffer.view(), "1<>2");

    buffer.clear();
    let all_empty: [Option<i32>; 2] = [None, None];
    buffer.append_sep("<>", &all_empty);
    assert!(buffer.view().is_empty());

    buffer.clear();
    buffer.append("(");
    buffer.append_repeat(4, 'x');
    buffer.append(")");
    assert_eq!(buffer.view(), "(xxxx)");
}

#[test]
fn basic_char_buffer_append_f_matches_upstream_cases() {
    let mut buffer = BasicCharBuffer::default();
    buffer.append_f("Hello", &[]);
    assert_eq!(buffer.view(), "Hello");

    buffer.clear();
    let string_args: [&dyn Printf; 1] = [&"World"];
    buffer.append_f("Hello %s", &string_args);
    assert_eq!(buffer.view(), "Hello World");

    buffer.clear();
    let mixed_args: [&dyn Printf; 2] = [&22u32, &3.1f64];
    buffer.append_f("Hello %08u|%gs", &mixed_args);
    assert_eq!(buffer.view(), "Hello 00000022|3.1s");

    buffer.clear();
    let empty_string_args: [&dyn Printf; 1] = [&""];
    buffer.append_f("Hello %130sfoo", &empty_string_args);
    let mut expected = String::from("Hello ");
    expected.push_str(&" ".repeat(130));
    expected.push_str("foo");
    assert_eq!(buffer.view(), expected);
}
