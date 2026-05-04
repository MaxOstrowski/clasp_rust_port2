//! Rust port of original_clasp/libpotassco/tests/test_text.cpp.

use std::collections::HashMap;
use std::io::Cursor;
use std::panic::{self, AssertUnwindSafe};

use crate::test_common::ReadObserver;
use rust_clasp::potassco::aspif_text::{AspifTextInput, AspifTextOutput};
use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, DomModifier, HeadType, Id, Lit, LitSpan, TruthValue, Weight, WeightLit,
    WeightLitSpan, atom, lit,
};
use rust_clasp::potassco::enums::enum_name;
use rust_clasp::potassco::match_basic_types::{ProgramReader, ReadMode};
use rust_clasp::potassco::rule_utils::RuleBuilder;
use rust_clasp::potassco::theory_data::{TupleType, parens};

#[derive(Default)]
struct TextObserver {
    base: ReadObserver,
    terms: HashMap<Id, String>,
    output: String,
}

impl TextObserver {
    fn program(&self) -> &str {
        &self.output
    }

    fn write_lits<T>(&self, items: &[T], beg: &str, end: &str, sep: &str) -> String
    where
        T: Copy,
        T: Into<RenderedLit>,
    {
        let mut out = String::from(beg);
        for (index, item) in items.iter().copied().enumerate() {
            if index != 0 {
                out.push_str(sep);
            }
            let rendered = item.into();
            if rendered.lit < 0 {
                out.push_str("not ");
            }
            out.push_str(&format!("x_{}", atom(rendered.lit)));
            if let Some(weight) = rendered.weight {
                out.push_str(&format!("={weight}"));
            }
        }
        out.push_str(end);
        out
    }

    fn write_head(&self, head_type: HeadType, head: &[Atom], has_body: bool) -> String {
        let mut out = if head_type == HeadType::Choice {
            self.write_lits(
                &head.iter().copied().map(lit).collect::<Vec<_>>(),
                "{",
                "}",
                "; ",
            )
        } else {
            self.write_lits(
                &head.iter().copied().map(lit).collect::<Vec<_>>(),
                "",
                "",
                "; ",
            )
        };
        if has_body {
            out.push_str(" :- ");
        }
        out
    }
}

#[derive(Copy, Clone)]
struct RenderedLit {
    lit: Lit,
    weight: Option<Weight>,
}

impl From<Lit> for RenderedLit {
    fn from(value: Lit) -> Self {
        Self {
            lit: value,
            weight: None,
        }
    }
}

impl From<WeightLit> for RenderedLit {
    fn from(value: WeightLit) -> Self {
        Self {
            lit: value.lit,
            weight: Some(value.weight),
        }
    }
}

impl AbstractProgram for TextObserver {
    fn init_program(&mut self, incremental: bool) {
        self.base.init_program(incremental);
    }

    fn begin_step(&mut self) {
        self.base.begin_step();
        self.output.clear();
    }

    fn rule(&mut self, head_type: HeadType, head: &[Atom], body: LitSpan<'_>) {
        let has_body = !body.is_empty() || (head_type == HeadType::Disjunctive && head.is_empty());
        self.output
            .push_str(&self.write_head(head_type, head, has_body));
        self.output
            .push_str(&self.write_lits(body, "", ".\n", "; "));
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: &[Atom],
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        self.output
            .push_str(&self.write_head(head_type, head, true));
        self.output.push_str(&format!("{bound}"));
        self.output
            .push_str(&self.write_lits(body, " {", "}.\n", "; "));
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        self.output
            .push_str(&self.write_lits(lits, "#minimize {", "}", "; "));
        self.output.push_str(&format!("@{priority}.\n"));
    }

    fn output_atom(&mut self, atom_id: Atom, name: &str) {
        self.base.output_atom(atom_id, name);
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        self.terms.insert(term_id, name.to_owned());
    }

    fn output(&mut self, term_id: Id, cond: LitSpan<'_>) {
        let name = self
            .terms
            .get(&term_id)
            .cloned()
            .unwrap_or_else(|| "?".to_owned());
        self.output.push_str(&format!("#term {name}"));
        self.output.push_str(&self.write_lits(
            cond,
            if cond.is_empty() { "" } else { " : " },
            ".",
            "; ",
        ));
        self.output.push_str(&format!(" [{term_id}]\n"));
    }

