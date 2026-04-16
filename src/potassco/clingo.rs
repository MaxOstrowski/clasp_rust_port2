//! Rust port of original_clasp/libpotassco/potassco/clingo.h and
//! original_clasp/libpotassco/src/clingo.cpp.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};
use std::panic::panic_any;

use crate::potassco::basic_types::{
    Id, Lit, LitSpan, TruthValue, Weight, WeightLit, WeightLitSpan,
};
use crate::potassco::enums::{EnumMetadata, EnumTag, HasEnumEntries, make_entries};
use crate::potassco::error::Error;

pub type ChangeList<'a> = LitSpan<'a>;
pub type StatisticsKey = u64;

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ClauseType {
    Learnt = 0,
    Locked = 1,
    Transient = 2,
    TransientLocked = 3,
}

impl ClauseType {
    fn from_bits(bits: u32) -> Self {
        match bits & 0b11 {
            0 => Self::Learnt,
            1 => Self::Locked,
            2 => Self::Transient,
            3 => Self::TransientLocked,
            _ => unreachable!(),
        }
    }
}

impl BitOr for ClauseType {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::from_bits(self as u32 | rhs as u32)
    }
}

impl BitOrAssign for ClauseType {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl BitAnd for ClauseType {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self::from_bits(self as u32 & rhs as u32)
    }
}

impl BitAndAssign for ClauseType {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

impl BitXor for ClauseType {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self::from_bits(self as u32 ^ rhs as u32)
    }
}

impl BitXorAssign for ClauseType {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = *self ^ rhs;
    }
}

impl Not for ClauseType {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::from_bits(!(self as u32))
    }
}

pub trait AbstractAssignment {
    fn solver_id(&self) -> Id;
    fn size(&self) -> u32;
    fn unassigned(&self) -> u32;
    fn has_conflict(&self) -> bool;
    fn level(&self) -> u32;
    fn root_level(&self) -> u32;
    fn has_lit(&self, lit: Lit) -> bool;
    fn value(&self, lit: Lit) -> TruthValue;
    fn level_of(&self, lit: Lit) -> u32;
    fn decision(&self, level: u32) -> Lit;
    fn trail_size(&self) -> u32;
    fn trail_at(&self, pos: u32) -> Lit;
    fn trail_begin(&self, level: u32) -> u32;

    fn trail_end(&self, level: u32) -> u32 {
        if level < self.level() {
            self.trail_begin(level + 1)
        } else {
            self.trail_size()
        }
    }

    fn is_total(&self) -> bool {
        self.unassigned() == 0
    }

    fn is_fixed(&self, lit: Lit) -> bool {
        self.value(lit) != TruthValue::Free && self.level_of(lit) == 0
    }

    fn is_true(&self, lit: Lit) -> bool {
        self.value(lit) == TruthValue::True
    }

