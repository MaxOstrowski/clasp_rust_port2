use std::io::Cursor;
use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::basic_types::{
    ATOM_MAX, AbstractProgram, AtomSpan, BodyType, DomModifier, HeadType, LitSpan, TruthValue,
    Weight, WeightLit, WeightLitSpan,
};
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco::match_basic_types::{
    BufferedStream, ProgramReader, ProgramReaderCore, ProgramReaderHooks, ReadMode, is_digit,
    match_num, match_term, read_program, to_digit,
};

struct ReaderHooks {
    incremental: bool,
    consume_all: bool,
    steps: Vec<String>,
    reset_count: usize,
}

impl ReaderHooks {
    fn new(incremental: bool, consume_all: bool) -> Self {
        Self {
            incremental,
            consume_all,
            steps: Vec::new(),
            reset_count: 0,
        }
    }
}

impl ProgramReaderHooks for ReaderHooks {
    fn do_attach(&mut self, _reader: &mut ProgramReaderCore, incremental: &mut bool) -> bool {
        *incremental = self.incremental;
        true
    }

    fn do_parse(&mut self, reader: &mut ProgramReaderCore) -> bool {
        reader.skip_ws();
        let mut step = String::new();
        while reader.peek() != '\0' {
            let next = reader.get();
            if next == ';' && !self.consume_all {
                break;
            }
            step.push(next);
        }
        self.steps.push(step.trim().to_owned());
        true
    }

    fn do_reset(&mut self, _reader: &mut ProgramReaderCore) {
        self.reset_count += 1;
    }
}

#[derive(Default)]
struct NullProgram;

impl AbstractProgram for NullProgram {
    fn rule(&mut self, _head_type: HeadType, _head: AtomSpan<'_>, _body: LitSpan<'_>) {}

    fn rule_weighted(
        &mut self,
        _head_type: HeadType,
        _head: AtomSpan<'_>,
        _bound: Weight,
        _body: WeightLitSpan<'_>,
    ) {
    }

