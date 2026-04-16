use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::basic_types::{Lit, TruthValue, WeightLit};
use rust_clasp::potassco::clingo::{
    AbstractAssignment, AbstractStatistics, ClauseType, PropagatorCheckMode, PropagatorControl,
    PropagatorInit, PropagatorUndoMode, StatisticsKey, StatisticsType,
};
use rust_clasp::potassco::enums::EnumTag;
use rust_clasp::potassco::error::Error;

#[derive(Default)]
struct DummyAssignment {
    values: Vec<(Lit, TruthValue, u32)>,
    decisions: Vec<Lit>,
    trail: Vec<Lit>,
    trail_starts: Vec<u32>,
    conflicts: bool,
    size: u32,
}

impl DummyAssignment {
    fn new() -> Self {
        Self {
            values: vec![
                (1, TruthValue::True, 0),
                (-1, TruthValue::False, 0),
                (2, TruthValue::True, 1),
                (-2, TruthValue::False, 1),
                (3, TruthValue::Free, u32::MAX),
                (-3, TruthValue::Free, u32::MAX),
            ],
            decisions: vec![2],
            trail: vec![1, 2],
            trail_starts: vec![0, 1],
            conflicts: false,
            size: 3,
        }
    }
}

impl AbstractAssignment for DummyAssignment {
    fn solver_id(&self) -> u32 {
        7
    }

    fn size(&self) -> u32 {
        self.size
    }

    fn unassigned(&self) -> u32 {
        1
    }

    fn has_conflict(&self) -> bool {
        self.conflicts
    }

    fn level(&self) -> u32 {
        1
    }

    fn root_level(&self) -> u32 {
        0
    }

    fn has_lit(&self, lit: Lit) -> bool {
        self.values
            .iter()
            .any(|(candidate, _, _)| *candidate == lit)
    }

    fn value(&self, lit: Lit) -> TruthValue {
        self.values
            .iter()
            .find_map(|(candidate, value, _)| (*candidate == lit).then_some(*value))
            .unwrap_or(TruthValue::Free)
    }

    fn level_of(&self, lit: Lit) -> u32 {
        self.values
            .iter()
            .find_map(|(candidate, _, level)| (*candidate == lit).then_some(*level))
            .unwrap_or(u32::MAX)
    }

    fn decision(&self, level: u32) -> Lit {
        self.decisions[level as usize]
    }

    fn trail_size(&self) -> u32 {
        self.trail.len() as u32
    }

    fn trail_at(&self, pos: u32) -> Lit {
        self.trail[pos as usize]
    }

    fn trail_begin(&self, level: u32) -> u32 {
        if level as usize >= self.trail_starts.len() {
            self.trail.len() as u32
        } else {
            self.trail_starts[level as usize]
        }
    }
}

#[derive(Default)]
struct DummyControl {
    last_clause: Vec<Lit>,
    last_clause_type: Option<ClauseType>,
    last_freeze: Option<bool>,
    watches: Vec<Lit>,
}

impl PropagatorControl for DummyControl {
    fn add_clause(&mut self, clause: &[Lit], clause_type: ClauseType) -> bool {
        self.last_clause = clause.to_vec();
        self.last_clause_type = Some(clause_type);
        true
    }

    fn add_weight_constraint(
        &mut self,
        _con: Lit,
        _lits: &[WeightLit],
        _bound: i32,
        _relation: i32,
    ) -> bool {
        true
    }

    fn add_variable(&mut self, freeze: bool) -> Lit {
        self.last_freeze = Some(freeze);
        99
    }

    fn propagate(&mut self) -> bool {
        true
    }

    fn has_watch(&self, lit: Lit) -> bool {
        self.watches.contains(&lit)
    }

    fn add_watch(&mut self, lit: Lit) {
        self.watches.push(lit);
    }

    fn remove_watch(&mut self, lit: Lit) {
        self.watches.retain(|candidate| *candidate != lit);
    }
}

impl PropagatorInit for DummyControl {
    fn check_mode(&self) -> PropagatorCheckMode {
        PropagatorCheckMode::Fixpoint
    }

    fn undo_mode(&self) -> PropagatorUndoMode {
        PropagatorUndoMode::Default
    }

    fn num_solver(&self) -> u32 {
        1
    }

    fn solver_literal(&self, lit: Lit) -> Lit {
        lit
    }

    fn set_check_mode(&mut self, _mode: PropagatorCheckMode) {}

    fn set_undo_mode(&mut self, _mode: PropagatorUndoMode) {}

    fn freeze_variable(&mut self, lit: Lit) {
        self.add_watch(lit);
    }

    fn add_minimize(&mut self, _priority: i32, _lit: WeightLit) {}
}

#[derive(Default)]
struct DummyStats;

