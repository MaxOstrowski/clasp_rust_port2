//! Partial Rust port of original_clasp/libpotassco/tests/test_smodels.cpp.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use rust_clasp::potassco::basic_types::{
	AbstractProgram, Atom, BodyType, DomModifier, HeadType, Lit, LitSpan, TruthValue, Weight,
	WeightLit, WeightLitSpan,
};
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco::smodels::{
	SmodelsInput, SmodelsOptions, SmodelsOutput, SmodelsType, match_dom_heu_pred,
	match_edge_pred, read_smodels,
};
use rust_clasp::potassco_check_pre;

use crate::test_common::{Edge, Heuristic, ReadObserver, Vec, to_cond};

type RawRule = Vec<i32>;

fn finalize(text: &mut String, atoms: &[&str], bpos: &str, bneg: &str) {
	text.push_str("0\n");
	for atom in atoms {
		text.push_str(atom);
		text.push('\n');
	}
	text.push_str("0\nB+\n");
	text.push_str(bpos);
	if !bpos.is_empty() && !bpos.ends_with('\n') {
		text.push('\n');
	}
	text.push_str("0\nB-\n");
	text.push_str(bneg);
	if !bneg.is_empty() && !bneg.ends_with('\n') {
		text.push('\n');
	}
	text.push_str("0\n1\n");
}

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

	fn rule(&mut self, head_type: HeadType, head: &[Atom], body: LitSpan<'_>) {
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
			raw[0] = head.len() as i32;
			raw.extend(head.iter().map(|&atom_id| atom_id as i32));
			rule_type = if head_type == HeadType::Choice {
				SmodelsType::Choice
			} else {
				SmodelsType::Disjunctive
			};
		}
		raw.extend(body.iter().copied());
		self.rules.entry(rule_type).or_default().push(raw);
	}

	fn rule_weighted(
		&mut self,
		head_type: HeadType,
		head: &[Atom],
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
		self.rules.entry(SmodelsType::Optimize).or_default().push(raw);
	}

	fn output_atom(&mut self, atom: Atom, name: &str) {
		self.base.output_atom(atom, name);
	}

	fn external(&mut self, atom: Atom, value: TruthValue) {
		let raw = if value == TruthValue::Release {
			vec![atom as i32]
		} else {
			vec![atom as i32, value as i32]
		};
		let kind = if value == TruthValue::Release {
			SmodelsType::ClaspReleaseExt
		} else {
			SmodelsType::ClaspAssignExt
		};
		self.rules.entry(kind).or_default().push(raw);
	}

	fn heuristic(
		&mut self,
		atom: Atom,
		modifier: DomModifier,
		bias: i32,
		priority: u32,
		condition: LitSpan<'_>,
	) {
		self.base.heuristic(atom, modifier, bias, priority, condition);
	}

	fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
		self.base.acyc_edge(source, target, condition);
	}

	fn end_step(&mut self) {}
}

fn parse(program: &str, opts: SmodelsOptions) -> TestObserver {
	let mut observer = TestObserver::default();
	read_smodels(Cursor::new(program.as_bytes()), &mut observer, opts);
	observer
}

