//! Rust port of the convert-related sections in
//! original_clasp/libpotassco/tests/test_smodels.cpp.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, BodyType, DomModifier, HeadType, Lit, LitSpan, TruthValue,
    Weight, WeightLit, WeightLitSpan, lit,
};
use rust_clasp::potassco::convert::SmodelsConvert;
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco::smodels::{SmodelsOptions, SmodelsOutput, SmodelsType, read_smodels};

use crate::test_common::{Heuristic, ReadObserver, Rule as ObsRule, Vec, to_cond};

type RawRule = Vec<i32>;

#[derive(Default)]
struct TestObserver {
    base: ReadObserver,
    rules: BTreeMap<SmodelsType, Vec<RawRule>>,
    compute: Vec<Lit>,
}

impl AbstractProgram for TestObserver {
    fn init_program(&mut self, incremental: bool) {
        self.base.init_program(incremental);
    }

    fn begin_step(&mut self) {
        self.base.begin_step();
    }

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        if head.is_empty() {
            if head_type == HeadType::Choice {
                return;
            }
            self.compute.push(-body[0]);
            return;
        }
        let mut rule_type = SmodelsType::Basic;
        let mut raw = vec![head[0] as i32];
        if head.len() > 1 || head_type == HeadType::Choice {
            rule_type = if head_type == HeadType::Choice {
                SmodelsType::Choice
            } else {
                SmodelsType::Disjunctive
            };
            raw[0] = head.len() as i32;
            raw.extend(head.iter().map(|&atom_id| atom_id as i32));
        }
        raw.extend(body.iter().copied());
        self.rules.entry(rule_type).or_default().push(raw);
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        assert_eq!(head_type, HeadType::Disjunctive);
        assert_eq!(head.len(), 1);
        let mut raw = vec![head[0] as i32, bound];
        let weighted = body.iter().any(|weighted_lit| weighted_lit.weight != 1);
        for weighted_lit in body {
            assert!(weighted_lit.weight >= 0);
            raw.push(weighted_lit.lit);
            if weighted {
                raw.push(weighted_lit.weight);
            }
        }
        let rule_type = if weighted {
            SmodelsType::Weight
        } else {
            SmodelsType::Cardinality
        };
        self.rules.entry(rule_type).or_default().push(raw);
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        let mut raw = vec![priority];
        for weighted_lit in lits {
            raw.push(weighted_lit.lit);
            raw.push(weighted_lit.weight);
        }
        self.rules
            .entry(SmodelsType::Optimize)
            .or_default()
            .push(raw);
    }

    fn output_atom(&mut self, atom: Atom, name: &str) {
        self.base.output_atom(atom, name);
    }

    fn external(&mut self, atom: Atom, value: TruthValue) {
        let kind = if value == TruthValue::Release {
            SmodelsType::ClaspReleaseExt
        } else {
            SmodelsType::ClaspAssignExt
        };
        let raw = if value == TruthValue::Release {
            vec![atom as i32]
        } else {
            vec![atom as i32, value as i32]
        };
        self.rules.entry(kind).or_default().push(raw);
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        self.compute.extend_from_slice(lits);
    }

    fn heuristic(
        &mut self,
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        self.base
            .heuristic(atom, modifier, bias, priority, condition);
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        self.base.acyc_edge(source, target, condition);
    }

    fn end_step(&mut self) {}
}

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

#[test]
fn smodels_output_supports_extended_programs() {
    let mut bytes = Vec::<u8>::new();
    {
        let mut out = SmodelsOutput::new(&mut bytes, true, 0);
        let mut writer = SmodelsConvert::new(&mut out, true);
        writer.init_program(true);
        writer.begin_step();
        writer.rule(HeadType::Choice, &[1, 2], &[3, -4]);
        writer.external(3, TruthValue::False);
        writer.external(4, TruthValue::False);
        writer.minimize(
            0,
            &[
                WeightLit { lit: -1, weight: 2 },
                WeightLit { lit: 2, weight: 1 },
            ],
        );
        writer.heuristic(1, DomModifier::Sign, 1, 0, &[]);
        writer.end_step();
        writer.begin_step();
        writer.rule(HeadType::Choice, &[3, 4], &[]);
        writer.end_step();
    }

    let text = String::from_utf8(bytes).expect("smodels output should be utf8");
    let expected = concat!(
        "90 0\n",
        "3 2 2 3 2 1 5 4\n",
        "1 6 0 0\n",
        "6 0 2 1 2 3 2 1\n",
        "91 4 0\n",
        "91 5 0\n",
        "0\n",
        "6 _heuristic(_atom(2),sign,1,0)\n",
        "2 _atom(2)\n",
        "0\n",
        "B+\n",
        "0\n",
        "B-\n",
        "1\n",
        "0\n",
        "1\n",
        "90 0\n",
        "3 2 4 5 0 0\n",
        "0\n",
        "0\n",
        "B+\n",
        "0\n",
        "B-\n",
        "1\n",
        "0\n",
        "1\n"
    );
    assert_eq!(text, expected);
}