    fn project(&mut self, atoms: &[Atom]) {
        self.output.push_str(&self.write_lits(
            &atoms.iter().copied().map(lit).collect::<Vec<_>>(),
            "#project {",
            "}.\n",
            "; ",
        ));
    }

    fn external(&mut self, atom_id: Atom, value: TruthValue) {
        self.output
            .push_str(&self.write_lits(&[lit(atom_id)], "#external {", "}.", "; "));
        self.output.push_str(&format!(" [{}]\n", enum_name(value)));
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        self.output
            .push_str(&self.write_lits(lits, "#assume {", "}.\n", "; "));
    }

    fn heuristic(
        &mut self,
        atom_id: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        self.output.push_str(&format!("#heuristic x_{atom_id}"));
        self.output.push_str(&self.write_lits(
            condition,
            if condition.is_empty() { "" } else { " : " },
            ".",
            "; ",
        ));
        self.output
            .push_str(&format!(" [{bias}@{priority}, {}]\n", enum_name(modifier)));
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        self.output.push_str(&format!("#edge ({source},{target})"));
        self.output.push_str(&self.write_lits(
            condition,
            if condition.is_empty() { "" } else { " : " },
            ".\n",
            "; ",
        ));
    }

    fn end_step(&mut self) {}
}

fn render_output(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("valid utf8 output")
}

fn read_text(input: &str, observer: &mut TextObserver) -> bool {
    let mut reader = ProgramReader::new(AspifTextInput::new(observer));
    reader.accept(Cursor::new(input.as_bytes())) && reader.parse(ReadMode::Complete)
}

#[test]
fn text_reader_attach_rejects_invalid_prefix_without_initializing_output() {
    let mut observer = TextObserver::default();
    let mut reader = ProgramReader::new(AspifTextInput::new(&mut observer));

    assert!(!reader.accept(Cursor::new(b"?\n".as_slice())));
    drop(reader);

    assert_eq!(observer.base.n_step, 0);
    assert!(!observer.base.incremental);
    assert_eq!(observer.program(), "");
}

#[test]
fn text_reader_attach_accepts_empty_input_and_parse_runs_single_step() {
    let mut observer = TextObserver::default();
    let mut reader = ProgramReader::new(AspifTextInput::new(&mut observer));

    assert!(reader.accept(Cursor::new(b"".as_slice())));
    assert!(reader.parse(ReadMode::Complete));
    drop(reader);

    assert_eq!(observer.base.n_step, 1);
    assert!(!observer.base.incremental);
    assert_eq!(observer.program(), "");
}

#[test]
fn text_reader_set_output_rebinds_observer() {
    let mut first = TextObserver::default();
    let mut second = TextObserver::default();
    let mut parser = AspifTextInput::new(&mut first);
    parser.set_output(&mut second);
    let mut reader = ProgramReader::new(parser);

    assert!(reader.accept(Cursor::new(b"x1.\n".as_slice())));
    assert!(reader.parse(ReadMode::Complete));
    drop(reader);

    assert_eq!(first.base.n_step, 0);
    assert_eq!(first.program(), "");
    assert_eq!(second.base.n_step, 1);
    assert_eq!(second.program(), "x_1.\n");
}

fn panic_message<F>(func: F) -> String
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(error) = payload.downcast_ref::<rust_clasp::potassco::error::Error>() {
        return error.to_string();
    }
    "non-string panic".to_owned()
}

