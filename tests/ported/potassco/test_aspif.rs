//! Rust port of the aspif-specific sections of original_clasp/libpotassco/tests/test_aspif.cpp.

use std::collections::HashMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::test_common::{Edge, Heuristic, ReadObserver, Rule};
use rust_clasp::potassco::aspif::{AspifInput, AspifOutput, AspifType, OutputMapping, read_aspif};
use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, BodyType, DomModifier, HeadType, Id, Lit, LitSpan, TruthValue, Weight,
    WeightLit, WeightLitSpan,
};
use rust_clasp::potassco::match_basic_types::{ProgramReader, read_program};
use rust_clasp::potassco::theory_data::{TheoryAtom, TheoryData, TheoryTermType, TupleType};
use rust_clasp::potassco_check_pre;

const BOUND_NONE: Weight = -1;

#[derive(Default)]
struct TestObserver {
    base: ReadObserver,
    rules: Vec<Rule>,
    min: Vec<(Weight, Vec<WeightLit>)>,
    shows: HashMap<Id, (String, Vec<Vec<Lit>>)>,
    externals: Vec<(Atom, TruthValue)>,
    projects: Vec<Atom>,
    assumes: Vec<Lit>,
    theory: TheoryData,
}

impl AbstractProgram for TestObserver {
    fn init_program(&mut self, incremental: bool) {
        self.base.init_program(incremental);
    }

    fn begin_step(&mut self) {
        self.base.begin_step();
    }

    fn rule(&mut self, head_type: HeadType, head: &[Atom], body: LitSpan<'_>) {
        self.rules.push(Rule {
            ht: head_type,
            head: head.to_vec(),
            bt: BodyType::Normal,
            bnd: BOUND_NONE,
            body: body
                .iter()
                .copied()
                .map(|lit| WeightLit { lit, weight: 1 })
                .collect(),
        });
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: &[Atom],
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        self.rules.push(Rule {
            ht: head_type,
            head: head.to_vec(),
            bt: BodyType::Sum,
            bnd: bound,
            body: body.to_vec(),
        });
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        self.min.push((priority, lits.to_vec()));
    }

    fn output_atom(&mut self, atom: Atom, name: &str) {
        potassco_check_pre!(atom != 0 || self.base.allow_zero_atom, "invalid atom");
        let slot = self.base.atoms.entry(atom as Lit).or_default();
        if slot.is_empty() {
            slot.push_str(name);
        } else {
            slot.push(';');
            slot.push_str(name);
        }
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        self.shows.entry(term_id).or_default().0 = name.to_owned();
    }

    fn output(&mut self, term_id: Id, condition: LitSpan<'_>) {
        self.shows
            .entry(term_id)
            .or_default()
            .1
            .push(condition.to_vec());
    }

    fn project(&mut self, atoms: &[Atom]) {
        self.projects.extend_from_slice(atoms);
    }

    fn external(&mut self, atom: Atom, value: TruthValue) {
        self.externals.push((atom, value));
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        self.assumes.extend_from_slice(lits);
    }

    fn heuristic(
        &mut self,
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        self.base.heuristics.push(Heuristic {
            atom,
            modifier,
            bias,
            prio: priority,
            cond: condition.to_vec(),
        });
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        self.base.edges.push(Edge {
            s: source,
            t: target,
            cond: condition.to_vec(),
        });
    }

    fn theory_term_number(&mut self, term_id: Id, number: i32) {
        self.theory.add_term_number(term_id, number);
    }

    fn theory_term_symbol(&mut self, term_id: Id, name: &str) {
        self.theory.add_term_symbol(term_id, name);
    }

    fn theory_term_compound(&mut self, term_id: Id, compound: i32, args: &[Id]) {
        if compound >= 0 {
            self.theory.add_term_function(term_id, compound as Id, args);
        } else {
            let tuple_type = match compound {
                -1 => TupleType::Paren,
                -2 => TupleType::Brace,
                -3 => TupleType::Bracket,
                _ => panic!("unexpected tuple type"),
            };
            self.theory.add_term_tuple(term_id, tuple_type, args);
        }
    }

    fn theory_element(&mut self, element_id: Id, terms: &[Id], _cond: LitSpan<'_>) {
        self.theory.add_element(element_id, terms, 0);
    }

    fn theory_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: &[Id]) {
        self.theory.add_atom(atom_or_zero, term_id, elements);
    }

    fn theory_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: &[Id],
        op: Id,
        rhs: Id,
    ) {
        self.theory
            .add_atom_guarded(atom_or_zero, term_id, elements, op, rhs);
    }

    fn end_step(&mut self) {}
}

