//! Partial Rust port of `original_clasp/clasp/solver_types.h` and
//! `original_clasp/src/solver_types.cpp`.
//!
//! This module currently ports the solver statistics types together with the
//! low-level watch, reason-store, value-set, assignment, and implied-literal
//! storage helpers from the upstream `solver_types` unit. Clause storage,
//! allocator/runtime pieces, and solver-bound reassignment remain blocked on
//! the still-incomplete solver and clause integration.

use core::ops::{Index, IndexMut};
use core::ptr::NonNull;

use crate::clasp::constraint::{
    Antecedent, ClauseHead, Constraint, ConstraintType, PropResult, Solver,
};
use crate::clasp::literal::{
    LitVec, Literal, ValT, ValueVec, Var_t, VarVec, true_value, value_free, value_true,
};
use crate::clasp::pod_vector::{PodVectorT, VectorLike, size32};
pub use crate::clasp::statistics::{
    StatisticArray, StatisticMap, StatisticObject, StatisticType, StatisticValue,
};
use crate::clasp::util::left_right_sequence::LeftRightSequence;
use crate::clasp::util::misc_types::ratio;
use crate::potassco::bits::{
    right_most_bit, store_clear_mask, store_set_mask, test_any, test_mask,
};

const CORE_STAT_KEYS: [&str; 6] = [
    "choices",
    "conflicts",
    "conflicts_analyzed",
    "restarts",
    "restarts_last",
    "restarts_blocked",
];

const JUMP_STAT_KEYS: [&str; 7] = [
    "jumps",
    "jumps_bounded",
    "levels",
    "levels_bounded",
    "max",
    "max_executed",
    "max_bounded",
];

const EXTENDED_STAT_KEYS: [&str; 23] = [
    "domain_choices",
    "models",
    "models_level",
    "hcc_tests",
    "hcc_partial",
    "lemmas_deleted",
    "distributed",
    "distributed_sum_lbd",
    "integrated",
    "lemmas",
    "lits_learnt",
    "lemmas_binary",
    "lemmas_ternary",
    "cpu_time",
    "integrated_imps",
    "integrated_jumps",
    "guiding_paths_lits",
    "guiding_paths",
    "splits",
    "lemmas_conflict",
    "lemmas_loop",
    "lemmas_other",
    "jumps",
];

fn sum_u64_array(values: &[u64; 3]) -> f64 {
    values.iter().copied().sum::<u64>() as f64
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoreStats {
    pub choices: u64,
    pub conflicts: u64,
    pub analyzed: u64,
    pub restarts: u64,
    pub last_restart: u64,
    pub bl_restarts: u64,
}

impl CoreStats {
    pub const fn backtracks(&self) -> u64 {
        self.conflicts - self.analyzed
    }

    pub const fn backjumps(&self) -> u64 {
        self.analyzed
    }

    pub fn avg_restart(&self) -> f64 {
        ratio(self.analyzed, self.restarts)
    }

    pub const fn size() -> u32 {
        CORE_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        CORE_STAT_KEYS
            .get(index as usize)
            .copied()
            .expect("core statistic key index out of bounds")
    }

    pub fn accu(&mut self, other: &Self) {
        self.choices += other.choices;
        self.conflicts += other.conflicts;
        self.analyzed += other.analyzed;
        self.restarts += other.restarts;
        self.last_restart = self.last_restart.max(other.last_restart);
        self.bl_restarts = self.bl_restarts.max(other.bl_restarts);
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "choices" => StatisticObject::from_value(&self.choices),
            "conflicts" => StatisticObject::from_value(&self.conflicts),
            "conflicts_analyzed" => StatisticObject::from_value(&self.analyzed),
            "restarts" => StatisticObject::from_value(&self.restarts),
            "restarts_last" => StatisticObject::from_value(&self.last_restart),
            "restarts_blocked" => StatisticObject::from_value(&self.bl_restarts),
            _ => panic!("unknown CoreStats key: {key}"),
        }
    }
}

impl StatisticMap for CoreStats {
    fn size(&self) -> u32 {
        CoreStats::size()
    }