fn expect_panic<F>(func: F)
where
	F: FnOnce(),
{
	assert!(catch_unwind(AssertUnwindSafe(func)).is_err());
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
fn smodels_reader_parses_core_directives() {
	let mut input = String::new();
	input.push_str("1 2 4 2 4 2 3 5\n");
	input.push_str("3 2 3 4 2 1 5 6\n");
	input.push_str("8 2 3 4 2 1 5 6\n");
	input.push_str("2 2 2 1 1 3 4\n");
	input.push_str("5 3 3 3 1 4 5 6 1 2 3\n");
	input.push_str("6 0 3 1 4 5 6 1 3 2\n");
	input.push_str("6 0 3 1 4 5 6 1 3 2\n");
	finalize(&mut input, &["1 Foo", "2 Bar", "3 Test(X,Y)"], "2\n3", "1\n4\n5");

	let observer = parse(&input, SmodelsOptions::default());

	assert_eq!(observer.base.n_step, 1);
	assert!(!observer.base.incremental);
	assert_eq!(observer.rules[&SmodelsType::Basic][0], vec![2, -4, -2, 3, 5]);
	assert_eq!(observer.rules[&SmodelsType::Choice][0], vec![2, 3, 4, -5, 6]);
	assert_eq!(
		observer.rules[&SmodelsType::Disjunctive][0],
		vec![2, 3, 4, -5, 6]
	);
	assert_eq!(observer.rules[&SmodelsType::Cardinality][0], vec![2, 1, -3, 4]);
	assert_eq!(
		observer.rules[&SmodelsType::Weight][0],
		vec![3, 3, -4, 1, 5, 2, 6, 3]
	);
	assert_eq!(
		observer.rules[&SmodelsType::Optimize][0],
		vec![0, -4, 1, 5, 3, 6, 2]
	);
	assert_eq!(
		observer.rules[&SmodelsType::Optimize][1],
		vec![1, -4, 1, 5, 3, 6, 2]
	);
	assert_eq!(observer.base.atoms.get(&1), Some(&"Foo".to_owned()));
	assert_eq!(observer.base.atoms.get(&2), Some(&"Bar".to_owned()));
	assert_eq!(observer.base.atoms.get(&3), Some(&"Test(X,Y)".to_owned()));
	assert_eq!(observer.compute, vec![2, 3, -1, -4, -5]);
}

#[test]
fn smodels_reader_handles_empty_and_external_blocks() {
	let mut empty_program = String::new();
	finalize(&mut empty_program, &[], "", "1");
	let observer = parse(&empty_program, SmodelsOptions::default());
	assert_eq!(observer.base.n_step, 1);
	assert_eq!(observer.compute, vec![-1]);

	let external_program = "0\n0\nB+\n0\nB-\n0\nE\n1\n2\n4\n0\n1\n";
	let observer = parse(external_program, SmodelsOptions::default());
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt].len(), 3);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][0], vec![1, 0]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][1], vec![2, 0]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][2], vec![4, 0]);
}

#[test]
fn smodels_reader_handles_clasp_extensions() {
	let mut input = String::new();
	input.push_str("90 0\n91 2 0\n91 3 1\n91 4 2\n92 4\n");
	finalize(&mut input, &[], "", "1");
	input.push_str("90 0\n");
	finalize(&mut input, &[], "", "1");

	let observer = parse(&input, SmodelsOptions::default().enable_clasp_ext());
	assert!(observer.base.incremental);
	assert_eq!(observer.base.n_step, 2);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][0], vec![2, 2]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][1], vec![3, 1]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][2], vec![4, 0]);
	assert_eq!(observer.rules[&SmodelsType::ClaspReleaseExt][0], vec![4]);
}

#[test]
fn smodels_output_round_trips_supported_constructs() {
	let mut writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	writer.init_program(false);
	writer.begin_step();
	writer.rule(HeadType::Disjunctive, &[1], &[2, -3, -4, 5]);
	writer.rule_weighted(
		HeadType::Disjunctive,
		&[1],
		4,
		&[
			WeightLit { lit: 2, weight: 2 },
			WeightLit { lit: -3, weight: 1 },
			WeightLit { lit: -4, weight: 3 },
			WeightLit { lit: 5, weight: 4 },
		],
	);
	writer.minimize(
		10,
		&[
			WeightLit { lit: 2, weight: -2 },
			WeightLit { lit: 3, weight: 1 },
			WeightLit { lit: 4, weight: -1 },
		],
	);
	writer.output_atom(1, "Hallo");
	writer.assume(&[-1, 2, -3, -4, 5, 6]);
	writer.end_step();

	let bytes = writer.into_inner();
	let program = String::from_utf8(bytes).expect("valid UTF-8 smodels text");
	let observer = parse(&program, SmodelsOptions::default());

	assert_eq!(observer.rules[&SmodelsType::Basic][0], vec![1, -3, -4, 2, 5]);
	assert_eq!(
		observer.rules[&SmodelsType::Weight][0],
		vec![1, 4, -3, 1, -4, 3, 2, 2, 5, 4]
	);
	assert_eq!(observer.rules[&SmodelsType::Optimize][0], vec![0, -2, 2, -4, 1, 3, 1]);
	assert_eq!(observer.base.atoms.get(&1), Some(&"Hallo".to_owned()));
	assert_eq!(observer.compute, vec![2, 5, 6, -1, -3, -4]);
}