fn to_aspif_rule(rule: &Rule) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} {}",
        AspifType::Rule as u32,
        rule.ht as u8,
        rule.head.len()
    ));
    for atom in &rule.head {
        out.push_str(&format!(" {}", atom));
    }
    out.push_str(&format!(" {} ", rule.bt as u8));
    if rule.bt == BodyType::Sum {
        out.push_str(&format!("{} {}", rule.bnd, rule.body.len()));
        for wlit in &rule.body {
            out.push_str(&format!(" {} {}", wlit.lit, wlit.weight));
        }
    } else {
        out.push_str(&rule.body.len().to_string());
        for wlit in &rule.body {
            out.push_str(&format!(" {}", wlit.lit));
        }
    }
    out.push('\n');
    out
}

fn read_with(program: &str, mapping: OutputMapping, fact: Atom) -> TestObserver {
    let mut observer = TestObserver::default();
    let mut reader = ProgramReader::new(AspifInput::new(&mut observer, mapping, fact));
    read_program(Cursor::new(program.as_bytes()), &mut reader);
    observer
}

fn catches_panic<F>(func: F) -> bool
where
    F: FnOnce(),
{
    catch_unwind(AssertUnwindSafe(func)).is_err()
}

fn render_term(data: &TheoryData, term_id: Id) -> String {
    let term = data.get_term(term_id).expect("term exists");
    match term.term_type() {
        TheoryTermType::Number => term.number().expect("number term").to_string(),
        TheoryTermType::Symbol => term.symbol().expect("symbol term").to_owned(),
        TheoryTermType::Compound => {
            let rendered_args = term
                .terms()
                .iter()
                .map(|&arg| render_term(data, arg))
                .collect::<Vec<_>>()
                .join(",");
            if term.is_function() {
                format!(
                    "{}({})",
                    render_term(data, term.function().expect("function id")),
                    rendered_args
                )
            } else {
                let parens = match term.tuple().expect("tuple term") {
                    TupleType::Bracket => ("[", "]"),
                    TupleType::Brace => ("{", "}"),
                    TupleType::Paren => ("(", ")"),
                };
                format!("{}{}{}", parens.0, rendered_args, parens.1)
            }
        }
    }
}

fn render_atom(data: &TheoryData, atom: &TheoryAtom) -> String {
    let mut out = String::from("&");
    out.push_str(&render_term(data, atom.term()));
    for &element_id in atom.elements() {
        let element = data.get_element(element_id).expect("element exists");
        out.push('{');
        for &term_id in element.terms() {
            out.push_str(&render_term(data, term_id));
        }
        out.push('}');
    }
    if let Some(&guard) = atom.guard() {
        out.push_str(&render_term(data, guard));
    }
    if let Some(&rhs) = atom.rhs() {
        out.push_str(&render_term(data, rhs));
    }
    out
}

#[test]
fn aspif_input_reads_rules_minimize_and_basic_v1_output() {
    let rules = [
        Rule {
            ht: HeadType::Disjunctive,
            head: vec![1],
            bt: BodyType::Normal,
            bnd: BOUND_NONE,
            body: vec![
                WeightLit { lit: -2, weight: 1 },
                WeightLit { lit: 3, weight: 1 },
            ],
        },
        Rule {
            ht: HeadType::Choice,
            head: vec![1, 2],
            bt: BodyType::Sum,
            bnd: 1,
            body: vec![
                WeightLit { lit: 2, weight: 1 },
                WeightLit { lit: -3, weight: 2 },
                WeightLit { lit: 5, weight: 1 },
            ],
        },
    ];
    let mut program = String::from("asp 1 0 0\n");
    for rule in &rules {
        program.push_str(&to_aspif_rule(rule));
    }
    program.push_str("2 -1 3 4 5 6 1 3 2\n");
    program.push_str("4 1 a 1 1\n");
    program.push_str("0\n");

    let observer = read_with(&program, OutputMapping::Atom, 0);

    assert_eq!(observer.base.n_step, 1);
    assert_eq!(observer.rules, rules);
    assert_eq!(
        observer.min,
        vec![(
            -1,
            vec![
                WeightLit { lit: 4, weight: 5 },
                WeightLit { lit: 6, weight: 1 },
                WeightLit { lit: 3, weight: 2 }
            ]
        )]
    );
    assert_eq!(observer.base.atoms.get(&1), Some(&"a".to_owned()));
}