#[test]
fn text_reader_basic_cases() {
    let cases = [
        ("", "", 1, false),
        (":- .", " :- .\n", 1, false),
        ("x1.", "x_1.\n", 1, false),
        ("x1 :- not   x2.", "x_1 :- not x_2.\n", 1, false),
        (
            "{x1} :- not x2.\n{x2, x3}.",
            "{x_1} :- not x_2.\n{x_2; x_3}.\n",
            1,
            false,
        ),
        ("{}.\n", "{}.\n", 1, false),
        (
            "x1 | x2 :- not x3.\nx1 ; x2 :- not x4.",
            "x_1; x_2 :- not x_3.\nx_1; x_2 :- not x_4.\n",
            1,
            false,
        ),
        (
            "x1 :- 2 {x2, x3=2, not x4 = 3, x5}.",
            "x_1 :- 2 {x_2=1; x_3=2; not x_4=3; x_5=1}.\n",
            1,
            false,
        ),
        ("a :- not b, x_3.", "x_1 :- not x_2; x_3.\n", 1, false),
        (":- x1, not x2.", " :- x_1; not x_2.\n", 1, false),
        (
            "#minimize {x1, x2, x3}.\n#minimize {not x1=2, x4, not x5 = 3}@1.\n",
            "#minimize {x_1=1; x_2=1; x_3=1}@0.\n#minimize {not x_1=2; x_4=1; not x_5=3}@1.\n",
            1,
            false,
        ),
        (
            "#project {a,x2}.#project {}.",
            "#project {x_1; x_2}.\n#project {}.\n",
            1,
            false,
        ),
        (
            "#external x1.\n#external x2. [true]\n#external x3. [false]\n#external x4. [free]\n#external x5. [release]\n",
            "#external {x_1}. [false]\n#external {x_2}. [true]\n#external {x_3}. [false]\n#external {x_4}. [free]\n#external {x_5}. [release]\n",
            1,
            false,
        ),
        (
            "#assume {a, not x2}.#assume {}.",
            "#assume {x_1; not x_2}.\n#assume {}.\n",
            1,
            false,
        ),
        (
            "#heuristic x1. [1, level]#heuristic x2 : x1. [2@1, true]#heuristic x3 :. [1,level]",
            "#heuristic x_1. [1@0, level]\n#heuristic x_2 : x_1. [2@1, true]\n#heuristic x_3. [1@0, level]\n",
            1,
            false,
        ),
        (
            "#edge (1,2) : x1.#edge (2,1).",
            "#edge (1,2) : x_1.\n#edge (2,1).\n",
            1,
            false,
        ),
    ];
    for (input, expected, steps, incremental) in cases {
        let mut observer = TextObserver::default();
        assert!(read_text(input, &mut observer));
        assert_eq!(observer.program(), expected);
        assert_eq!(observer.base.n_step, steps);
        assert_eq!(observer.base.incremental, incremental);
    }
}

#[test]
fn text_reader_match_agg_parses_mixed_implicit_and_explicit_weights() {
    let mut observer = TextObserver::default();

    assert!(read_text(
        "x1 :- 2 {x2, x3=2, not x4 = 3, x5}.",
        &mut observer
    ));

    assert_eq!(
        observer.program(),
        "x_1 :- 2 {x_2=1; x_3=2; not x_4=3; x_5=1}.\n"
    );
}

#[test]
fn text_reader_output_directives_map_like_upstream() {
    let mut observer = TextObserver::default();
    assert!(read_text("#output a(1) : x1.\n", &mut observer));
    assert_eq!(observer.program(), "");
    assert_eq!(observer.base.atoms.get(&1), Some(&"a(1)".to_owned()));

    let mut observer = TextObserver::default();
    assert!(read_text("#output foo.\n", &mut observer));
    assert_eq!(observer.program(), "#term foo. [0]\n");

    let mut observer = TextObserver::default();
    assert!(read_text("#output a(1) : not x1.\n", &mut observer));
    assert_eq!(observer.program(), "#term a(1) : not x_1. [0]\n");

    let mut observer = TextObserver::default();
    assert!(read_text("#output a(1) : x1, x2.\n", &mut observer));
    assert_eq!(observer.program(), "#term a(1) : x_1; x_2. [0]\n");

    let mut observer = TextObserver::default();
    assert!(read_text("#output \"A(X)\" : x1.\n", &mut observer));
    assert_eq!(observer.program(), "#term \"A(X)\" : x_1. [0]\n");

    let mut observer = TextObserver::default();
    assert!(read_text("#output foo. [term]\n", &mut observer));
    assert_eq!(observer.program(), "#term foo. [0]\n");

    let mut observer = TextObserver::default();
    assert!(read_text(
        "#output a(1) : x1. [term]\n#output a(1) : x2. [term]\n#output a(1) : x3. [term]\n",
        &mut observer,
    ));
    assert_eq!(
        observer.program(),
        "#term a(1) : x_1. [0]\n#term a(1) : x_2. [0]\n#term a(1) : x_3. [0]\n"
    );

    let mut observer = TextObserver::default();
    assert!(read_text(
        "#output a(1) : x1. [term]\n#output a(2) : x2. [term]\n#output a(3) : x3. [term]\n",
        &mut observer,
    ));
    assert_eq!(
        observer.program(),
        "#term a(1) : x_1. [0]\n#term a(2) : x_2. [1]\n#term a(3) : x_3. [2]\n"
    );
}

