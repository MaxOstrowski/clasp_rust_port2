use libc::SIGALRM;
use rust_clasp::clasp::cli::clasp_output::{
    ColorStyleSpec, OutputSink, OutputSinkInitError, interrupted_string, signal_name, write_styled,
};
use rust_clasp::potassco::format::{BasicCharBuffer, Color, Emphasis, TextStyle, TextStyleSpec};
use rust_clasp::potassco::platform::CFile;

unsafe extern "C" {
    fn fclose(file: *mut CFile) -> i32;
    fn fread(ptr: *mut core::ffi::c_void, size: usize, count: usize, stream: *mut CFile) -> usize;
    fn rewind(stream: *mut CFile);
    fn tmpfile() -> *mut CFile;
}

fn make_temp_file() -> *mut CFile {
    let file = unsafe { tmpfile() };
    assert!(!file.is_null());
    file
}

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
        ColorStyleSpec::new("*:").unwrap(),
        ColorStyleSpec::default_colors()
    );
    assert_eq!(ColorStyleSpec::new("").unwrap(), ColorStyleSpec::default());

    let rgb = ColorStyleSpec::new("info=1;31:note=32:warning=02;34").unwrap();
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
    let error = ColorStyleSpec::new("bla").unwrap_err();
    assert_eq!(error.to_string(), "unknown color key 'bla'");

    let error = ColorStyleSpec::new("info=10").unwrap_err();
    assert_eq!(error.to_string(), "invalid emphasis in 'info=10'");

    let error = ColorStyleSpec::new("info=1;2").unwrap_err();
    assert_eq!(error.to_string(), "duplicate emphasis in 'info=1;2'");
}

#[test]
fn output_sink_writes_to_file_string_and_char_buffer() {
    let file = make_temp_file();
    {
        let mut sink = OutputSink::from_c_file(file).unwrap();
        assert_eq!(sink.file(), file);
        assert_eq!(sink.write("gamma"), 5);
        sink.flush();
    }
    let mut bytes = [0_u8; 16];
    unsafe {
        rewind(file);
    }
    let count = unsafe { fread(bytes.as_mut_ptr().cast(), 1, bytes.len(), file) };
    assert_eq!(&bytes[..count], b"gamma");
    assert_eq!(unsafe { fclose(file) }, 0);

    let mut writer_bytes = Vec::new();
    {
        let writer: &mut dyn std::io::Write = &mut writer_bytes;
        let mut sink = OutputSink::from(writer);
        assert!(sink.file().is_null());
        assert_eq!(sink.write("delta"), 5);
        sink.flush();
    }
    assert_eq!(writer_bytes, b"delta");

    let mut string = String::new();
    {
        let mut sink = OutputSink::from(&mut string);
        assert!(sink.file().is_null());
        assert_eq!(sink.write("alpha"), 5);
        sink.flush();
    }
    assert_eq!(string, "alpha");

    let mut buffer = BasicCharBuffer::default();
    {
        let mut sink = OutputSink::from(&mut buffer);
        assert!(sink.file().is_null());
        assert_eq!(sink.write("beta"), 4);
        sink.flush();
    }
    assert_eq!(buffer.view(), "beta");
}

#[test]
fn output_sink_rejects_null_file_pointer() {
    match OutputSink::from_c_file(std::ptr::null_mut()) {
        Err(error) => assert_eq!(error, OutputSinkInitError),
        Ok(_) => panic!("expected invalid output sink error"),
    }
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