#[test]
fn aspif_input_reuses_facts_for_empty_v1_output_and_round_robins() {
    let program = concat!(
        "asp 1 0 0\n",
        "1 0 1 1 0 0\n",
        "1 0 1 2 0 0\n",
        "1 0 1 3 0 0\n",
        "4 1 a 0\n",
        "4 1 b 0\n",
        "4 1 c 0\n",
        "4 1 d 0\n",
        "0\n"
    );

    let observer = read_with(program, OutputMapping::Atom, 0);

    assert_eq!(observer.rules.len(), 3);
    assert_eq!(observer.base.atoms.get(&1), Some(&"a;d".to_owned()));
    assert_eq!(observer.base.atoms.get(&2), Some(&"b".to_owned()));
    assert_eq!(observer.base.atoms.get(&3), Some(&"c".to_owned()));
    assert!(observer.shows.is_empty());
}

#[test]
fn aspif_input_maps_empty_v1_output_to_fixed_fact_atom() {
    let program = concat!("asp 1 0 0\n", "4 1 a 0\n", "4 3 foo 0\n", "0\n");

    let observer = read_with(program, OutputMapping::AtomFact, 15);

    assert_eq!(observer.base.atoms.get(&15), Some(&"a;foo".to_owned()));
    assert!(observer.shows.is_empty());

    let mut zero_observer = TestObserver::default();
    zero_observer.base.allow_zero_atom = true;
    let mut reader = ProgramReader::new(AspifInput::new(
        &mut zero_observer,
        OutputMapping::AtomFact,
        0,
    ));
    read_program(Cursor::new(program.as_bytes()), &mut reader);
    assert_eq!(zero_observer.base.atoms.get(&0), Some(&"a;foo".to_owned()));
}

#[test]
fn aspif_input_reuses_fact_atom_across_incremental_steps() {
    let program = concat!(
        "asp 1 0 0 incremental\n",
        "1 0 1 1 0 0\n",
        "4 1 a 0\n",
        "0\n",
        "1 0 2 2 3 0 0\n",
        "4 1 x 0\n",
        "0\n"
    );

    let observer = read_with(program, OutputMapping::Atom, 0);

    assert!(observer.base.incremental);
    assert_eq!(observer.base.n_step, 2);
    assert_eq!(observer.base.atoms.get(&1), Some(&"a;x".to_owned()));
    assert_eq!(observer.rules.len(), 2);
    assert_eq!(observer.rules[0].head, vec![1]);
    assert_eq!(observer.rules[1].head, vec![2, 3]);
}

#[test]
fn aspif_input_ignores_comments_and_rejects_invalid_headers() {
    let observer = read_with(
        concat!("asp 1 0 0\n", "10Hello World\n", "0\n"),
        OutputMapping::Atom,
        0,
    );
    assert_eq!(observer.base.n_step, 1);

    for program in [
        concat!("asp 1 0 0\n", "0\n", "0\n"),
        "asp 1 2 0 incremental\n0\n",
        "asp 1 0 0 foo\n0\n",
    ] {
        assert!(catches_panic(|| {
            let _ = read_with(program, OutputMapping::Atom, 0);
        }));
    }
}

#[test]
fn aspif_input_reads_projection_external_assume_edge_heuristic_and_theory() {
    let program = concat!(
        "asp 1 0 0\n",
        "3 3 1 2 987232\n",
        "5 1 0\n",
        "5 2 1\n",
        "6 2 1 -2\n",
        "8 0 1 2 1 -2\n",
        "7 1 2 -1 3 2 -1 10\n",
        "9 0 1 200\n",
        "9 0 6 1\n",
        "9 0 11 2\n",
        "9 1 0 4 diff\n",
        "9 1 2 2 <=\n",
        "9 1 4 1 -\n",
        "9 1 5 3 end\n",
        "9 1 8 5 start\n",
        "9 2 10 4 2 7 9\n",
        "9 2 7 5 1 6\n",
        "9 2 9 8 1 6\n",
        "9 4 0 1 10 0\n",
        "9 6 0 0 1 0 2 1\n",
        "0\n"
    );

    let observer = read_with(program, OutputMapping::Atom, 0);

    assert_eq!(observer.projects, vec![1, 2, 987232]);
    assert_eq!(
        observer.externals,
        vec![(1, TruthValue::Free), (2, TruthValue::True)]
    );
    assert_eq!(observer.assumes, vec![1, -2]);
    assert_eq!(
        observer.base.edges,
        vec![Edge {
            s: 0,
            t: 1,
            cond: vec![1, -2]
        }]
    );
    assert_eq!(
        observer.base.heuristics,
        vec![Heuristic {
            atom: 2,
            modifier: DomModifier::Sign,
            bias: -1,
            prio: 3,
            cond: vec![-1, 10]
        }]
    );
    assert_eq!(observer.theory.num_atoms(), 1);

    assert_eq!(
        render_atom(&observer.theory, &observer.theory.atoms()[0]),
        "&diff{-(end(1),start(1))}<=200"
    );
}