    fn key(&self, index: u32) -> &str {
        CoreStats::key(index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        CoreStats::at(self, key)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JumpStats {
    pub jumps: u64,
    pub bounded: u64,
    pub jump_sum: u64,
    pub bound_sum: u64,
    pub max_jump: u32,
    pub max_jump_ex: u32,
    pub max_bound: u32,
}

impl JumpStats {
    pub fn update(&mut self, dl: u32, uip_level: u32, b_level: u32) {
        self.jumps += 1;
        self.jump_sum += u64::from(dl - uip_level);
        self.max_jump = self.max_jump.max(dl - uip_level);
        if uip_level < b_level {
            self.bounded += 1;
            self.bound_sum += u64::from(b_level - uip_level);
            self.max_jump_ex = self.max_jump_ex.max(dl - b_level);
            self.max_bound = self.max_bound.max(b_level - uip_level);
        } else {
            self.max_jump_ex = self.max_jump;
        }
    }

    pub const fn jumped(&self) -> u64 {
        self.jump_sum - self.bound_sum
    }

    pub fn jumped_ratio(&self) -> f64 {
        ratio(self.jumped(), self.jump_sum)
    }

    pub fn avg_bound(&self) -> f64 {
        ratio(self.bound_sum, self.bounded)
    }

    pub fn avg_jump(&self) -> f64 {
        ratio(self.jump_sum, self.jumps)
    }

    pub fn avg_jump_ex(&self) -> f64 {
        ratio(self.jumped(), self.jumps)
    }

    pub fn accu(&mut self, other: &Self) {
        self.jumps += other.jumps;
        self.bounded += other.bounded;
        self.jump_sum += other.jump_sum;
        self.bound_sum += other.bound_sum;
        self.max_jump = self.max_jump.max(other.max_jump);
        self.max_jump_ex = self.max_jump_ex.max(other.max_jump_ex);
        self.max_bound = self.max_bound.max(other.max_bound);
    }

    pub const fn size() -> u32 {
        JUMP_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        JUMP_STAT_KEYS
            .get(index as usize)
            .copied()
            .expect("jump statistic key index out of bounds")
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "jumps" => StatisticObject::from_value(&self.jumps),
            "jumps_bounded" => StatisticObject::from_value(&self.bounded),
            "levels" => StatisticObject::from_value(&self.jump_sum),
            "levels_bounded" => StatisticObject::from_value(&self.bound_sum),
            "max" => StatisticObject::from_value(&self.max_jump),
            "max_executed" => StatisticObject::from_value(&self.max_jump_ex),
            "max_bounded" => StatisticObject::from_value(&self.max_bound),
            _ => panic!("unknown JumpStats key: {key}"),
        }
    }
}

impl StatisticMap for JumpStats {
    fn size(&self) -> u32 {
        JumpStats::size()
    }

    fn key(&self, index: u32) -> &str {
        JumpStats::key(index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        JumpStats::at(self, key)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtendedStats {
    pub dom_choices: u64,
    pub models: u64,
    pub model_lits: u64,
    pub hcc_tests: u64,
    pub hcc_partial: u64,
    pub deleted: u64,
    pub distributed: u64,
    pub sum_dist_lbd: u64,
    pub integrated: u64,
    pub learnts: [u64; 3],
    pub lits: [u64; 3],
    pub binary: u32,
    pub ternary: u32,
    pub cpu_time: f64,
    pub int_imps: u64,
    pub int_jumps: u64,
    pub gp_lits: u64,
    pub gps: u32,
    pub splits: u32,
    pub jumps: JumpStats,
}

impl Default for ExtendedStats {
    fn default() -> Self {
        Self {
            dom_choices: 0,
            models: 0,
            model_lits: 0,
            hcc_tests: 0,
            hcc_partial: 0,
            deleted: 0,
            distributed: 0,
            sum_dist_lbd: 0,
            integrated: 0,
            learnts: [0; 3],
            lits: [0; 3],
            binary: 0,
            ternary: 0,
            cpu_time: 0.0,
            int_imps: 0,
            int_jumps: 0,
            gp_lits: 0,
            gps: 0,
            splits: 0,
            jumps: JumpStats::default(),
        }
    }
}

impl ExtendedStats {
    pub const fn index(kind: ConstraintType) -> Option<usize> {
        match kind {
            ConstraintType::Static => None,
            ConstraintType::Conflict => Some(0),
            ConstraintType::Loop => Some(1),
            ConstraintType::Other => Some(2),
        }
    }

    pub fn sum(values: [u64; 3]) -> u64 {
        values.into_iter().sum()
    }

    pub fn add_learnt(&mut self, size: u32, kind: ConstraintType) {
        if let Some(index) = Self::index(kind) {
            self.learnts[index] += 1;
            self.lits[index] += u64::from(size);
            self.binary += u32::from(size == 2);
            self.ternary += u32::from(size == 3);
        }
    }

    pub fn lemmas(&self) -> u64 {
        Self::sum(self.learnts)
    }

    pub fn learnt_lits(&self) -> u64 {
        Self::sum(self.lits)
    }

    pub fn lemmas_of(&self, kind: ConstraintType) -> u64 {
        Self::index(kind).map_or(0, |index| self.learnts[index])
    }

    pub fn avg_len(&self, kind: ConstraintType) -> f64 {
        if let Some(index) = Self::index(kind) {
            ratio(self.lits[index], self.learnts[index])
        } else {
            0.0
        }
    }

    pub fn avg_model(&self) -> f64 {
        ratio(self.model_lits, self.models)
    }

    pub fn dist_ratio(&self) -> f64 {
        ratio(self.distributed, self.learnts[0] + self.learnts[1])
    }

    pub fn avg_dist_lbd(&self) -> f64 {
        ratio(self.sum_dist_lbd, self.distributed)
    }

    pub fn avg_int_jump(&self) -> f64 {
        ratio(self.int_jumps, self.int_imps)
    }

    pub fn avg_gp(&self) -> f64 {
        ratio(self.gp_lits, u64::from(self.gps))
    }

    pub fn int_ratio(&self) -> f64 {
        ratio(self.integrated, self.distributed)
    }

    pub fn accu(&mut self, other: &Self) {
        self.dom_choices += other.dom_choices;
        self.models += other.models;
        self.model_lits += other.model_lits;
        self.hcc_tests += other.hcc_tests;
        self.hcc_partial += other.hcc_partial;
        self.deleted += other.deleted;
        self.distributed += other.distributed;
        self.sum_dist_lbd += other.sum_dist_lbd;
        self.integrated += other.integrated;
        for index in 0..self.learnts.len() {
            self.learnts[index] += other.learnts[index];
            self.lits[index] += other.lits[index];
        }
        self.binary += other.binary;
        self.ternary += other.ternary;
        self.cpu_time += other.cpu_time;
        self.int_imps += other.int_imps;
        self.int_jumps += other.int_jumps;
        self.gp_lits += other.gp_lits;
        self.gps += other.gps;
        self.splits += other.splits;
        self.jumps.accu(&other.jumps);
    }

    pub const fn size() -> u32 {
        EXTENDED_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        EXTENDED_STAT_KEYS
            .get(index as usize)
            .copied()
            .expect("extended statistic key index out of bounds")
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "domain_choices" => StatisticObject::from_value(&self.dom_choices),
            "models" => StatisticObject::from_value(&self.models),
            "models_level" => StatisticObject::from_value(&self.model_lits),
            "hcc_tests" => StatisticObject::from_value(&self.hcc_tests),
            "hcc_partial" => StatisticObject::from_value(&self.hcc_partial),
            "lemmas_deleted" => StatisticObject::from_value(&self.deleted),
            "distributed" => StatisticObject::from_value(&self.distributed),
            "distributed_sum_lbd" => StatisticObject::from_value(&self.sum_dist_lbd),
            "integrated" => StatisticObject::from_value(&self.integrated),
            "lemmas" => StatisticObject::from_mapped_value(&self.learnts, sum_u64_array),
            "lits_learnt" => StatisticObject::from_mapped_value(&self.lits, sum_u64_array),
            "lemmas_binary" => StatisticObject::from_value(&self.binary),
            "lemmas_ternary" => StatisticObject::from_value(&self.ternary),
            "cpu_time" => StatisticObject::from_value(&self.cpu_time),
            "integrated_imps" => StatisticObject::from_value(&self.int_imps),
            "integrated_jumps" => StatisticObject::from_value(&self.int_jumps),
            "guiding_paths_lits" => StatisticObject::from_value(&self.gp_lits),
            "guiding_paths" => StatisticObject::from_value(&self.gps),
            "splits" => StatisticObject::from_value(&self.splits),
            "lemmas_conflict" => StatisticObject::from_value(&self.learnts[0]),
            "lemmas_loop" => StatisticObject::from_value(&self.learnts[1]),
            "lemmas_other" => StatisticObject::from_value(&self.learnts[2]),
            "jumps" => StatisticObject::map(&self.jumps),
            _ => panic!("unknown ExtendedStats key: {key}"),
        }
    }
}

impl StatisticMap for ExtendedStats {
    fn size(&self) -> u32 {
        ExtendedStats::size()
    }

    fn key(&self, index: u32) -> &str {
        ExtendedStats::key(index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        ExtendedStats::at(self, key)
    }
}

#[derive(Debug, Default)]
pub struct SolverStats {
    pub core: CoreStats,
    pub extra: Option<Box<ExtendedStats>>,
    pub multi: Option<NonNull<SolverStats>>,
}

impl Clone for SolverStats {
    fn clone(&self) -> Self {
        let mut clone = Self {
            core: self.core,
            extra: None,
            multi: None,
        };
        if self.extra.is_some() {
            let _ = clone.enable_extended();
            if let (Some(dst), Some(src)) = (clone.extra.as_mut(), self.extra.as_ref()) {
                dst.accu(src);
            }
        }
        clone
    }
}

impl SolverStats {
    pub fn enable_extended(&mut self) -> bool {
        if self.extra.is_none() {
            self.extra = Some(Box::new(ExtendedStats::default()));
        }
        true
    }

    pub fn enable(&mut self, other: &Self) -> bool {
        other.extra.is_none() || self.enable_extended()
    }

    pub fn reset(&mut self) {
        self.core = CoreStats::default();
        if let Some(extra) = self.extra.as_mut() {
            **extra = ExtendedStats::default();
        }
    }

    pub fn accu(&mut self, other: &Self) {
        self.core.accu(&other.core);
        if let (Some(extra), Some(other_extra)) = (self.extra.as_mut(), other.extra.as_ref()) {
            extra.accu(other_extra);
        }
    }

    pub fn accu_with_enable(&mut self, other: &Self, enable_rhs: bool) {
        if enable_rhs {
            let _ = self.enable(other);
        }
        self.accu(other);
    }

    pub fn flush(&self) {
        if let Some(mut multi) = self.multi {
            let multi = unsafe { multi.as_mut() };
            let _ = multi.enable(self);
            multi.accu(self);
            multi.flush();
        }
    }

    pub fn swap_stats(&mut self, other: &mut Self) {
        core::mem::swap(&mut self.core, &mut other.core);
        core::mem::swap(&mut self.extra, &mut other.extra);
    }

    pub const fn size(&self) -> u32 {
        CoreStats::size() + self.extra.is_some() as u32
    }

    pub fn key(&self, index: u32) -> &str {
        if index < CoreStats::size() {
            CoreStats::key(index)
        } else if index == CoreStats::size() && self.extra.is_some() {
            "extra"
        } else {
            panic!("solver statistic key index out of bounds")
        }
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        if key == "extra" {
            let extra = self
                .extra
                .as_deref()
                .expect("requested extra stats but no extended statistics are enabled");
            StatisticObject::map(extra)
        } else {
            self.core.at(key)
        }
    }

    pub fn add_learnt(&mut self, size: u32, kind: ConstraintType) {
        if let Some(extra) = self.extra.as_mut() {
            extra.add_learnt(size, kind);
        }
    }

    pub fn add_conflict(&mut self, dl: u32, uip_level: u32, b_level: u32, _lbd: u32) {
        self.core.analyzed += 1;
        if let Some(extra) = self.extra.as_mut() {
            extra.jumps.update(dl, uip_level, b_level);
        }
    }

    pub fn add_deleted(&mut self, num: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.deleted += u64::from(num);
        }
    }

    pub fn add_distributed(&mut self, lbd: u32, _kind: ConstraintType) {
        if let Some(extra) = self.extra.as_mut() {
            extra.distributed += 1;
            extra.sum_dist_lbd += u64::from(lbd);
        }
    }

    pub fn add_test(&mut self, partial: bool) {
        if let Some(extra) = self.extra.as_mut() {
            extra.hcc_tests += 1;
            extra.hcc_partial += u64::from(partial);
        }
    }

    pub fn add_model(&mut self, dl: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.models += 1;
            extra.model_lits += u64::from(dl);
        }
    }

    pub fn add_cpu_time(&mut self, time: f64) {
        if let Some(extra) = self.extra.as_mut() {
            extra.cpu_time += time;
        }
    }

    pub fn add_split(&mut self, count: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.splits += count;
        }
    }

    pub fn add_dom_choice(&mut self, count: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.dom_choices += u64::from(count);
        }
    }

    pub fn add_integrated_asserting(&mut self, start_level: u32, jump_level: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.int_imps += 1;
            extra.int_jumps += u64::from(start_level - jump_level);
        }
    }

