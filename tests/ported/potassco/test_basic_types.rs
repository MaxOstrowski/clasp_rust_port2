use std::cmp::Ordering;

use rust_clasp::potassco::basic_types::{
    ATOM_MAX, ATOM_MIN, AbstractProgram, AtomArg, AtomArgMode, AtomCompare, AtomSpan, BodyType,
    DomModifier, HeadType, LitSpan, TruthValue, Weight, WeightLit, WeightLitSpan, atom,
    atom_symbol, cmp_atom, lit, neg, pop_arg, predicate, to_span, valid_atom, weight,
};
use rust_clasp::potassco::enums::{enum_cast, enum_count, enum_max, enum_min, enum_name};

#[derive(Default)]
struct LifecycleProbe {
    marker: u32,
}

impl AbstractProgram for LifecycleProbe {
    fn rule(&mut self, _head_type: HeadType, _head: AtomSpan<'_>, _body: LitSpan<'_>) {
        panic!("unexpected rule call")
    }

    fn rule_weighted(
        &mut self,
        _head_type: HeadType,
        _head: AtomSpan<'_>,
        _bound: Weight,
        _body: WeightLitSpan<'_>,
    ) {
        panic!("unexpected weighted rule call")
    }

    fn minimize(&mut self, _priority: Weight, _lits: WeightLitSpan<'_>) {
        panic!("unexpected minimize call")
    }

    fn output_atom(&mut self, _atom: u32, _name: &str) {
        panic!("unexpected output_atom call")
    }
}

#[test]
fn basic_type_helpers_follow_upstream_conventions() {
    let atom_id = 7u32;
    let literal = 7i32;
    let weighted = WeightLit { lit: -4, weight: 3 };

    assert_eq!(weight(atom_id), 1);
    assert!(valid_atom(ATOM_MIN));
    assert!(valid_atom(ATOM_MAX));
    assert!(!valid_atom(ATOM_MIN - 1));
    assert!(!valid_atom((ATOM_MAX as i64) + 1));
    assert!(!valid_atom(-400i32));

    assert_eq!(weight(literal), 1);
    assert_eq!(atom(literal), 7);
    assert_eq!(neg(literal), -7);
    assert_eq!(weight(weighted), 3);
    assert_eq!(lit(weighted), -4);
    assert!(weighted > -4);
    assert!(weighted < 3);
    assert_eq!(to_span(&atom_id), &[7u32]);
}

#[test]
fn enum_metadata_is_available_for_ported_basic_enums() {
    assert_eq!(enum_min::<HeadType>(), 0);
    assert_eq!(enum_max::<HeadType>(), 1);
    assert_eq!(enum_count::<HeadType>(), 2);

    assert_eq!(enum_min::<BodyType>(), 0);
    assert_eq!(enum_max::<BodyType>(), 2);
    assert_eq!(enum_count::<BodyType>(), 3);

    assert_eq!(enum_min::<TruthValue>(), 0);
    assert_eq!(enum_max::<TruthValue>(), 3);
    assert_eq!(enum_count::<TruthValue>(), 4);
    assert_eq!(enum_name(TruthValue::False), "false");
    assert_eq!(enum_name(TruthValue::Release), "release");

    assert_eq!(enum_min::<DomModifier>(), 0);
    assert_eq!(enum_max::<DomModifier>(), 5);
    assert_eq!(enum_count::<DomModifier>(), 6);
    assert_eq!(enum_name(DomModifier::Init), "init");
    assert_eq!(enum_name(DomModifier::Level), "level");
    assert_eq!(enum_cast::<TruthValue>(2), Some(TruthValue::False));
    assert_eq!(enum_cast::<TruthValue>(9), None);
}

#[test]
fn cmp_atom_supports_default_natural_and_arity_modes() {
    assert_eq!(cmp_atom("", "", AtomCompare::CMP_DEFAULT), Ordering::Equal);
    assert_eq!(cmp_atom("a", "b", AtomCompare::CMP_DEFAULT), Ordering::Less);
    assert_eq!(
        cmp_atom("10", "2", AtomCompare::CMP_DEFAULT),
        Ordering::Less
    );

    let natural = AtomCompare::CMP_NATURAL;
    assert_eq!(cmp_atom("10", "2", natural), Ordering::Greater);
    assert_eq!(cmp_atom("2", "10", natural), Ordering::Less);
    assert_eq!(cmp_atom("a(2,10,4)", "a(2,10,10)", natural), Ordering::Less);
    assert_eq!(cmp_atom("a(00001)", "a(01)", natural), Ordering::Equal);
    assert_eq!(cmp_atom("a(-12)", "a(-11)", natural), Ordering::Less);

    let arity = AtomCompare::CMP_ARITY;
    assert_eq!(cmp_atom("a(2)", "a(1,0)", arity), Ordering::Less);
    assert_eq!(cmp_atom("a(10,0)", "a(2,0)", arity), Ordering::Less);

    let both = AtomCompare::CMP_ARITY | AtomCompare::CMP_NATURAL;
    assert_eq!(cmp_atom("a(10,0)", "a(2,0)", both), Ordering::Greater);
    assert_eq!(cmp_atom("a(2,0)", "a(10,0)", both), Ordering::Less);
}

#[test]
fn atom_symbol_predicate_and_pop_arg_match_original_cases() {
    assert_eq!(predicate(""), ("", 0));
    assert_eq!(predicate("foo"), ("foo", 0));
    assert_eq!(predicate("foo(x,y)"), ("foo", 2));
    assert_eq!(predicate("tuple((1,2),(3,4))"), ("tuple", 2));
    assert_eq!(predicate("tuple((1,2),(3,4)"), ("tuple", -1));

    assert_eq!(atom_symbol("foo"), ("foo", 0, ""));
    assert_eq!(atom_symbol("foo(x)"), ("foo", 1, "x"));
    assert_eq!(atom_symbol("foo(\"bla\",2)"), ("foo", 2, "\"bla\",2"));
    assert_eq!(atom_symbol("(1,2,3)"), ("", 3, "1,2,3"));
    assert_eq!(atom_symbol("tuple((1,2),(3,4)"), ("tuple", -1, ""));

    let mut args = "1,2,3";
    assert_eq!(pop_arg(&mut args, AtomArg::First, AtomArgMode::Raw), "1");
    assert_eq!(args, "2,3");

    let mut args = "1,2,3";
    assert_eq!(pop_arg(&mut args, AtomArg::Last, AtomArgMode::Raw), "3");
    assert_eq!(args, "1,2");

    let mut args = "(\"1,2\",3),4";
    assert_eq!(
        pop_arg(&mut args, AtomArg::First, AtomArgMode::Raw),
        "(\"1,2\",3)"
    );
    assert_eq!(args, "4");

    let mut args = "\"(1,2)\",\"(3,4)\"";
    assert_eq!(
        pop_arg(&mut args, AtomArg::First, AtomArgMode::Unquote),
        "(1,2)"
    );
    assert_eq!(args, "\"(3,4)\"");
}

#[test]
fn abstract_program_lifecycle_defaults_are_noops() {
    let mut program = LifecycleProbe { marker: 17 };

    program.init_program(true);
    program.begin_step();
    program.end_step();

    assert_eq!(program.marker, 17);
}
