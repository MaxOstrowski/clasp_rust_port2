//! Rust port of original_clasp/libpotassco/tests/test_common.h.

use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::basic_types::{
    AbstractProgram, Atom, BodyType, DomModifier, HeadType, Lit, LitSpan, Weight, WeightLit,
    WeightLitSpan, lit,
};
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco_check_pre;

pub type Vec<T> = std::vec::Vec<T>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomicCond {
    atom: Atom,
}

impl AtomicCond {
    #[must_use]
    pub const fn new(atom: Atom) -> Self {
        Self { atom }
    }

    #[must_use]
    pub fn cond(self) -> [Lit; 1] {
        [lit(self.atom)]
    }
}

impl From<AtomicCond> for Vec<Lit> {
    fn from(value: AtomicCond) -> Self {
        value.cond().into()
    }
}

impl PartialEq<&[Lit]> for AtomicCond {
    fn eq(&self, other: &&[Lit]) -> bool {
        *other == [lit(self.atom)]
    }
}

impl PartialEq<Vec<Lit>> for AtomicCond {
    fn eq(&self, other: &Vec<Lit>) -> bool {
        other.as_slice() == [lit(self.atom)]
    }
}

#[must_use]
pub const fn to_cond(atom: Atom) -> AtomicCond {
    AtomicCond::new(atom)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    pub ht: HeadType,
    pub head: Vec<Atom>,
    pub bt: BodyType,
    pub bnd: Weight,
    pub body: Vec<WeightLit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edge {
    pub s: i32,
    pub t: i32,
    pub cond: Vec<Lit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heuristic {
    pub atom: Atom,
    pub modifier: DomModifier,
    pub bias: i32,
    pub prio: u32,
    pub cond: Vec<Lit>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReadObserver {
    pub atoms: HashMap<Lit, String>,
    pub heuristics: Vec<Heuristic>,
    pub edges: Vec<Edge>,
    pub n_step: i32,
    pub incremental: bool,
    pub allow_zero_atom: bool,
}

impl AbstractProgram for ReadObserver {
    fn init_program(&mut self, incremental: bool) {
        self.incremental = incremental;
    }

    fn begin_step(&mut self) {
        self.n_step += 1;
    }

    fn rule(&mut self, _head_type: HeadType, _head: &[Atom], _body: LitSpan<'_>) {
        panic!("ReadObserver requires a derived test observer to implement rule()")
    }

    fn rule_weighted(
        &mut self,
        _head_type: HeadType,
        _head: &[Atom],
        _bound: Weight,
        _body: WeightLitSpan<'_>,
    ) {
        panic!("ReadObserver requires a derived test observer to implement rule_weighted()")
    }

    fn minimize(&mut self, _priority: Weight, _lits: WeightLitSpan<'_>) {
        panic!("ReadObserver requires a derived test observer to implement minimize()")
    }

    fn output_atom(&mut self, atom: Atom, name: &str) {
        potassco_check_pre!(atom != 0 || self.allow_zero_atom, "invalid atom");
        let slot = self.atoms.entry(lit(atom)).or_default();
        if slot.is_empty() {
            slot.push_str(name);
        } else {
            slot.push(';');
            slot.push_str(name);
        }
    }

    fn heuristic(
        &mut self,
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        self.heuristics.push(Heuristic {
            atom,
            modifier,
            bias,
            prio: priority,
            cond: condition.to_vec(),
        });
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        self.edges.push(Edge {
            s: source,
            t: target,
            cond: condition.to_vec(),
        });
    }
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
fn atomic_cond_behaves_like_a_single_literal_condition() {
    let atom = 17;
    let cond = to_cond(atom);

    assert_eq!(cond.cond(), [17]);
    assert_eq!(Vec::<Lit>::from(cond), vec![17]);
    assert!(cond == &[17][..]);
    assert!(cond == vec![17]);
    assert!(cond != &[18][..]);
}

#[test]
fn helper_record_types_preserve_value_semantics() {
    let lhs = Rule {
        ht: HeadType::Choice,
        head: vec![2, 4],
        bt: BodyType::Sum,
        bnd: 3,
        body: vec![
            WeightLit { lit: 5, weight: 1 },
            WeightLit { lit: -6, weight: 2 },
        ],
    };
    let rhs = Rule {
        ht: HeadType::Choice,
        head: vec![2, 4],
        bt: BodyType::Sum,
        bnd: 3,
        body: vec![
            WeightLit { lit: 5, weight: 1 },
            WeightLit { lit: -6, weight: 2 },
        ],
    };
    assert_eq!(lhs, rhs);
}

#[test]
fn read_observer_tracks_program_state_and_appends_atom_names() {
    let mut observer = ReadObserver::default();

    observer.init_program(true);
    observer.begin_step();
    observer.begin_step();
    observer.heuristic(7, DomModifier::Level, -2, 9, &to_cond(4).cond());
    observer.acyc_edge(3, 5, &to_cond(8).cond());
    observer.output_atom(11, "foo");
    observer.output_atom(11, "bar");

    assert!(observer.incremental);
    assert_eq!(observer.n_step, 2);
    assert_eq!(observer.atoms.get(&11), Some(&"foo;bar".to_owned()));
    assert_eq!(observer.heuristics.len(), 1);
    assert_eq!(
        observer.heuristics[0],
        Heuristic {
            atom: 7,
            modifier: DomModifier::Level,
            bias: -2,
            prio: 9,
            cond: vec![4],
        }
    );
    assert_eq!(
        observer.edges[0],
        Edge {
            s: 3,
            t: 5,
            cond: vec![8],
        }
    );
}

#[test]
fn read_observer_zero_atom_follows_upstream_precondition_flag() {
    let mut observer = ReadObserver::default();
    let error = catch_error(|| observer.output_atom(0, "invalid"));
    assert!(matches!(
        error,
        Error::InvalidArgument(message) if message.contains("invalid atom")
    ));

    observer.allow_zero_atom = true;
    observer.output_atom(0, "ok");
    observer.output_atom(0, "still-ok");
    assert_eq!(observer.atoms.get(&0), Some(&"ok;still-ok".to_owned()));
}
