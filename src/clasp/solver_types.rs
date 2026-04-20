//! Partial Rust port of `original_clasp/clasp/solver_types.h` and
//! `original_clasp/src/solver_types.cpp`.
//!
//! This module currently ports the solver statistics types from the upstream
//! `solver_types` unit. Clause storage and allocator/runtime pieces remain
//! blocked on the still-incomplete solver and clause integration.

use core::ptr::NonNull;

use crate::clasp::constraint::ConstraintType;
pub use crate::clasp::statistics::{
    StatisticArray, StatisticMap, StatisticObject, StatisticType, StatisticValue,
};
use crate::clasp::util::misc_types::ratio;

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
