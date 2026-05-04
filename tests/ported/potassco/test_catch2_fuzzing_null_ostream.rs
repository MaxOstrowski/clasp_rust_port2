use std::io::Write;

use rust_clasp::potassco::catch2_fuzzing_null_ostream::{NullOStream, NullStreambuf};

#[test]
fn overflow_resets_put_area_and_returns_expected_value() {
    let mut buffer = NullStreambuf::default();

    assert_eq!(buffer.overflow(None), b'\0');
    assert_eq!(buffer.available_put_area(), 64);

    assert_eq!(buffer.overflow(Some(b'x')), b'x');
    assert_eq!(buffer.available_put_area(), 64);
}

#[test]
fn constructor_creates_a_sink_stream() {
    let mut stream = NullOStream::new();

    assert_eq!(stream.write(b"discarded").unwrap(), 9);
    stream.write_all(b"more discarded").unwrap();
}

#[test]
fn rdbuf_exposes_the_underlying_null_stream_buffer() {
    let mut stream = NullOStream::new();

    let buffer = stream.rdbuf();
    assert_eq!(buffer.overflow(Some(b'y')), b'y');
    assert_eq!(buffer.available_put_area(), 64);
}

#[test]
fn avoid_out_of_line_virtual_compiler_warning_is_a_noop() {
    let mut stream = NullOStream::new();

    stream.avoid_out_of_line_virtual_compiler_warning();
    assert_eq!(stream.write(b"still discarded").unwrap(), 15);
}