#[test]
fn text_reader_match_term_preserves_nested_args_and_quoted_strings() {
    let mut observer = TextObserver::default();

    assert!(read_text(
        "#output f(g(1,h(2)),\"A,B\") : x1. [term]\n",
        &mut observer,
    ));

    assert_eq!(
        observer.program(),
        "#term f(g(1,h(2)),\"A,B\") : x_1. [0]\n"
    );
}

#[test]
fn text_reader_incremental_and_errors() {
    let mut observer = TextObserver::default();
    {
        let mut reader = ProgramReader::new(AspifTextInput::new(&mut observer));
        assert!(reader.accept(Cursor::new(
            b"#incremental.\n{x1}.\n#step.\n{x2}.\n".as_slice()
        )));
        assert!(reader.parse(ReadMode::Incremental));
        assert!(reader.parse(ReadMode::Incremental));
    }
    assert!(observer.base.incremental);
    assert_eq!(observer.base.n_step, 2);
    assert_eq!(observer.program(), "{x_2}.\n");

    let message = panic_message(|| {
        let mut observer = TextObserver::default();
        let _ = read_text("#incremental.\n#foo.\n", &mut observer);
    });
    assert!(message.contains("parse error in line 2: unrecognized directive"));

    let message = panic_message(|| {
        let mut observer = TextObserver::default();
        let _ = read_text("#output a(1) : x1. [atom]\n", &mut observer);
    });
    assert!(message.contains("'term' expected"));
}

#[test]
fn text_reader_match_heu_mod_uses_default_priority_and_modifier_name() {
    let mut observer = TextObserver::default();

    assert!(read_text("#heuristic x1. [1, level]", &mut observer));

    assert_eq!(observer.program(), "#heuristic x_1. [1@0, level]\n");
}

#[test]
fn text_writer_empty_program_is_empty_like_upstream() {
    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);

    out.init_program(false);
    out.begin_step();
    out.end_step();

    assert_eq!(render_output(&bytes), "");
}

#[test]
fn text_writer_rules_and_directives() {
    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    let mut rb = RuleBuilder::default();
    out.init_program(false);
    out.begin_step();

    rb.add_head(1).end(Some(&mut out));
    out.output_atom(1, "foo");
    rb.start_with_type(HeadType::Choice)
        .add_head(1)
        .add_head(2)
        .add_goal(-3)
        .add_goal(4)
        .end(Some(&mut out));
    out.output_atom(3, "bar");
    out.external(2, TruthValue::True);
    out.assume(&[1, -2, 3]);
    out.project(&[1, 2, 3]);
    out.acyc_edge(0, 1, &[1, -2]);
    out.heuristic(1, DomModifier::True, 1, 2, &[2, -3]);
    out.end_step();

    assert_eq!(
        render_output(&bytes),
        "foo.\n{foo;x_2} :- not bar, x_4.\n#external x_2. [true]\n#assume{foo, not x_2, bar}.\n#project{foo, x_2, bar}.\n#edge(0,1) : foo, not x_2.\n#heuristic foo : x_2, not bar. [1@2, true]\n#show foo/0.\n#show bar/0.\n"
    );
}

#[test]
fn text_writer_handles_weight_and_minimize_forms() {
    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    out.init_program(false);
    out.begin_step();
    out.rule_weighted(
        HeadType::Disjunctive,
        &[1, 2],
        3,
        &[
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 4, weight: 1 },
            WeightLit { lit: 5, weight: 1 },
            WeightLit { lit: 6, weight: 2 },
        ],
    );
    out.minimize(
        1,
        &[
            WeightLit { lit: -1, weight: 3 },
            WeightLit { lit: -2, weight: 1 },
            WeightLit { lit: -3, weight: 1 },
        ],
    );
    out.end_step();
    assert_eq!(
        render_output(&bytes),
        "x_1|x_2 :- 3 #sum{2,1 : not x_3; 1,2 : x_4; 1,3 : x_5; 2,4 : x_6}.\n#minimize{3@1,1 : not x_1; 1@1,2 : not x_2; 1@1,3 : not x_3}.\n#show.\n"
    );
}

