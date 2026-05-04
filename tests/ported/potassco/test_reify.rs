//! Rust port of original_clasp/libpotassco/tests/test_reify.cpp.

use std::io::Cursor;

use rust_clasp::potassco::basic_types::{AbstractProgram, Atom, HeadType, TruthValue, WeightLit};
use rust_clasp::potassco::reify::{Reifier, ReifierOptions};

fn render<F>(options: ReifierOptions, func: F) -> String
where
    F: FnOnce(&mut Reifier<Vec<u8>>),
{
    let mut reifier = Reifier::new(Vec::new(), options);
    func(&mut reifier);
    String::from_utf8(reifier.into_inner()).expect("reifier output must be utf-8")
}

fn render_default<F>(func: F) -> String
where
    F: FnOnce(&mut Reifier<Vec<u8>>),
{
    render(ReifierOptions::default(), func)
}

fn parse_aspif(program: &str) -> String {
    let mut reifier = Reifier::new(Vec::new(), ReifierOptions::default());
    assert_eq!(reifier.parse(Cursor::new(program.as_bytes())), 0);
    String::from_utf8(reifier.into_inner()).expect("reifier output must be utf-8")
}

#[test]
fn reify_empty() {
    assert_eq!(render_default(|_| {}), "");
}

#[test]
fn reify_incremental() {
    assert_eq!(
        render_default(|reifier| reifier.init_program(true)),
        "tag(incremental).\n"
    );
}

#[test]
fn reify_begin_step_is_noop() {
    let out = render(
        ReifierOptions {
            calculate_sccs: false,
            reify_step: true,
        },
        |reifier| {
            reifier.begin_step();
            reifier.project(&[1]);
            reifier.end_step();
        },
    );
    assert_eq!(out, "project(1,0).\n");
}

#[test]
fn reify_normal() {
    let out = render_default(|reifier| {
        reifier.rule(HeadType::Disjunctive, &[1], &[2]);
    });
    assert_eq!(
        out,
        "atom_tuple(0).\natom_tuple(0,1).\nliteral_tuple(0).\nliteral_tuple(0,2).\nrule(disjunction(0),normal(0)).\n"
    );
}

#[test]
fn reify_step() {
    let out = render(
        ReifierOptions {
            calculate_sccs: false,
            reify_step: true,
        },
        |reifier| {
            reifier.init_program(true);
            reifier.rule(HeadType::Disjunctive, &[1], &[2]);
            reifier.end_step();
            reifier.begin_step();
            reifier.rule(HeadType::Disjunctive, &[3], &[2]);
            reifier.end_step();
        },
    );
    assert_eq!(
        out,
        concat!(
            "tag(incremental).\n",
            "atom_tuple(0,0).\n",
            "atom_tuple(0,1,0).\n",
            "literal_tuple(0,0).\n",
            "literal_tuple(0,2,0).\n",
            "rule(disjunction(0),normal(0),0).\n",
            "atom_tuple(0,1).\n",
            "atom_tuple(0,3,1).\n",
            "literal_tuple(0,1).\n",
            "literal_tuple(0,2,1).\n",
            "rule(disjunction(0),normal(0),1).\n"
        )
    );
}

#[test]
fn reify_cycle() {
    let out = render(
        ReifierOptions {
            calculate_sccs: true,
            reify_step: false,
        },
        |reifier| {
            reifier.rule(HeadType::Disjunctive, &[1], &[2]);
            reifier.rule(HeadType::Disjunctive, &[2], &[1]);
            reifier.end_step();
        },
    );
    assert_eq!(
        out,
        concat!(
            "atom_tuple(0).\n",
            "atom_tuple(0,1).\n",
            "literal_tuple(0).\n",
            "literal_tuple(0,2).\n",
            "rule(disjunction(0),normal(0)).\n",
            "atom_tuple(1).\n",
            "atom_tuple(1,2).\n",
            "literal_tuple(1).\n",
            "literal_tuple(1,1).\n",
            "rule(disjunction(1),normal(1)).\n",
            "scc(0,1).\n",
            "scc(0,2).\n"
        )
    );
}