    pub fn add_integrated(&mut self, count: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.integrated += u64::from(count);
        }
    }

    pub fn remove_integrated(&mut self, count: u32) {
        if let Some(extra) = self.extra.as_mut() {
            extra.integrated -= u64::from(count);
        }
    }

    pub fn add_path(&mut self, size: usize) {
        if let Some(extra) = self.extra.as_mut() {
            extra.gps += 1;
            extra.gp_lits += size as u64;
        }
    }

    pub fn set_multi(&mut self, other: &mut SolverStats) {
        self.multi = Some(NonNull::from(other));
    }
}

#[allow(non_snake_case)]
impl SolverStats {
    pub fn enableExtended(&mut self) -> bool {
        self.enable_extended()
    }

    pub fn swapStats(&mut self, other: &mut Self) {
        self.swap_stats(other)
    }

    pub fn addLearnt(&mut self, size: u32, kind: ConstraintType) {
        self.add_learnt(size, kind)
    }

    pub fn addConflict(&mut self, dl: u32, uip_level: u32, b_level: u32, lbd: u32) {
        self.add_conflict(dl, uip_level, b_level, lbd)
    }

    pub fn addDeleted(&mut self, num: u32) {
        self.add_deleted(num)
    }

    pub fn addDistributed(&mut self, lbd: u32, kind: ConstraintType) {
        self.add_distributed(lbd, kind)
    }