#[test]
fn smodels_output_enforces_upstream_preconditions() {
	let mut writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	writer.init_program(false);
	writer.begin_step();
	expect_panic(|| writer.rule(HeadType::Disjunctive, &[], &[]));
	writer.rule(HeadType::Choice, &[], &[]);
	expect_panic(|| writer.output_atom(0, "invalid"));
	expect_panic(|| writer.external(1, TruthValue::False));
	expect_panic(|| writer.project(&[]));
	expect_panic(|| writer.heuristic(1, DomModifier::Sign, 1, 0, &[]));
	expect_panic(|| writer.acyc_edge(1, 2, &[]));
	writer.end_step();

	let mut with_false = SmodelsOutput::new(Vec::<u8>::new(), false, 1);
	with_false.init_program(false);
	with_false.begin_step();
	with_false.rule(HeadType::Disjunctive, &[], &[2, -3, -4, 5]);
	with_false.rule_weighted(
		HeadType::Disjunctive,
		&[],
		2,
		&[
			WeightLit { lit: 2, weight: 2 },
			WeightLit { lit: -3, weight: 1 },
			WeightLit { lit: -4, weight: 3 },
			WeightLit { lit: 5, weight: 4 },
		],
	);
	with_false.end_step();
	let rendered = String::from_utf8(with_false.into_inner()).expect("utf8");
	assert!(rendered.contains("1 1 4 2 3 4 2 5"));
	assert!(rendered.contains("5 1 2 4 2 3 4 2 5 1 3 2 4"));

	let mut only_one_compute = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	only_one_compute.init_program(false);
	only_one_compute.begin_step();
	only_one_compute.assume(&[-1]);
	expect_panic(|| only_one_compute.assume(&[2]));
}