#[test]
fn convert_rule_maps_literals_to_smodels_atoms() {
    let mut observer = TestObserver::default();
    let expected;
    {
        let mut convert = SmodelsConvert::new(&mut observer, true);
        convert.init_program(false);
        convert.begin_step();
        convert.rule(HeadType::Disjunctive, &[1], &[4, -3, -2, 5]);
        expected = vec![
            convert.get(lit(1)),
            convert.get(4),
            convert.get(-3),
            convert.get(-2),
            convert.get(5),
        ];
    }
    assert_eq!(observer.rules[&SmodelsType::Basic], vec![expected]);
}

#[test]
fn convert_weighted_rules_follow_upstream_cases() {
    let mut weighted = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut weighted, true);
        convert.init_program(false);
        convert.begin_step();
        convert.rule_weighted(
            HeadType::Disjunctive,
            &[1],
            3,
            &[
                WeightLit { lit: 4, weight: 2 },
                WeightLit { lit: -3, weight: 3 },
                WeightLit { lit: -2, weight: 1 },
                WeightLit { lit: 5, weight: 4 },
            ],
        );
    }
    assert_eq!(weighted.rules[&SmodelsType::Weight].len(), 1);

    let mut mixed = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut mixed, true);
        convert.init_program(false);
        convert.begin_step();
        convert.rule_weighted(
            HeadType::Choice,
            &[1, 2, 3],
            3,
            &[
                WeightLit { lit: 4, weight: 2 },
                WeightLit { lit: -3, weight: 3 },
                WeightLit { lit: -2, weight: 1 },
                WeightLit { lit: 5, weight: 4 },
            ],
        );
    }
    assert_eq!(mixed.rules[&SmodelsType::Choice].len(), 1);
    assert_eq!(mixed.rules[&SmodelsType::Weight].len(), 1);

    let mut satisfied = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut satisfied, true);
        convert.init_program(false);
        convert.begin_step();
        convert.rule_weighted(
            HeadType::Disjunctive,
            &[1],
            -3,
            &[
                WeightLit { lit: 4, weight: 2 },
                WeightLit { lit: -3, weight: 3 },
                WeightLit { lit: -2, weight: 1 },
                WeightLit { lit: 5, weight: 4 },
            ],
        );
    }
    assert!(!satisfied.rules.contains_key(&SmodelsType::Weight));
    assert_eq!(satisfied.rules[&SmodelsType::Basic].len(), 1);
}

#[test]
fn convert_rejects_negative_weights_in_rule_bodies() {
    let mut observer = TestObserver::default();
    let error = {
        let mut convert = SmodelsConvert::new(&mut observer, true);
        convert.init_program(false);
        convert.begin_step();
        catch_error(|| {
            convert.rule_weighted(
                HeadType::Choice,
                &[1, 2, 3],
                2,
                &[
                    WeightLit { lit: 4, weight: 2 },
                    WeightLit {
                        lit: -3,
                        weight: -3,
                    },
                ],
            )
        })
    };
    assert!(matches!(
        error,
        Error::InvalidArgument(ref message) if message.contains("negative weights in body")
    ));
}

#[test]
fn convert_minimize_flushes_by_priority_on_end_step() {
    let mut observer = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut observer, true);
        convert.init_program(false);
        convert.begin_step();
        convert.minimize(
            3,
            &[
                WeightLit { lit: 8, weight: 1 },
                WeightLit { lit: -7, weight: 2 },
            ],
        );
        convert.minimize(
            10,
            &[
                WeightLit { lit: 4, weight: 1 },
                WeightLit {
                    lit: -3,
                    weight: -2,
                },
                WeightLit { lit: -2, weight: 1 },
                WeightLit { lit: 5, weight: -1 },
            ],
        );
        convert.minimize(
            3,
            &[
                WeightLit { lit: -6, weight: 1 },
                WeightLit { lit: 9, weight: 1 },
            ],
        );
        convert.end_step();
    }
    assert_eq!(observer.rules[&SmodelsType::Optimize].len(), 2);
}

#[test]
fn convert_output_terms_create_step_local_auxiliaries() {
    let mut observer = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut observer, true);
        convert.init_program(false);
        convert.begin_step();
        convert.output_term(0, "Foo");
        convert.output(0, &[1, -2, 3]);
        convert.output(0, &[-1, -3]);
        convert.end_step();
    }
    assert_eq!(observer.rules[&SmodelsType::Basic].len(), 4);
    assert_eq!(observer.base.atoms.len(), 1);
}