    pub fn addTest(&mut self, partial: bool) {
        self.add_test(partial)
    }

    pub fn addModel(&mut self, dl: u32) {
        self.add_model(dl)
    }

    pub fn addCpuTime(&mut self, time: f64) {
        self.add_cpu_time(time)
    }

    pub fn addSplit(&mut self, count: u32) {
        self.add_split(count)
    }

    pub fn addDomChoice(&mut self, count: u32) {
        self.add_dom_choice(count)
    }

    pub fn addIntegratedAsserting(&mut self, start_level: u32, jump_level: u32) {
        self.add_integrated_asserting(start_level, jump_level)
    }

    pub fn addIntegrated(&mut self, count: u32) {
        self.add_integrated(count)
    }

    pub fn removeIntegrated(&mut self, count: u32) {
        self.remove_integrated(count)
    }

    pub fn addPath(&mut self, size: usize) {
        self.add_path(size)
    }
}

impl StatisticMap for SolverStats {
    fn size(&self) -> u32 {
        SolverStats::size(self)
    }

    fn key(&self, index: u32) -> &str {
        SolverStats::key(self, index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        SolverStats::at(self, key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClauseWatch {
    pub head: *mut ClauseHead,
}

impl ClauseWatch {
    pub const fn new(head: *mut ClauseHead) -> Self {
        Self { head }
    }

    pub const fn eq_head(head: *mut ClauseHead) -> ClauseWatchEqHead {
        ClauseWatchEqHead::new(head)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClauseWatchEqHead {
    pub head: *mut ClauseHead,
}

impl ClauseWatchEqHead {
    pub const fn new(head: *mut ClauseHead) -> Self {
        Self { head }
    }

    pub fn matches(&self, watch: &ClauseWatch) -> bool {
        self.head == watch.head
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericWatch {
    pub con: *mut Constraint,
    pub data: u32,
}

impl GenericWatch {
    pub const fn new(con: *mut Constraint, data: u32) -> Self {
        Self { con, data }
    }

    pub const fn eq_constraint(con: *mut Constraint) -> GenericWatchEqConstraint {
        GenericWatchEqConstraint::new(con)
    }

    pub fn propagate(&mut self, solver: &mut Solver, literal: Literal) -> PropResult {
        let constraint = unsafe { self.con.as_mut() }
            .expect("generic watch requires a non-null constraint pointer");
        constraint.propagate(solver, literal, &mut self.data)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericWatchEqConstraint {
    pub con: *mut Constraint,
}

impl GenericWatchEqConstraint {
    pub const fn new(con: *mut Constraint) -> Self {
        Self { con }
    }

    pub fn matches(&self, watch: &GenericWatch) -> bool {
        self.con == watch.con
    }
}

pub type WatchList = LeftRightSequence<ClauseWatch, GenericWatch, 0>;

pub fn release_vec(watches: &mut WatchList) {
    watches.reset();
}

#[allow(non_snake_case)]
pub fn releaseVec(watches: &mut WatchList) {
    release_vec(watches);
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasonStore32 {
    entries: PodVectorT<Antecedent>,
}

impl ReasonStore32 {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn reserve(&mut self, count: usize) {
        self.entries.reserve(count);
    }

    pub fn resize(&mut self, new_len: usize) {
        self.entries.resize(new_len, Antecedent::new());
    }

    pub fn truncate(&mut self, new_len: usize) {
        self.entries.truncate(new_len);
    }

    pub fn push_back(&mut self, antecedent: Antecedent) {
        self.entries.push_back(antecedent);
    }

    pub fn data(&self, var: u32) -> u32 {
        Self::decode(&self.entries[var as usize])
    }

    pub fn set_data(&mut self, var: u32, data: u32) {
        Self::encode(&mut self.entries[var as usize], data);
    }

    pub fn encode(antecedent: &mut Antecedent, data: u32) {
        *antecedent.as_u64_mut() =
            (u64::from(data) << 32) | u64::from(*antecedent.as_u64_mut() as u32);
    }

    pub fn decode(antecedent: &Antecedent) -> u32 {
        (antecedent.as_u64() >> 32) as u32
    }
}

impl Index<usize> for ReasonStore32 {
    type Output = Antecedent;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for ReasonStore32 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasonStore32Value {
    antecedent: Antecedent,
}

impl ReasonStore32Value {
    pub fn new(antecedent: Antecedent, data: u32) -> Self {
        let mut value = antecedent;
        if data != u32::MAX {
            ReasonStore32::encode(&mut value, data);
            assert_eq!(value.type_(), Antecedent::GENERIC);
        }
        Self { antecedent: value }
    }

    pub const fn ante(&self) -> Antecedent {
        self.antecedent
    }

    pub fn data(&self) -> u32 {
        if self.antecedent.type_() == Antecedent::GENERIC {
            ReasonStore32::decode(&self.antecedent)
        } else {
            u32::MAX
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasonStore64 {
    entries: PodVectorT<Antecedent>,
    pub dv: VarVec,
}

impl ReasonStore64 {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn reserve(&mut self, count: usize) {
        self.entries.reserve(count);
    }

    pub fn resize(&mut self, new_len: usize) {
        self.entries.resize(new_len, Antecedent::new());
    }

    pub fn truncate(&mut self, new_len: usize) {
        self.entries.truncate(new_len);
        self.dv.truncate(new_len);
    }

    pub fn push_back(&mut self, antecedent: Antecedent) {
        self.entries.push_back(antecedent);
    }

    pub fn data_size(&self) -> u32 {
        size32(&self.dv)
    }

    pub fn data_resize(&mut self, new_len: u32) {
        if new_len > self.data_size() {
            self.dv.resize(new_len as usize, u32::MAX);
        }
    }

    pub fn data(&self, var: u32) -> u32 {
        if var < self.data_size() {
            self.dv[var as usize]
        } else {
            u32::MAX
        }
    }

    pub fn set_data(&mut self, var: u32, data: u32) {
        self.data_resize(var + 1);
        self.dv[var as usize] = data;
    }
}

impl Index<usize> for ReasonStore64 {
    type Output = Antecedent;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for ReasonStore64 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasonStore64Value {
    antecedent: Antecedent,
    data: u32,
}

impl ReasonStore64Value {
    pub const fn new(antecedent: Antecedent, data: u32) -> Self {
        Self { antecedent, data }
    }

    pub const fn ante(&self) -> Antecedent {
        self.antecedent
    }

    pub const fn data(&self) -> u32 {
        self.data
    }
}

#[cfg(target_pointer_width = "64")]
pub type ReasonVec = ReasonStore64;
#[cfg(target_pointer_width = "32")]
pub type ReasonVec = ReasonStore32;

#[cfg(target_pointer_width = "64")]
pub type ReasonWithData = ReasonStore64Value;
#[cfg(target_pointer_width = "32")]
pub type ReasonWithData = ReasonStore32Value;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValueSet {
    pub rep: u8,
}

#[allow(non_upper_case_globals)]
impl ValueSet {
    pub const user_value: u32 = 0x03u32;
    pub const saved_value: u32 = 0x0Cu32;
    pub const pref_value: u32 = 0x30u32;
    pub const def_value: u32 = 0xC0u32;

    pub fn sign(&self) -> bool {
        test_any(right_most_bit(self.rep), 0xAAu8)
    }

    pub const fn empty(&self) -> bool {
        self.rep == 0
    }

    pub fn has(&self, value: u32) -> bool {
        test_any(u32::from(self.rep), value)
    }

    pub fn get(&self, value: u32) -> ValT {
        ((u32::from(self.rep) & value) / right_most_bit(value)) as ValT
    }

    pub fn set(&mut self, which: u32, value: ValT) {
        let mask = which as u8;
        store_clear_mask(&mut self.rep, mask);
        store_set_mask(&mut self.rep, value * right_most_bit(mask));
    }

    pub fn save(&mut self, value: ValT) {
        store_clear_mask(&mut self.rep, Self::saved_value as u8);
        store_set_mask(&mut self.rep, value << 2);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Assignment {
    pub trail: LitVec,
    pub front: u32,
    assign_: PodVectorT<u32>,
    reason_: ReasonVec,
    pref_: PodVectorT<ValueSet>,
    elims_: u32,
    units_: u32,
}

impl Assignment {
    const ELIM_MASK: u32 = 0xFFFFFFF0u32;
    const SEEN_MASK_VAR: u32 = 0b1100u32;
    const VALUE_MASK: u32 = 0b0011u32;
    const LEVEL_SHIFT: u32 = 4u32;

    pub fn q_empty(&self) -> bool {
        self.front == size32(&self.trail)
    }

    pub fn q_size(&self) -> u32 {
        size32(&self.trail) - self.front
    }

    pub fn q_pop(&mut self) -> Literal {
        let literal = self.trail[self.front as usize];
        self.front += 1;
        literal
    }

    pub fn q_reset(&mut self) {
        self.front = size32(&self.trail);
    }

    pub fn num_vars(&self) -> u32 {
        size32(&self.assign_)
    }

    pub fn assigned(&self) -> u32 {
        size32(&self.trail)
    }

    pub fn free(&self) -> u32 {
        self.num_vars() - (self.assigned() + self.elims_)
    }

    pub const fn max_level(&self) -> u32 {
        (1u32 << 28) - 2
    }

    pub fn value(&self, var: Var_t) -> ValT {
        (self.assign_[var as usize] & Self::VALUE_MASK) as ValT
    }

    pub fn level(&self, var: Var_t) -> u32 {
        self.assign_[var as usize] >> Self::LEVEL_SHIFT
    }

    pub fn valid(&self, var: Var_t) -> bool {
        !test_mask(self.assign_[var as usize], Self::ELIM_MASK)
    }

    pub fn pref(&self, var: Var_t) -> ValueSet {
        if (var as usize) < self.pref_.len() {
            self.pref_[var as usize]
        } else {
            ValueSet::default()
        }
    }

    pub fn reason(&self, var: Var_t) -> &Antecedent {
        &self.reason_[var as usize]
    }

    pub fn data(&self, var: Var_t) -> u32 {
        self.reason_.data(var)
    }

    pub fn reserve(&mut self, count: u32) {
        self.assign_.reserve(count as usize);
        self.reason_.reserve(count as usize);
    }

    pub fn ensure_var(&mut self, var: Var_t) {
        let needed = var as usize + 1;
        if self.assign_.len() < needed {
            self.resize(var + 1);
        }
    }

    pub fn resize(&mut self, new_vars: u32) {
        self.assign_.resize(new_vars as usize, 0);
        self.reason_.resize(new_vars as usize);
    }

    pub fn truncate_vars(&mut self, new_vars: u32) {
        let new_len = new_vars as usize + 1;
        let old_trail = self.trail.as_slice().to_vec();
        let front_prefix = old_trail[..(self.front as usize).min(old_trail.len())]
            .iter()
            .filter(|lit| lit.var() <= new_vars)
            .count() as u32;
        let units_prefix = old_trail[..(self.units_ as usize).min(old_trail.len())]
            .iter()
            .filter(|lit| lit.var() <= new_vars)
            .count() as u32;
        let kept: Vec<Literal> = old_trail
            .into_iter()
            .filter(|lit| lit.var() <= new_vars)
            .collect();

        self.assign_.truncate(new_len);
        self.reason_.truncate(new_len);
        self.pref_.truncate(new_len);
        self.trail.assign_from_slice(&kept);
        self.front = front_prefix.min(size32(&self.trail));
        self.units_ = units_prefix.min(size32(&self.trail));
        self.elims_ = (1..=new_vars).filter(|&var| !self.valid(var)).count() as u32;
    }

    pub fn add_var(&mut self) -> Var_t {
        self.assign_.push_back(0);
        self.reason_.push_back(Antecedent::new());
        self.num_vars() - 1
    }

    pub fn set_raw_assignment(&mut self, var: Var_t, value: ValT, level: u32) {
        self.ensure_var(var);
        self.assign_[var as usize] = (level << Self::LEVEL_SHIFT) | u32::from(value);
    }

    pub fn push_trail_literal(&mut self, literal: Literal) {
        self.trail.push_back(literal);
    }

    pub fn clear_trail(&mut self) {
        self.trail.clear();
        self.front = 0;
    }

    pub fn request_prefs(&mut self) {
        if self.pref_.len() != self.assign_.len() {
            self.pref_.resize(self.assign_.len(), ValueSet::default());
        }
    }

    pub fn eliminate(&mut self, var: Var_t) {
        assert_eq!(
            self.value(var),
            value_free,
            "can not eliminate assigned var"
        );
        if self.valid(var) {
            self.assign_[var as usize] = Self::ELIM_MASK | u32::from(value_true);
            self.elims_ += 1;
        }
    }

    pub fn assign(&mut self, literal: Literal, level: u32, antecedent: Antecedent) -> bool {
        self.assign_impl(literal, level, antecedent, None)
    }

    pub fn assign_with_data(
        &mut self,
        literal: Literal,
        level: u32,
        antecedent: Antecedent,
        data: u32,
    ) -> bool {
        self.assign_impl(literal, level, antecedent, Some(data))
    }

    pub fn undo_trail(&mut self, first: usize, save: bool) {
        let stop = self.trail[first];
        if save {
            self.request_prefs();
            self.pop_until::<true>(stop);
        } else {
            self.pop_until::<false>(stop);
        }
        self.q_reset();
    }

    pub fn undo_last(&mut self) {
        let var = self.trail.back().var();
        self.clear(var);
        self.trail.pop_back();
    }

    pub fn last(&self) -> Literal {
        *self.trail.back()
    }

    pub fn last_mut(&mut self) -> &mut Literal {
        self.trail.back_mut()
    }

    pub fn units(&self) -> u32 {
        self.units_
    }

    pub fn seen_var(&self, var: Var_t) -> bool {
        test_any(self.assign_[var as usize], Self::SEEN_MASK_VAR)
    }

    pub fn seen_literal(&self, literal: Literal) -> bool {
        test_any(
            self.assign_[literal.var() as usize],
            Self::seen_mask(literal),
        )
    }

    pub fn values(&self, out: &mut ValueVec) {
        out.clear();
        out.reserve(self.assign_.len());
        for value in self.assign_.as_slice() {
            out.push_back((value & Self::VALUE_MASK) as ValT);
        }
    }

    pub fn set_seen_var(&mut self, var: Var_t) {
        store_set_mask(&mut self.assign_[var as usize], Self::SEEN_MASK_VAR);
    }

    pub fn set_seen_literal(&mut self, literal: Literal) {
        store_set_mask(
            &mut self.assign_[literal.var() as usize],
            Self::seen_mask(literal),
        );
    }

    pub fn clear_seen(&mut self, var: Var_t) {
        store_clear_mask(&mut self.assign_[var as usize], Self::SEEN_MASK_VAR);
    }

    pub fn clear_value(&mut self, var: Var_t) {
        store_clear_mask(&mut self.assign_[var as usize], Self::VALUE_MASK);
    }

    pub fn set_value(&mut self, var: Var_t, value: ValT) {
        assert!(self.value(var) == value || self.value(var) == value_free);
        self.assign_[var as usize] |= u32::from(value);
    }

    pub fn set_reason(&mut self, var: Var_t, antecedent: Antecedent) {
        self.reason_[var as usize] = antecedent;
    }

    pub fn set_data(&mut self, var: Var_t, data: u32) {
        self.reason_.set_data(var, data);
    }

    pub fn set_pref(&mut self, var: Var_t, which: u32, value: ValT) {
        self.pref_[var as usize].set(which, value);
    }

    pub fn mark_units(&mut self) -> bool {
        while self.units_ != self.front {
            let var = self.trail[self.units_ as usize].var();
            self.set_seen_var(var);
            self.units_ += 1;
        }
        true
    }

    pub fn set_units(&mut self, units: u32) {
        self.units_ = units;
    }

    pub fn reset_prefs(&mut self) {
        let len = self.pref_.len();
        self.pref_.assign_fill(len, ValueSet::default());
    }

    pub fn clear(&mut self, var: Var_t) {
        self.assign_[var as usize] = 0;
    }

    fn assign_impl(
        &mut self,
        literal: Literal,
        level: u32,
        antecedent: Antecedent,
        data: Option<u32>,
    ) -> bool {
        let var = literal.var();
        let current = self.value(var);
        if current == value_free {
            assert!(self.valid(var));
            self.assign_[var as usize] =
                (level << Self::LEVEL_SHIFT) + u32::from(true_value(literal));
            self.reason_[var as usize] = antecedent;
            if let Some(data) = data {
                self.reason_.set_data(var, data);
            }
            self.trail.push_back(literal);
            true
        } else {
            current == true_value(literal)
        }
    }

    fn pop_until<const SAVE_VALUE: bool>(&mut self, stop: Literal) {
        loop {
            let literal = *self.trail.back();
            self.trail.pop_back();
            let var = literal.var();
            if SAVE_VALUE {
                let value = self.value(var);
                self.pref_[var as usize].save(value);
            }
            self.clear(var);
            if literal == stop {
                break;
            }
        }
    }

    fn seen_mask(literal: Literal) -> u32 {
        u32::from(true_value(literal)) << 2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImpliedLiteral {
    pub lit: Literal,
    pub level: u32,
    pub ante: ReasonWithData,
}

impl ImpliedLiteral {
    pub fn new(lit: Literal, level: u32, antecedent: Antecedent) -> Self {
        Self::with_data(lit, level, antecedent, u32::MAX)
    }

    pub fn with_data(lit: Literal, level: u32, antecedent: Antecedent, data: u32) -> Self {
        Self {
            lit,
            level,
            ante: ReasonWithData::new(antecedent, data),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ImpliedList {
    pub lits: PodVectorT<ImpliedLiteral>,
    pub level: u32,
    pub front: u32,
}

impl ImpliedList {
    pub fn len(&self) -> usize {
        self.lits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lits.is_empty()
    }

    pub fn find(&mut self, literal: Literal) -> Option<&mut ImpliedLiteral> {
        self.lits
            .as_mut_slice()
            .iter_mut()
            .find(|entry| entry.lit == literal)
    }

    pub fn add(&mut self, dl: u32, literal: ImpliedLiteral) {
        if dl > self.level {
            self.level = dl;
        }
        self.lits.push_back(literal);
    }

    pub fn assign(&mut self, solver: &mut Solver) -> bool {
        assert!(self.front as usize <= self.lits.len());
        let dl = solver.decision_level();
        let mut ok = !solver.has_conflict();
        let start = self.front as usize;
        let pending: Vec<ImpliedLiteral> = self.lits.as_slice()[start..].to_vec();
        let mut write = start;

        for implied in pending {
            if implied.level <= dl {
                if ok {
                    ok = solver.force_with_data(
                        implied.lit,
                        implied.ante.ante(),
                        implied.ante.data(),
                    );
                }
                if implied.level < dl || implied.ante.ante().is_null() {
                    self.lits.as_mut_slice()[write] = implied;
                    write += 1;
                }
            }
        }

        self.lits.truncate(write);
        self.level = dl * u32::from(!self.lits.is_empty());
        self.front = if self.level > solver.root_level() {
            self.front
        } else {
            size32(&self.lits)
        };
        ok
    }

    pub fn active(&self, dl: u32) -> bool {
        dl < self.level && self.front as usize != self.lits.len()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, ImpliedLiteral> {
        self.lits.as_slice().iter()
    }
}

impl<'a> IntoIterator for &'a ImpliedList {
    type Item = &'a ImpliedLiteral;
    type IntoIter = core::slice::Iter<'a, ImpliedLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