#[test]
fn text_writer_output_name_edge_cases() {
    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    let mut rb = RuleBuilder::default();
    out.init_program(false);
    out.begin_step();
    rb.start_with_type(HeadType::Choice)
        .add_head(1)
        .add_head(2)
        .end(Some(&mut out));
    out.output_atom(1, "a(1)");
    out.output_atom(1, "b(1)");
    out.output_term(0, "a");
    out.output(0, &[1]);
    out.output(0, &[1]);
    out.end_step();
    assert_eq!(
        render_output(&bytes),
        "b(1) :- a(1).\n{a(1);x_2}.\n#show a : a(1).\n#show a : a(1).\n#show a/1.\n#show b/1.\n"
    );

    let message = panic_message(|| {
        let mut bytes = Vec::new();
        let mut out = AspifTextOutput::new(&mut bytes);
        out.init_program(false);
        out.begin_step();
        out.output_atom(1, "x_2");
    });
    assert!(message.contains("reserved"));
}

#[test]
fn text_writer_supports_incremental_programs() {
    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    let mut rb = RuleBuilder::default();
    out.init_program(true);
    out.begin_step();
    rb.start_with_type(HeadType::Choice)
        .add_head(1)
        .add_head(2)
        .end(Some(&mut out));
    out.external(3, TruthValue::False);
    out.output_atom(1, "a(1)");
    out.end_step();
    out.begin_step();
    rb.start().add_head(3).add_goal(1).end(Some(&mut out));
    out.end_step();
    out.begin_step();
    rb.start()
        .add_head(4)
        .add_goal(2)
        .add_goal(-3)
        .end(Some(&mut out));
    out.end_step();
    assert_eq!(
        render_output(&bytes),
        "% #program base.\n{a(1);x_2}.\n#external x_3.\n#show a/1.\n% #program step(1).\nx_3 :- a(1).\n% #program step(2).\nx_4 :- x_2, not x_3.\n"
    );
}

#[test]
fn text_writer_theory_terms_and_atoms() {
    assert_eq!(parens(TupleType::Paren), "()");
    assert_eq!(parens(TupleType::Brace), "{}");
    assert_eq!(parens(TupleType::Bracket), "[]");

    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    out.init_program(false);
    out.begin_step();
    out.theory_term_symbol(0, "t");
    out.theory_atom(0, 0, &[]);
    out.end_step();
    assert_eq!(render_output(&bytes), "&t{}.\n");

    let mut bytes = Vec::new();
    let mut out = AspifTextOutput::new(&mut bytes);
    out.init_program(false);
    out.begin_step();
    out.theory_term_symbol(0, "diff");
    out.theory_term_symbol(1, "<=");
    out.theory_term_number(2, 200);
    out.theory_term_symbol(3, "end");
    out.theory_term_symbol(4, "start");
    out.theory_term_number(5, 1);
    out.theory_term_compound(6, 3, &[5]);
    out.theory_term_compound(7, 4, &[5]);
    out.theory_term_symbol(8, "-");
    out.theory_term_compound(9, 8, &[6, 7]);
    out.theory_element(0, &[9], &[]);
    out.theory_atom_guarded(0, 0, &[0], 1, 2);
    out.end_step();
    assert_eq!(render_output(&bytes), "&diff{end(1) - start(1)} <= 200.\n");
}

#[test]
fn text_writer_rejects_duplicate_theory_atoms() {
    let message = panic_message(|| {
        let mut bytes = Vec::new();
        let mut out = AspifTextOutput::new(&mut bytes);
        out.init_program(false);
        out.begin_step();
        out.theory_term_symbol(0, "t");
        out.theory_term_symbol(1, "x");
        out.theory_atom(1, 0, &[]);
        out.theory_atom(1, 1, &[]);
        out.end_step();
    });
    assert!(message.contains("Redefinition: theory atom"));
}