#[test]
fn convert_external_edges_and_heuristics_match_upstream_behavior() {
    let mut external = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut external, true);
        convert.init_program(false);
        convert.begin_step();
        convert.external(1, TruthValue::Free);
        convert.external(2, TruthValue::True);
        convert.external(3, TruthValue::False);
        convert.external(4, TruthValue::Release);
        convert.end_step();
    }
    assert_eq!(external.rules[&SmodelsType::ClaspAssignExt].len(), 3);
    assert_eq!(external.rules[&SmodelsType::ClaspReleaseExt].len(), 1);

    let mut edges = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut edges, true);
        let edge_cond_atom = 1;
        convert.init_program(false);
        convert.begin_step();
        convert.output_atom(1, "a");
        convert.acyc_edge(0, 1, &to_cond(&edge_cond_atom).cond());
        convert.acyc_edge(1, 0, &[1, 2, 3]);
        convert.end_step();
    }
    assert_eq!(edges.rules[&SmodelsType::Basic].len(), 2);
    let first_edge_atom = edges.rules[&SmodelsType::Basic][0][0];
    let second_edge_atom = edges.rules[&SmodelsType::Basic][1][0];
    assert_eq!(edges.base.atoms[&first_edge_atom], "_edge(0,1)");
    assert_eq!(edges.base.atoms[&second_edge_atom], "_edge(1,0)");

    let mut heuristics = TestObserver::default();
    {
        let mut convert = SmodelsConvert::new(&mut heuristics, true);
        let heuristic_cond_atom = 2;
        let heuristic_init_cond_atom = 3;
        convert.init_program(false);
        convert.begin_step();
        convert.output_atom(1, "a");
        convert.output_atom(2, "b");
        convert.heuristic(
            1,
            DomModifier::Level,
            10,
            2,
            &to_cond(&heuristic_cond_atom).cond(),
        );
        convert.heuristic(
            1,
            DomModifier::Init,
            10,
            2,
            &to_cond(&heuristic_init_cond_atom).cond(),
        );
        convert.end_step();
    }
    assert_eq!(heuristics.rules[&SmodelsType::Basic].len(), 1);
    let explicit_condition = heuristics.rules[&SmodelsType::Basic][0][0];
    assert_eq!(
        heuristics.base.atoms[&explicit_condition],
        "_heuristic(a,level,10,2)"
    );
    assert!(
        heuristics
            .base
            .atoms
            .values()
            .any(|name| name == "_heuristic(a,init,10,2)")
    );
}

#[test]
fn smodels_reader_converts_edge_and_heuristic_atoms_back_to_directives() {
    let mut bytes = Vec::<u8>::new();
    {
        let mut writer = SmodelsOutput::new(&mut bytes, false, 0);
        writer.init_program(false);
        writer.begin_step();
        writer.rule(HeadType::Choice, &[1, 2, 3, 4, 5, 6, 7], &[]);
        writer.output_atom(1, "_edge(1,2)");
        writer.output_atom(2, r#"_edge("1,2","2,1")"#);
        writer.output_atom(3, r#"_edge("2,1","1,2")"#);
        writer.output_atom(4, "f(a,b,c,d(q(r(s))))");
        writer.output_atom(5, "f(\"a,b(c,d)\")");
        writer.output_atom(6, "_heuristic(f(a,b,c,d(q(r(s)))),sign,-1)");
        writer.output_atom(7, "_heuristic(f(\"a,b(c,d)\"),factor,2,1)");
        writer.end_step();
    }

    let mut observer = TestObserver::default();
    let result = read_smodels(
        Cursor::new(bytes),
        &mut observer,
        SmodelsOptions::default()
            .enable_clasp_ext()
            .convert_edges()
            .convert_heuristic(),
    );
    assert_eq!(result, 0);
    assert_eq!(observer.base.edges.len(), 3);
    let first_edge_cond = 1;
    let second_edge_cond = 2;
    let third_edge_cond = 3;
    assert_eq!(
        observer.base.edges[0].cond,
        to_cond(&first_edge_cond).cond().to_vec()
    );
    assert_eq!(
        observer.base.edges[1].cond,
        to_cond(&second_edge_cond).cond().to_vec()
    );
    assert_eq!(
        observer.base.edges[2].cond,
        to_cond(&third_edge_cond).cond().to_vec()
    );

    assert_eq!(observer.base.heuristics.len(), 2);
    let first_heuristic_cond = 6;
    assert_eq!(
        observer.base.heuristics[0],
        Heuristic {
            atom: 4,
            modifier: DomModifier::Sign,
            bias: -1,
            prio: 1,
            cond: to_cond(&first_heuristic_cond).cond().to_vec(),
        }
    );
    let second_heuristic_cond = 7;
    assert_eq!(
        observer.base.heuristics[1],
        Heuristic {
            atom: 5,
            modifier: DomModifier::Factor,
            bias: 2,
            prio: 1,
            cond: to_cond(&second_heuristic_cond).cond().to_vec(),
        }
    );
}

#[test]
fn test_common_rule_helper_stays_constructible() {
    let rule = ObsRule {
        ht: HeadType::Disjunctive,
        head: vec![1],
        bt: BodyType::Normal,
        bnd: 0,
        body: vec![WeightLit { lit: 1, weight: 1 }],
    };
    assert_eq!(rule.head, vec![1]);
}