    fn is_false(&self, lit: Lit) -> bool {
        self.value(lit) == TruthValue::False
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PropagatorCheckMode {
    No = 0,
    Total = 1,
    Fixpoint = 2,
    Both = 3,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PropagatorUndoMode {
    Default = 0,
    Always = 1,
}

pub trait PropagatorControl {
    fn add_clause(&mut self, clause: LitSpan<'_>, clause_type: ClauseType) -> bool;

    fn add_clause_default(&mut self, clause: LitSpan<'_>) -> bool {
        self.add_clause(clause, ClauseType::Learnt)
    }

    fn add_weight_constraint(
        &mut self,
        con: Lit,
        lits: WeightLitSpan<'_>,
        bound: Weight,
        relation: i32,
    ) -> bool;

    fn add_variable(&mut self, freeze: bool) -> Lit;

    fn add_variable_default(&mut self) -> Lit {
        self.add_variable(true)
    }

    fn propagate(&mut self) -> bool;
    fn has_watch(&self, lit: Lit) -> bool;
    fn add_watch(&mut self, lit: Lit);
    fn remove_watch(&mut self, lit: Lit);
}

pub trait PropagatorInit: PropagatorControl {
    fn check_mode(&self) -> PropagatorCheckMode;
    fn undo_mode(&self) -> PropagatorUndoMode;
    fn num_solver(&self) -> u32;
    fn solver_literal(&self, lit: Lit) -> Lit;
    fn set_check_mode(&mut self, mode: PropagatorCheckMode);
    fn set_undo_mode(&mut self, mode: PropagatorUndoMode);
    fn freeze_variable(&mut self, lit: Lit);
    fn add_minimize(&mut self, priority: Weight, lit: WeightLit);
}

pub trait AbstractPropagator {
    fn init(&mut self, assignment: &dyn AbstractAssignment, init: &mut dyn PropagatorInit);
    fn attach(&mut self, assignment: &dyn AbstractAssignment, ctrl: &mut dyn PropagatorControl);
    fn propagate(
        &mut self,
        assignment: &dyn AbstractAssignment,
        ctrl: &mut dyn PropagatorControl,
        changes: ChangeList<'_>,
    );
    fn check(&mut self, assignment: &dyn AbstractAssignment, ctrl: &mut dyn PropagatorControl);
    fn undo(&mut self, assignment: &dyn AbstractAssignment, undo: LitSpan<'_>);
}

pub trait AbstractHeuristic {
    fn decide(&mut self, assignment: &dyn AbstractAssignment, fallback: Lit) -> Lit;
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StatisticsType {
    Value = 0,
    Array = 1,
    Map = 2,
}

impl EnumTag for StatisticsType {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Value),
            1 => Some(Self::Array),
            2 => Some(Self::Map),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(make_entries(&[
            (StatisticsType::Value, "value"),
            (StatisticsType::Array, "array"),
            (StatisticsType::Map, "map"),
        ])))
    }
}

impl HasEnumEntries for StatisticsType {
    fn entries_metadata() -> crate::potassco::enums::EnumEntries<Self> {
        make_entries(&[
            (StatisticsType::Value, "value"),
            (StatisticsType::Array, "array"),
            (StatisticsType::Map, "map"),
        ])
    }
}

fn quoted(value: impl core::fmt::Display) -> String {
    format!("'{value}'")
}

fn stats_message(parts: &[String]) -> String {
    format!("bad stats access: {}", parts.join(" "))
}

pub trait AbstractStatistics {
    fn root(&self) -> StatisticsKey;
    fn type_of(&self, key: StatisticsKey) -> StatisticsType;
    fn size(&self, key: StatisticsKey) -> usize;
    fn writable(&self, key: StatisticsKey) -> bool;

    fn at(&self, array: StatisticsKey, index: usize) -> StatisticsKey;
    fn push(&mut self, array: StatisticsKey, item_type: StatisticsType) -> StatisticsKey;

    fn key(&self, map: StatisticsKey, index: usize) -> &str;
    fn get(&self, map: StatisticsKey, at: &str) -> StatisticsKey;
    fn find(&self, map: StatisticsKey, element: &str, out_key: Option<&mut StatisticsKey>) -> bool;
    fn add(&mut self, map: StatisticsKey, name: &str, item_type: StatisticsType) -> StatisticsKey;

    fn value(&self, key: StatisticsKey) -> f64;
    fn set(&mut self, key: StatisticsKey, value: f64);

    fn throw_type(expected: StatisticsType, got: StatisticsType) -> !
    where
        Self: Sized,
    {
        panic_any(Error::InvalidArgument(stats_message(&[
            quoted(expected.name().unwrap_or("")),
            "expected but got".to_owned(),
            quoted(got.name().unwrap_or("")),
        ])))
    }

    fn throw_key(key: StatisticsKey) -> !
    where
        Self: Sized,
    {
        panic_any(Error::InvalidArgument(stats_message(&[
            "invalid key".to_owned(),
            quoted(key),
        ])))
    }

    fn throw_path(path: &str, at: &str) -> !
    where
        Self: Sized,
    {
        let error = if !path.is_empty() && !at.is_empty() {
            Error::OutOfRange(stats_message(&[
                "invalid key".to_owned(),
                quoted(at),
                "in path".to_owned(),
                quoted(path),
            ]))
        } else {
            let target = if at.is_empty() { path } else { at };
            Error::OutOfRange(stats_message(&["invalid key".to_owned(), quoted(target)]))
        };
        panic_any(error)
    }

    fn throw_write(key: StatisticsKey, item_type: StatisticsType) -> !
    where
        Self: Sized,
    {
        panic_any(Error::InvalidArgument(stats_message(&[
            "key".to_owned(),
            quoted(key),
            "is not a writable".to_owned(),
            item_type.name().unwrap_or("").to_owned(),
        ])))
    }

    fn throw_range(index: usize, size: usize) -> !
    where
        Self: Sized,
    {
        panic_any(Error::OutOfRange(stats_message(&[
            "index".to_owned(),
            quoted(index),
            "is out of range for object of size".to_owned(),
            quoted(size),
        ])))
    }
}