#[test]
fn aspif_input_supports_version2_output_forms_and_rejects_v1_output_records() {
    let program = concat!(
        "asp 2 0 0\n",
        "4 0 1 3 foo\n",
        "4 1 0 3 bar\n",
        "4 2 0 2 1 -2\n",
        "0\n"
    );
    let observer = read_with(program, OutputMapping::Atom, 0);

    assert_eq!(observer.base.atoms.get(&1), Some(&"foo".to_owned()));
    assert_eq!(
        observer.shows.get(&0),
        Some(&("bar".to_owned(), vec![vec![1, -2]]))
    );

    let bad_program = concat!("asp 2 0 0\n", "4 1 a 1 1\n", "0\n");
    assert!(catches_panic(|| {
        let _ = read_with(bad_program, OutputMapping::Atom, 0);
    }));

    let zero_atom_program = concat!("asp 2 0 0\n", "4 0 0 3 foo\n", "0\n");
    assert!(catches_panic(|| {
        let _ = read_with(zero_atom_program, OutputMapping::Atom, 0);
    }));
}

#[test]
fn read_aspif_uses_default_atom_mapping() {
    let program = concat!("asp 1 0 0\n", "1 0 1 1 0 0\n", "4 1 a 0\n", "0\n");
    let mut observer = TestObserver::default();
    read_aspif(Cursor::new(program.as_bytes()), &mut observer);
    assert_eq!(observer.base.atoms.get(&1), Some(&"a".to_owned()));
}