#[test]
fn match_dom_heuristic_predicate_follows_upstream_rules() {
	let mut atom_name = "";
	let mut modifier = DomModifier::Init;
	let mut bias = 0;
	let mut priority = 0;

	assert!(!match_dom_heu_pred("heuristic()", &mut atom_name, &mut modifier, &mut bias, &mut priority));
	assert!(!match_dom_heu_pred("_heuristic(x)", &mut atom_name, &mut modifier, &mut bias, &mut priority));
	assert!(!match_dom_heu_pred(
		"_heuristic(x,invalid,1)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert!(match_dom_heu_pred(
		"_heuristic(x,level,-10)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert_eq!(atom_name, "x");
	assert_eq!(modifier, DomModifier::Level);
	assert_eq!(bias, -10);
	assert_eq!(priority, 10);
	assert!(match_dom_heu_pred(
		"_heuristic(x,sign,-2147483648)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert_eq!(bias, i32::MIN);
	assert_eq!(priority, 2_147_483_648);
	assert!(match_dom_heu_pred(
		"_heuristic(a(\"fo\\\"o\"),init,1)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert_eq!(atom_name, "a(\"fo\\\"o\")");
	assert!(match_dom_heu_pred(
		"_heuristic(x,level,-10,123)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert_eq!(priority, 123);
	assert!(!match_dom_heu_pred(
		"_heuristic(x,sign,-2147483649)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
	assert!(!match_dom_heu_pred(
		"_heuristic(a(\"foo,init,1)",
		&mut atom_name,
		&mut modifier,
		&mut bias,
		&mut priority
	));
}

#[test]
fn match_edge_predicate_follows_upstream_rules() {
	let mut left = "";
	let mut right = "";

	assert!(!match_edge_pred("edge()", &mut left, &mut right));
	assert!(!match_edge_pred("_edge(1)", &mut left, &mut right));
	assert!(!match_edge_pred("_acyc_1_foo_bar", &mut left, &mut right));
	assert!(match_edge_pred("_acyc_1_99_100", &mut left, &mut right));
	assert_eq!(left, "99");
	assert_eq!(right, "100");
	assert!(match_edge_pred("_edge(x(\"Foo,bar\"),bar)", &mut left, &mut right));
	assert_eq!(left, "x(\"Foo,bar\")");
	assert_eq!(right, "bar");
}

#[test]
fn smodels_reader_can_convert_edge_and_heuristic_atoms() {
	let mut writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	writer.init_program(false);
	writer.begin_step();
	writer.rule(HeadType::Choice, &[1, 2, 3, 4, 5, 6], &[]);
	writer.output_atom(1, "f(a,b,c,d(q(r(s))))");
	writer.output_atom(2, "f(\"a,b(c,d)\")");
	writer.output_atom(3, "_heuristic(f(a,b,c,d(q(r(s)))),sign,-1)");
	writer.output_atom(4, "_heuristic(f(a,b,c,d(q(r(s)))),true,1)");
	writer.output_atom(5, "_heuristic(f(\"a,b(c,d)\"),level,-1,10)");
	writer.output_atom(6, "_heuristic(f(\"a,b(c,d)\"),factor,2,1)");
	writer.end_step();
	let program = String::from_utf8(writer.into_inner()).expect("utf8");

	let observer = parse(
		&program,
		SmodelsOptions::default()
			.enable_clasp_ext()
			.convert_edges()
			.convert_heuristic(),
	);

	assert_eq!(
		observer.base.heuristics,
		vec![
			Heuristic {
				atom: 1,
				modifier: DomModifier::Sign,
				bias: -1,
				prio: 1,
				cond: Vec::<Lit>::from(to_cond(3)),
			},
			Heuristic {
				atom: 1,
				modifier: DomModifier::True,
				bias: 1,
				prio: 1,
				cond: Vec::<Lit>::from(to_cond(4)),
			},
			Heuristic {
				atom: 2,
				modifier: DomModifier::Level,
				bias: -1,
				prio: 10,
				cond: Vec::<Lit>::from(to_cond(5)),
			},
			Heuristic {
				atom: 2,
				modifier: DomModifier::Factor,
				bias: 2,
				prio: 1,
				cond: Vec::<Lit>::from(to_cond(6)),
			},
		]
	);

	let mut edge_writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	edge_writer.init_program(false);
	edge_writer.begin_step();
	edge_writer.rule(HeadType::Choice, &[1, 2, 3], &[]);
	edge_writer.output_atom(1, "_edge(1,2)");
	edge_writer.output_atom(2, "_acyc_1_1234_4321");
	edge_writer.output_atom(3, "_edge(4321,1234)");
	edge_writer.end_step();
	let edge_program = String::from_utf8(edge_writer.into_inner()).expect("utf8");
	let edge_observer = parse(
		&edge_program,
		SmodelsOptions::default()
			.enable_clasp_ext()
			.convert_edges()
			.convert_heuristic(),
	);

	assert_eq!(edge_observer.base.edges.len(), 3);
	assert_eq!(edge_observer.base.edges[0].cond, Vec::<Lit>::from(to_cond(1)));
	assert_eq!(edge_observer.base.edges[0].s, 0);
	assert_eq!(edge_observer.base.edges[0].t, 1);
	assert_eq!(edge_observer.base.edges[1].cond, Vec::<Lit>::from(to_cond(2)));
	assert_eq!(edge_observer.base.edges[2].cond, Vec::<Lit>::from(to_cond(3)));
}

#[test]
fn smodels_output_supports_extension_directives() {
	let mut writer = SmodelsOutput::new(Vec::<u8>::new(), true, 0);
	writer.init_program(true);
	writer.begin_step();
	writer.rule(HeadType::Choice, &[1, 2], &[3, -4]);
	writer.external(3, TruthValue::False);
	writer.external(4, TruthValue::Free);
	writer.external(5, TruthValue::Release);
	writer.end_step();

	let program = String::from_utf8(writer.into_inner()).expect("utf8");
	let observer = parse(&program, SmodelsOptions::default().enable_clasp_ext());
	assert!(observer.base.incremental);
	assert_eq!(observer.rules[&SmodelsType::Choice][0], vec![2, 1, 2, -4, 3]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][0], vec![3, 2]);
	assert_eq!(observer.rules[&SmodelsType::ClaspAssignExt][1], vec![4, 0]);
	assert_eq!(observer.rules[&SmodelsType::ClaspReleaseExt][0], vec![5]);
}

#[test]
fn smodels_input_type_is_constructible_directly() {
	let mut observer = TestObserver::default();
	let mut reader = rust_clasp::potassco::match_basic_types::ProgramReader::new(SmodelsInput::new(
		&mut observer,
		SmodelsOptions::default(),
	));
	let mut text = String::new();
	finalize(&mut text, &[], "", "1");
	assert_eq!(
		rust_clasp::potassco::match_basic_types::read_program(Cursor::new(text.as_bytes()), &mut reader),
		0
	);
}

#[test]
fn smodels_precondition_panics_use_ported_error_type() {
	let mut observer = ReadObserver::default();
	let error = catch_error(|| observer.output_atom(0, "invalid"));
	assert!(matches!(error, Error::InvalidArgument(message) if message.contains("invalid atom")));

	let mut output = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
	output.init_program(false);
	output.begin_step();
	let error = catch_error(|| {
		potassco_check_pre!(false, "boom");
	});
	assert!(matches!(error, Error::InvalidArgument(message) if message.contains("boom")));
}
