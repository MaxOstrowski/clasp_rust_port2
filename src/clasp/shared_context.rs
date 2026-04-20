//! Partial Rust port of `original_clasp/clasp/shared_context.h` and
//! `original_clasp/src/shared_context.cpp`.
//!
//! This module currently ports the standalone `ProblemStats` aggregate and its
//! statistics-object interface. The full shared-context runtime remains blocked
//! on solver/clause integration.

use crate::clasp::statistics::{StatisticMap, StatisticObject};

const PROBLEM_STAT_KEYS: [&str; 8] = [
    "vars",
    "vars_eliminated",
    "vars_frozen",
    "constraints",
    "constraints_binary",
    "constraints_ternary",
    "acyc_edges",
    "complexity",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemVarStats {
    pub num: u32,
    pub eliminated: u32,
    pub frozen: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemConstraintStats {
    pub other: u32,
    pub binary: u32,
    pub ternary: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemStats {
    pub vars: ProblemVarStats,
    pub constraints: ProblemConstraintStats,
    pub acyc_edges: u32,
    pub complexity: u32,
}

impl ProblemStats {
    pub const fn num_constraints(&self) -> u32 {
        self.constraints.other + self.constraints.binary + self.constraints.ternary
    }

    pub fn diff(&mut self, other: &Self) {
        self.vars.num = self.vars.num.abs_diff(other.vars.num);
        self.vars.eliminated = self.vars.eliminated.abs_diff(other.vars.eliminated);
        self.vars.frozen = self.vars.frozen.abs_diff(other.vars.frozen);
        self.constraints.other = self.constraints.other.abs_diff(other.constraints.other);
        self.constraints.binary = self.constraints.binary.abs_diff(other.constraints.binary);
        self.constraints.ternary = self.constraints.ternary.abs_diff(other.constraints.ternary);
        self.acyc_edges = self.acyc_edges.abs_diff(other.acyc_edges);
    }

    pub fn accu(&mut self, other: &Self) {
        self.vars.num += other.vars.num;
        self.vars.eliminated += other.vars.eliminated;
        self.vars.frozen += other.vars.frozen;
        self.constraints.other += other.constraints.other;
        self.constraints.binary += other.constraints.binary;
        self.constraints.ternary += other.constraints.ternary;
        self.acyc_edges += other.acyc_edges;
    }

    pub const fn size() -> u32 {
        PROBLEM_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        PROBLEM_STAT_KEYS
            .get(index as usize)
            .copied()
            .expect("problem statistic key index out of bounds")
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "vars" => StatisticObject::from_value(&self.vars.num),
            "vars_eliminated" => StatisticObject::from_value(&self.vars.eliminated),
            "vars_frozen" => StatisticObject::from_value(&self.vars.frozen),
            "constraints" => StatisticObject::from_value(&self.constraints.other),
            "constraints_binary" => StatisticObject::from_value(&self.constraints.binary),
            "constraints_ternary" => StatisticObject::from_value(&self.constraints.ternary),
            "acyc_edges" => StatisticObject::from_value(&self.acyc_edges),
            "complexity" => StatisticObject::from_value(&self.complexity),
            _ => panic!("unknown ProblemStats key: {key}"),
        }
    }
}

impl StatisticMap for ProblemStats {
    fn size(&self) -> u32 {
        Self::size()
    }

    fn key(&self, index: u32) -> &str {
        Self::key(index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        Self::at(self, key)
    }
}