impl AbstractStatistics for DummyStats {
    fn root(&self) -> StatisticsKey {
        0
    }

    fn type_of(&self, _key: StatisticsKey) -> StatisticsType {
        StatisticsType::Value
    }

    fn size(&self, _key: StatisticsKey) -> usize {
        0
    }

    fn writable(&self, _key: StatisticsKey) -> bool {
        false
    }

    fn at(&self, _array: StatisticsKey, _index: usize) -> StatisticsKey {
        0
    }

    fn push(&mut self, _array: StatisticsKey, _item_type: StatisticsType) -> StatisticsKey {
        0
    }

    fn key(&self, _map: StatisticsKey, _index: usize) -> &str {
        ""
    }

    fn get(&self, _map: StatisticsKey, _at: &str) -> StatisticsKey {
        0
    }

    fn find(
        &self,
        _map: StatisticsKey,
        _element: &str,
        _out_key: Option<&mut StatisticsKey>,
    ) -> bool {
        false
    }

    fn add(
        &mut self,
        _map: StatisticsKey,
        _name: &str,
        _item_type: StatisticsType,
    ) -> StatisticsKey {
        0
    }

    fn value(&self, _key: StatisticsKey) -> f64 {
        0.0
    }

    fn set(&mut self, _key: StatisticsKey, _value: f64) {}
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
fn assignment_helpers_follow_upstream_semantics() {
    let assignment = DummyAssignment::new();

    assert_eq!(assignment.trail_end(0), 1);
    assert_eq!(assignment.trail_end(1), 2);
    assert!(!assignment.is_total());
    assert!(assignment.is_fixed(1));
    assert!(!assignment.is_fixed(2));
    assert!(assignment.is_true(2));
    assert!(assignment.is_false(-2));
    assert!(!assignment.is_true(3));
}

#[test]
fn control_default_helpers_match_upstream_defaults() {
    let mut control = DummyControl::default();

    assert!(control.add_clause_default(&[1, -2]));
    assert_eq!(control.last_clause, vec![1, -2]);
    assert_eq!(control.last_clause_type, Some(ClauseType::Learnt));
    assert_eq!(control.add_variable_default(), 99);
    assert_eq!(control.last_freeze, Some(true));
}

#[test]
fn clause_type_bit_operations_preserve_two_bit_encoding() {
    assert_eq!(
        ClauseType::Locked | ClauseType::Transient,
        ClauseType::TransientLocked
    );
    assert_eq!(
        ClauseType::TransientLocked & ClauseType::Locked,
        ClauseType::Locked
    );
    assert_eq!(
        ClauseType::TransientLocked ^ ClauseType::Locked,
        ClauseType::Transient
    );
    assert_eq!(!ClauseType::Learnt, ClauseType::TransientLocked);
}

#[test]
fn statistics_type_exposes_named_entries() {
    assert_eq!(StatisticsType::Value.name(), Some("value"));
    assert_eq!(StatisticsType::Array.name(), Some("array"));
    assert_eq!(StatisticsType::Map.name(), Some("map"));
}

#[test]
fn statistics_throw_helpers_format_expected_errors() {
    let wrong_type = catch_error(|| {
        <DummyStats as AbstractStatistics>::throw_type(StatisticsType::Value, StatisticsType::Array)
    });
    assert_eq!(
        wrong_type,
        Error::InvalidArgument("bad stats access: 'value' expected but got 'array'".to_owned())
    );

    let wrong_key = catch_error(|| <DummyStats as AbstractStatistics>::throw_key(42));
    assert_eq!(
        wrong_key,
        Error::InvalidArgument("bad stats access: invalid key '42'".to_owned())
    );

    let wrong_path =
        catch_error(|| <DummyStats as AbstractStatistics>::throw_path("root.child", "leaf"));
    assert_eq!(
        wrong_path,
        Error::OutOfRange("bad stats access: invalid key 'leaf' in path 'root.child'".to_owned())
    );

    let wrong_path_tail =
        catch_error(|| <DummyStats as AbstractStatistics>::throw_path("root.child", ""));
    assert_eq!(
        wrong_path_tail,
        Error::OutOfRange("bad stats access: invalid key 'root.child'".to_owned())
    );

    let not_writable =
        catch_error(|| <DummyStats as AbstractStatistics>::throw_write(7, StatisticsType::Map));
    assert_eq!(
        not_writable,
        Error::InvalidArgument("bad stats access: key '7' is not a writable map".to_owned())
    );

    let out_of_range = catch_error(|| <DummyStats as AbstractStatistics>::throw_range(5, 3));
    assert_eq!(
        out_of_range,
        Error::OutOfRange(
            "bad stats access: index '5' is out of range for object of size '3'".to_owned()
        )
    );
}
