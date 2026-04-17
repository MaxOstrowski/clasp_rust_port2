use libc::SIGALRM;
use rust_clasp::clasp::cli::clasp_output::{
    ColorStyleSpec, OutputSink, interrupted_string, signal_name, write_styled,
};
use rust_clasp::potassco::format::{BasicCharBuffer, Color, Emphasis, TextStyle, TextStyleSpec};

#[test]
fn color_style_defaults_match_upstream_values() {
    let empty = ColorStyleSpec::default();
    assert_eq!(empty.trace(), TextStyle::default());
    assert_eq!(empty.info(), TextStyle::default());
    assert_eq!(empty.note(), TextStyle::default());
    assert_eq!(empty.warn(), TextStyle::default());
    assert_eq!(empty.err(), TextStyle::default());

    let default = ColorStyleSpec::default_colors();
    assert_eq!(
        default.trace(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::None,
            foreground: Some(Color::BrightMagenta),
            background: None,
        })
    );
    assert_eq!(
        default.info(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Green),
            background: None,
        })
    );
    assert_eq!(
        default.note(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::None,
            foreground: Some(Color::BrightYellow),
            background: None,
        })
    );
    assert_eq!(
        default.warn(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::BrightYellow),
            background: None,
        })
    );
    assert_eq!(
        default.err(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Red),
            background: None,
        })
    );
}

#[test]
fn color_style_parser_matches_upstream_cases() {
    assert_eq!(
        ColorStyleSpec::parse("*:").unwrap(),
        ColorStyleSpec::default_colors()
    );
    assert_eq!(
        ColorStyleSpec::parse("").unwrap(),
        ColorStyleSpec::default()
    );

    let rgb = ColorStyleSpec::parse("info=1;31:note=32:warning=02;34").unwrap();
    assert_eq!(
        rgb.info(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Bold,
            foreground: Some(Color::Red),
            background: None,
        })
    );
    assert_eq!(
        rgb.note(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::None,
            foreground: Some(Color::Green),
            background: None,
        })
    );
    assert_eq!(
        rgb.warn(),
        TextStyle::new(TextStyleSpec {
            emphasis: Emphasis::Faint,
            foreground: Some(Color::Blue),
            background: None,
        })
    );
    assert_eq!(rgb.trace(), TextStyle::default());
    assert_eq!(rgb.err(), TextStyle::default());
}

#[test]
fn color_style_parser_reports_upstream_error_categories() {
    let error = ColorStyleSpec::parse("bla").unwrap_err();
    assert_eq!(error.to_string(), "unknown color key 'bla'");

    let error = ColorStyleSpec::parse("info=10").unwrap_err();
    assert_eq!(error.to_string(), "invalid emphasis in 'info=10'");

    let error = ColorStyleSpec::parse("info=1;2").unwrap_err();
    assert_eq!(error.to_string(), "duplicate emphasis in 'info=1;2'");
}

#[test]
fn output_sink_writes_to_string_and_char_buffer() {
    let mut string = String::new();
    {
        let mut sink = OutputSink::from(&mut string);
        assert_eq!(sink.write("alpha"), 5);
        sink.flush();
    }
    assert_eq!(string, "alpha");

    let mut buffer = BasicCharBuffer::default();
    {
        let mut sink = OutputSink::from(&mut buffer);
        assert_eq!(sink.write("beta"), 4);
        sink.flush();
    }
    assert_eq!(buffer.view(), "beta");
}

#[test]
fn write_styled_wraps_text_with_ansi_sequences() {
    let mut out = String::new();
    let style = ColorStyleSpec::default_colors().warn();
    write_styled(&mut out, &style, "solver version 1.0");
    assert_eq!(
        out,
        format!("{}solver version 1.0{}", style.view(), style.reset_view())
    );
}

#[test]
fn source_signal_names_match_upstream_table() {
    assert_eq!(signal_name(0), None);
    assert_eq!(signal_name(1), Some("SIGHUP"));
    assert_eq!(signal_name(2), Some("SIGINT"));
    assert_eq!(signal_name(8), None);
    assert_eq!(signal_name(14), Some("SIGALRM"));
    assert_eq!(signal_name(18), None);
    assert_eq!(signal_name(19), None);
}

#[test]
fn interruption_strings_distinguish_alarm_from_other_signals() {
    assert_eq!(interrupted_string(SIGALRM), "TIME LIMIT");
    assert_eq!(interrupted_string(2), "INTERRUPTED");
    assert_eq!(interrupted_string(0), "INTERRUPTED");
}