#[test]
fn aspif_output_writes_rules_minimize_external_assume_project_edge_and_heuristic() {
    let mut writer = AspifOutput::new(Vec::new(), 1);
    writer.init_program(false);
    writer.begin_step();
    writer.rule(HeadType::Disjunctive, &[1], &[-2, 3, -4]);
    writer.rule_weighted(
        HeadType::Choice,
        &[1, 2],
        1,
        &[
            WeightLit { lit: 2, weight: 1 },
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 5, weight: 1 },
        ],
    );
    writer.minimize(
        1,
        &[
            WeightLit { lit: 1, weight: -2 },
            WeightLit { lit: -3, weight: 2 },
        ],
    );
    writer.external(2, TruthValue::Release);
    writer.assume(&[1, -2]);
    writer.project(&[1, 987232]);
    writer.acyc_edge(0, 1, &[1, -2]);
    writer.heuristic(2, DomModifier::Sign, -1, 3, &[-1, 10]);
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    let expected = concat!(
        "asp 1 0 0\n",
        "1 0 1 1 0 3 -2 3 -4\n",
        "1 1 2 1 2 1 1 3 2 1 -3 2 5 1\n",
        "2 1 2 1 -2 -3 2\n",
        "5 2 3\n",
        "6 2 1 -2\n",
        "3 2 1 987232\n",
        "8 0 1 2 1 -2\n",
        "7 1 2 -1 3 2 -1 10\n",
        "0\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn aspif_output_converts_v1_output_terms_like_upstream() {
    let mut writer = AspifOutput::new(Vec::new(), 1);
    writer.init_program(false);
    writer.begin_step();
    writer.output_term(0, "a_term");
    writer.output_term(1, "another_term");
    writer.output_term(2, "fact");
    writer.output(0, &[1, 2, 3]);
    writer.output(1, &[-1, -2]);
    writer.output(2, &[]);
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    let expected = concat!(
        "asp 1 0 0\n",
        "1 0 1 4 0 0\n",
        "4 6 a_term 2 5 4\n",
        "1 0 1 5 0 1 6\n",
        "1 0 1 6 0 3 1 2 3\n",
        "4 12 another_term 2 7 4\n",
        "1 0 1 7 0 1 8\n",
        "1 0 1 8 0 2 -1 -2\n",
        "4 4 fact 2 9 4\n",
        "1 0 1 9 0 0\n",
        "0\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn aspif_output_interleaves_v1_term_conversion_with_other_directives() {
    let mut writer = AspifOutput::new(Vec::new(), 1);
    writer.init_program(false);
    writer.begin_step();
    writer.minimize(
        1,
        &[
            WeightLit { lit: 1, weight: -2 },
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 4, weight: 1 },
        ],
    );
    writer.output_term(0, "a_term");
    writer.output(0, &[1, 2, 3]);
    writer.minimize(
        -2,
        &[
            WeightLit { lit: 1, weight: -2 },
            WeightLit { lit: -3, weight: 2 },
            WeightLit { lit: 4, weight: 1 },
            WeightLit { lit: 10, weight: 3 },
        ],
    );
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    let expected = concat!(
        "asp 1 0 0\n",
        "2 1 3 1 -2 -3 2 4 1\n",
        "1 0 1 5 0 0\n",
        "4 6 a_term 2 6 5\n",
        "1 0 1 6 0 1 7\n",
        "1 0 1 7 0 3 1 2 3\n",
        "2 -2 4 1 -2 -3 2 4 1 8 3\n",
        "0\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn aspif_output_preserves_incremental_v1_term_state_across_steps() {
    let mut writer = AspifOutput::new(Vec::new(), 1);
    writer.init_program(true);
    writer.begin_step();
    writer.rule(HeadType::Choice, &[1, 2, 3], &[]);
    writer.output_term(0, "foo");
    writer.output(0, &[1, -2, 3]);
    writer.output(0, &[-1, -3]);
    writer.end_step();
    writer.begin_step();
    writer.rule(HeadType::Choice, &[4], &[]);
    writer.output(0, &[3, 4]);
    writer.output(0, &[-3, -4]);
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    let expected = concat!(
        "asp 1 0 0 incremental\n",
        "1 1 3 1 2 3 0 0\n",
        "1 0 1 4 0 0\n",
        "4 3 foo 2 5 4\n",
        "1 0 1 5 0 1 6\n",
        "1 0 1 6 0 3 1 -2 3\n",
        "1 0 1 5 0 1 7\n",
        "1 0 1 7 0 2 -1 -3\n",
        "0\n",
        "1 1 1 8 0 0\n",
        "4 3 foo 2 9 4\n",
        "1 0 1 9 0 2 10 -5\n",
        "1 0 1 10 0 2 3 8\n",
        "1 0 1 9 0 2 11 -5\n",
        "1 0 1 11 0 2 -3 -8\n",
        "0\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn aspif_output_handles_incremental_v1_term_rewrites_without_aux_rule() {
    let mut writer = AspifOutput::new(Vec::new(), 1);
    writer.init_program(true);
    writer.begin_step();
    writer.rule(HeadType::Choice, &[1, 2], &[]);
    writer.rule(HeadType::Disjunctive, &[], &[-1, -2]);
    writer.output_term(0, "a");
    writer.output(0, &[2]);
    writer.end_step();
    writer.begin_step();
    writer.output(0, &[1]);
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    let expected = concat!(
        "asp 1 0 0 incremental\n",
        "1 1 2 1 2 0 0\n",
        "1 0 0 0 2 -1 -2\n",
        "1 0 1 3 0 0\n",
        "4 1 a 2 4 3\n",
        "1 0 1 4 0 1 2\n",
        "0\n",
        "4 1 a 2 5 3\n",
        "1 0 1 5 0 2 1 -4\n",
        "0\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn aspif_output_supports_v2_outputs_and_rejects_zero_atom() {
    let mut writer = AspifOutput::new(Vec::new(), 0);
    writer.init_program(false);
    writer.begin_step();
    writer.output_atom(1, "an_atom");
    writer.output_term(0, "a_term");
    writer.output(0, &[1, 2, 3]);
    writer.end_step();

    let out = String::from_utf8(writer.into_inner()).expect("utf8 aspif output");
    assert_eq!(
        out,
        concat!(
            "asp 2 0 0\n",
            "4 0 1 7 an_atom\n",
            "4 1 0 6 a_term\n",
            "4 2 0 3 1 2 3\n",
            "0\n"
        )
    );

    let mut bad = AspifOutput::new(Vec::new(), 0);
    bad.init_program(false);
    bad.begin_step();
    assert!(catches_panic(|| bad.output_atom(0, "fact")));
}