#[test]
fn reify_choice() {
    let out = render_default(|reifier| {
        reifier.rule(HeadType::Choice, &[1, 2], &[]);
    });
    assert_eq!(
        out,
        "atom_tuple(0).\natom_tuple(0,1).\natom_tuple(0,2).\nliteral_tuple(0).\nrule(choice(0),normal(0)).\n"
    );
}

#[test]
fn reify_sum() {
    let out = render_default(|reifier| {
        reifier.rule_weighted(
            HeadType::Disjunctive,
            &[],
            1,
            &[
                WeightLit { lit: 1, weight: 1 },
                WeightLit { lit: 2, weight: 1 },
            ],
        );
    });
    assert_eq!(
        out,
        concat!(
            "atom_tuple(0).\n",
            "weighted_literal_tuple(0).\n",
            "weighted_literal_tuple(0,1,1).\n",
            "weighted_literal_tuple(0,2,1).\n",
            "rule(disjunction(0),sum(0,1)).\n"
        )
    );
}

#[test]
fn reify_minimize() {
    let out = render_default(|reifier| {
        reifier.minimize(
            0,
            &[
                WeightLit { lit: 1, weight: 10 },
                WeightLit { lit: 2, weight: 20 },
            ],
        );
    });
    assert_eq!(
        out,
        concat!(
            "weighted_literal_tuple(0).\n",
            "weighted_literal_tuple(0,1,10).\n",
            "weighted_literal_tuple(0,2,20).\n",
            "minimize(0,0).\n"
        )
    );
}

#[test]
fn reify_project() {
    let out = render_default(|reifier| {
        reifier.project(&[1]);
    });
    assert_eq!(out, "project(1).\n");
}

#[test]
fn reify_output() {
    let out = render_default(|reifier| {
        reifier.output_term(0, "a");
        reifier.output(0, &[2, 3]);
    });
    assert_eq!(
        out,
        "outputTerm(a,0).\nliteral_tuple(0).\nliteral_tuple(0,2).\nliteral_tuple(0,3).\noutput(0,0).\n"
    );
}

#[test]
fn reify_output_atom() {
    let out = render_default(|reifier| {
        reifier.output_atom(1, "a");
    });
    assert_eq!(out, "outputAtom(a,1).\n");
}

#[test]
fn reify_external() {
    let out = render_default(|reifier| {
        reifier.external(1, TruthValue::False);
    });
    assert_eq!(out, "external(1,false).\n");
}

#[test]
fn reify_assume() {
    let out = render_default(|reifier| {
        reifier.assume(&[1]);
    });
    assert_eq!(out, "assume(1).\n");
}

#[test]
fn reify_heuristic() {
    let out = render_default(|reifier| {
        reifier.heuristic(
            1,
            rust_clasp::potassco::basic_types::DomModifier::Level,
            1,
            0,
            &[],
        );
        reifier.heuristic(
            2,
            rust_clasp::potassco::basic_types::DomModifier::True,
            2,
            1,
            &[3],
        );
    });
    assert_eq!(
        out,
        concat!(
            "literal_tuple(0).\n",
            "heuristic(1,level,1,0,0).\n",
            "literal_tuple(1).\n",
            "literal_tuple(1,3).\n",
            "heuristic(2,true,2,1,1).\n"
        )
    );
}

#[test]
fn reify_edge() {
    let out = render_default(|reifier| {
        reifier.acyc_edge(1, 2, &[1]);
        reifier.acyc_edge(2, 1, &[]);
    });
    assert_eq!(
        out,
        "literal_tuple(0).\nliteral_tuple(0,1).\nedge(1,2,0).\nliteral_tuple(1).\nedge(2,1,1).\n"
    );
}