    fn minimize(&mut self, _priority: Weight, _lits: WeightLitSpan<'_>) {}

    fn output_atom(&mut self, _atom: u32, _name: &str) {}
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

#[test]
fn buffered_stream_matches_upstream_cases() {
    let mut stream = BufferedStream::new(Cursor::new(b"Foo"));
    let mut out = [b'x'; 10];
    let read = stream.read(&mut out);
    assert_eq!(read, 3);
    assert_eq!(&out[..read], b"Foo");

    let mut stream = BufferedStream::new(Cursor::new(b"Hello World!"));
    assert!(stream.r#match("Hello"));
    assert_eq!(stream.get(), ' ');
    assert!(!stream.r#match("World!!"));
    assert!(stream.r#match("World!"));
    assert_eq!(stream.peek(), '\0');

    let long = "x".repeat(4050) + &"y".repeat(200) + &"z".repeat(100);
    let mut stream = BufferedStream::new(Cursor::new(long.as_bytes()));
    let mut prefix = vec![b'-'; 4020];
    assert_eq!(stream.read(&mut prefix), prefix.len());
    assert_eq!(prefix, vec![b'x'; 4020]);
    assert!(stream.r#match(&("x".repeat(30) + &"y".repeat(198))));
    assert!(stream.r#match(&("y".repeat(2) + &"z".repeat(95))));
    let mut suffix = [b'-'; 20];
    let count = stream.read(&mut suffix);
    assert_eq!(count, 5);
    assert_eq!(&suffix[..count], b"zzzzz");
    assert_eq!(stream.peek(), '\0');

    let mut stream = BufferedStream::new(Cursor::new(b"Foo"));
    assert!(!stream.unget('x'));
    assert_eq!(stream.get(), 'F');
    assert!(stream.unget('x'));
    assert_eq!(stream.get(), 'x');
    assert_eq!(stream.get(), 'o');
    assert!(stream.unget('h'));
    assert!(stream.unget('W'));
    assert!(stream.r#match("Who"));
    assert_eq!(stream.peek(), '\0');
    assert!(stream.unget('!'));
    assert_eq!(stream.peek(), '!');
}

#[test]
fn buffered_stream_normalizes_newlines_and_tracks_lines() {
    let mut stream = BufferedStream::new(Cursor::new(b"a\r\nb\rc\n"));
    assert_eq!(stream.line(), 1);
    assert_eq!(stream.get(), 'a');
    assert_eq!(stream.get(), '\n');
    assert_eq!(stream.line(), 2);
    assert_eq!(stream.get(), 'b');
    assert_eq!(stream.get(), '\n');
    assert_eq!(stream.line(), 3);
    assert_eq!(stream.get(), 'c');
    assert_eq!(stream.get(), '\n');
    assert_eq!(stream.line(), 4);
}

#[test]
fn term_and_number_matching_follow_upstream_behavior() {
    let mut input = "tuple((1,2),\"(3,4)\"),rest";
    assert_eq!(match_term(&mut input), Some("tuple((1,2),\"(3,4)\")"));
    assert_eq!(input, ",rest");

    let mut input = "-12x";
    let mut matched = "";
    let mut value = 0;
    assert!(match_num(&mut input, Some(&mut matched), Some(&mut value)));
    assert_eq!(matched, "-12");
    assert_eq!(value, -12);
    assert_eq!(input, "x");

    let mut input = "2147483648";
    assert!(!match_num(&mut input, None, None));

    assert!(is_digit('7'));
    assert!(!is_digit('x'));
    assert_eq!(to_digit('7'), 7);
}

#[test]
fn program_reader_supports_complete_and_incremental_modes() {
    let mut reader = ProgramReader::new(ReaderHooks::new(false, true));
    assert!(reader.accept(Cursor::new(b" alpha beta ")));
    assert!(reader.parse(ReadMode::Complete));
    assert_eq!(reader.hooks().steps, ["alpha beta"]);
    assert!(!reader.incremental());
    assert!(!reader.more());

    let mut reader = ProgramReader::new(ReaderHooks::new(true, false));
    assert!(reader.accept(Cursor::new(b"one; two; three")));
    assert!(reader.incremental());
    assert!(reader.parse(ReadMode::Incremental));
    assert_eq!(reader.hooks().steps, ["one"]);
    assert!(reader.more());
    assert!(reader.parse(ReadMode::Complete));
    assert_eq!(reader.hooks().steps, ["one", "two", "three"]);
    assert!(!reader.more());
}

#[test]
fn program_reader_core_matches_numbers_and_reports_extra_input() {
    let mut reader = ProgramReader::new(ReaderHooks::new(false, true));
    assert!(reader.accept(Cursor::new(b"7 0 3 -2 4 -5 9")));
    let core = reader.core_mut();
    assert_eq!(core.match_atom("atom expected"), 7);
    assert_eq!(core.match_atom_or_zero("atom or zero expected"), 0);
    assert_eq!(core.match_id("id expected"), 3);
    assert_eq!(core.match_lit("literal expected"), -2);
    assert_eq!(core.match_weight(true, "weight expected"), 4);
    assert_eq!(
        core.match_wlit(false, "weight literal expected"),
        WeightLit { lit: -5, weight: 9 }
    );

    let mut reader = ProgramReader::new(ReaderHooks::new(false, true));
    assert!(reader.accept(Cursor::new(b"2")));
    assert_eq!(
        reader
            .core_mut()
            .match_enum::<BodyType>("body type expected"),
        BodyType::Count
    );

    let mut reader = ProgramReader::new(ReaderHooks::new(false, false));
    assert!(reader.accept(Cursor::new(b"one; two")));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = reader.parse(ReadMode::Complete);
    }))
    .expect_err("expected invalid extra input to panic");
    let message = panic_message(panic);
    assert!(message.contains("invalid extra input"));

    let mut reader = ProgramReader::new(ReaderHooks::new(false, true));
    let code = read_program(Cursor::new(b"program"), &mut reader);
    assert_eq!(code, 0);
    assert_eq!(reader.hooks().steps, ["program"]);
}

#[test]
fn program_reader_reset_and_var_limit_follow_the_core_contract() {
    let mut reader = ProgramReader::new(ReaderHooks::new(false, true));
    assert!(reader.accept(Cursor::new(b"1")));
    reader.core_mut().set_max_var(ATOM_MAX - 1);
    assert_eq!(reader.core_mut().match_atom("atom expected"), 1);
    reader.reset();
    assert_eq!(reader.line(), 1);
    assert_eq!(reader.hooks().reset_count, 2);
}

#[test]
fn abstract_program_unsupported_operations_raise_domain_errors() {
    let mut program = NullProgram;

    assert_eq!(
        catch_error(|| program.project(&[1, 2])),
        Error::DomainError("projection not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.output_term(0, "term")),
        Error::DomainError("output term not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.output(0, &[1, -2])),
        Error::DomainError("output term not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.external(7, TruthValue::True)),
        Error::DomainError("externals not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.assume(&[1, -2])),
        Error::DomainError("assumptions not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.heuristic(7, DomModifier::Level, -3, 2, &[1])),
        Error::DomainError("heuristic directive not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.acyc_edge(1, 2, &[3])),
        Error::DomainError("edge directive not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_term_number(0, 7)),
        Error::DomainError("theory data not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_term_symbol(0, "sym")),
        Error::DomainError("theory data not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_term_compound(0, 1, &[2, 3])),
        Error::DomainError("theory data not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_element(0, &[1, 2], &[3])),
        Error::DomainError("theory data not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_atom(0, 1, &[2, 3])),
        Error::DomainError("theory data not supported".to_owned())
    );
    assert_eq!(
        catch_error(|| program.theory_atom_guarded(0, 1, &[2, 3], 4, 5)),
        Error::DomainError("theory data not supported".to_owned())
    );
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    "non-string panic payload".to_owned()
}