#[test]
fn reify_sorted_tuples() {
    let out = render_default(|reifier| {
        reifier.rule(HeadType::Choice, &[1, 2], &[]);
        reifier.rule(HeadType::Choice, &[2, 1], &[]);
    });
    assert_eq!(
        out,
        "atom_tuple(0).\natom_tuple(0,1).\natom_tuple(0,2).\nliteral_tuple(0).\nrule(choice(0),normal(0)).\nrule(choice(0),normal(0)).\n"
    );
}

#[test]
fn reify_unique_tuples() {
    let out = render_default(|reifier| {
        reifier.rule(HeadType::Choice, &[1, 2, 1], &[]);
        reifier.rule(HeadType::Choice, &[1, 2], &[]);
    });
    assert_eq!(
        out,
        "atom_tuple(0).\natom_tuple(0,1).\natom_tuple(0,2).\nliteral_tuple(0).\nrule(choice(0),normal(0)).\nrule(choice(0),normal(0)).\n"
    );
}

#[test]
fn reify_parse_theory_terms() {
    let out = parse_aspif(concat!(
        "asp 1 0 0\n",
        "9 0 6 42\n",
        "9 1 0 6 banana\n",
        "9 2 14 4 2 12 13\n",
        "0"
    ));
    assert_eq!(
        out,
        concat!(
            "theory_number(6,42).\n",
            "theory_string(0,\"banana\").\n",
            "theory_tuple(0).\n",
            "theory_tuple(0,0,12).\n",
            "theory_tuple(0,1,13).\n",
            "theory_function(14,4,0).\n"
        )
    );
}

#[test]
fn reify_parse_theory_atoms() {
    let out = parse_aspif(concat!(
        "asp 1 0 0\n",
        "9 4 0 1 10 0\n",
        "9 5 6 0 1 1\n",
        "9 6 6 0 1 1 2 3\n",
        "0"
    ));
    assert_eq!(
        out,
        concat!(
            "theory_tuple(0).\n",
            "theory_tuple(0,0,10).\n",
            "literal_tuple(0).\n",
            "theory_element(0,0,0).\n",
            "theory_element_tuple(0).\n",
            "theory_element_tuple(0,1).\n",
            "theory_atom(6,0,0).\n",
            "theory_atom(6,0,0,2,3).\n"
        )
    );
}

#[test]
fn reify_theory_unique_tuples() {
    let out = render_default(|reifier| {
        reifier.theory_term_compound(14, 4, &[12, 13]);
        reifier.theory_term_compound(37, 9, &[12, 13]);
    });
    assert_eq!(
        out,
        concat!(
            "theory_tuple(0).\n",
            "theory_tuple(0,0,12).\n",
            "theory_tuple(0,1,13).\n",
            "theory_function(14,4,0).\n",
            "theory_function(37,9,0).\n"
        )
    );
}

#[test]
fn reify_theory_quoting() {
    let out = render_default(|reifier| {
        reifier.theory_term_symbol(0, "hell\"o");
        reifier.theory_term_symbol(1, "gre\\at");
        reifier.theory_term_symbol(2, "worl\nd");
    });
    assert_eq!(
        out,
        "theory_string(0,\"hell\\\"o\").\ntheory_string(1,\"gre\\\\at\").\ntheory_string(2,\"worl\\nd\").\n"
    );
}

#[test]
fn reify_supports_atom_tuple_dedup_for_weighted_bodies() {
    let out = render_default(|reifier| {
        let head: [Atom; 0] = [];
        reifier.rule_weighted(
            HeadType::Disjunctive,
            &head,
            3,
            &[
                WeightLit { lit: 2, weight: 1 },
                WeightLit { lit: 2, weight: 1 },
                WeightLit { lit: 1, weight: 5 },
            ],
        );
    });
    assert_eq!(
        out,
        concat!(
            "atom_tuple(0).\n",
            "weighted_literal_tuple(0).\n",
            "weighted_literal_tuple(0,1,5).\n",
            "weighted_literal_tuple(0,2,1).\n",
            "rule(disjunction(0),sum(0,3)).\n"
        )
    );
}
