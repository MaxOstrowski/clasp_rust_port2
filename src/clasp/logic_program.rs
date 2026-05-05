//! Partial Rust port of `original_clasp/clasp/logic_program.h` and
//! `original_clasp/src/logic_program.cpp`.

use core::ops::{Index, IndexMut};
use std::panic::panic_any;

use crate::clasp::literal::VarType;
use crate::clasp::logic_program_types::PrgNode;
use crate::clasp::statistics::{StatisticMap, StatisticObject};
use crate::potassco::basic_types::{BodyType, HeadType};
use crate::potassco::error::Error;

const BODY_STAT_KEY_COUNT: usize = BodyType::Count as usize + 1;
const RULE_STAT_KEY_COUNT: usize = RuleStatsKey::KeyNum as usize;
const LP_STAT_KEYS: [&str; 30] = [
    "atoms",
    "atoms_aux",
    "disjunctions",
    "disjunctions_non_hcf",
    "bodies",
    "bodies_tr",
    "sum_bodies",
    "sum_bodies_tr",
    "count_bodies",
    "count_bodies_tr",
    "sccs",
    "sccs_non_hcf",
    "gammas",
    "ufs_nodes",
    "rules",
    "rules_normal",
    "rules_choice",
    "rules_minimize",
    "rules_acyc",
    "rules_heuristic",
    "rules_tr",
    "rules_tr_normal",
    "rules_tr_choice",
    "rules_tr_minimize",
    "rules_tr_acyc",
    "rules_tr_heuristic",
    "eqs",
    "eqs_atom",
    "eqs_body",
    "eqs_other",
];

fn sum_body_stats(value: &BodyStats) -> f64 {
    value.sum() as f64
}

fn sum_rule_stats(value: &RuleStats) -> f64 {
    value.sum() as f64
}

fn sum_eqs(value: &LpStats) -> f64 {
    value.eqs() as f64
}

fn panic_lp_stats_range(key: &str) -> ! {
    panic_any(Error::OutOfRange(format!("invalid LpStats key: {key}")))
}

fn panic_lp_stats_index(index: u32) -> ! {
    panic_any(Error::OutOfRange(format!(
        "invalid LpStats index {index} (size: {})",
        LP_STAT_KEYS.len()
    )))
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExtendedRuleMode {
    #[default]
    Native = 0,
    Transform = 1,
    TransformChoice = 2,
    TransformCard = 3,
    TransformWeight = 4,
    TransformScc = 5,
    TransformNhcf = 6,
    TransformInteg = 7,
    TransformDynamic = 8,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AtomSorting {
    #[default]
    Auto = 0,
    No = 1,
    Number = 2,
    Name = 3,
    Natural = 4,
    Arity = 5,
    ArityNatural = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AspOptions {
    pub er_mode: ExtendedRuleMode,
    pub iters: u32,
    pub sort_atom: AtomSorting,
    pub no_scc: bool,
    pub supp_mod: bool,
    pub df_order: bool,
    pub backprop: bool,
    pub old_map: bool,
    pub no_gamma: bool,
}

impl Default for AspOptions {
    fn default() -> Self {
        Self {
            er_mode: ExtendedRuleMode::Native,
            iters: 5,
            sort_atom: AtomSorting::Auto,
            no_scc: false,
            supp_mod: false,
            df_order: false,
            backprop: false,
            old_map: false,
            no_gamma: false,
        }
    }
}

impl AspOptions {
    pub const MAX_EQ_ITERS: u32 = (1u32 << 23) - 1;

    pub fn iterations(&mut self, iterations: u32) -> &mut Self {
        self.iters = iterations.min(Self::MAX_EQ_ITERS);
        self
    }

    pub fn depth_first(&mut self) -> &mut Self {
        self.df_order = true;
        self
    }

    pub fn backpropagate(&mut self) -> &mut Self {
        self.backprop = true;
        self
    }

    pub fn no_scc(&mut self) -> &mut Self {
        self.no_scc = true;
        self
    }

    pub fn no_eq(&mut self) -> &mut Self {
        self.iters = 0;
        self
    }

    pub fn disable_gamma(&mut self) -> &mut Self {
        self.no_gamma = true;
        self
    }

    pub fn ext(&mut self, mode: ExtendedRuleMode) -> &mut Self {
        self.er_mode = mode;
        self
    }

    pub fn sort(&mut self, sorting: AtomSorting) -> &mut Self {
        self.sort_atom = sorting;
        self
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleStatsKey {
    Normal = HeadType::Disjunctive as u32,
    Choice = HeadType::Choice as u32,
    Minimize,
    Acyc,
    Heuristic,
    KeyNum,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuleStats {
    key: [u32; RULE_STAT_KEY_COUNT],
}

impl RuleStats {
    pub const fn num_keys() -> u32 {
        RULE_STAT_KEY_COUNT as u32
    }

    pub fn to_str(key: u32) -> &'static str {
        assert!(key <= Self::num_keys(), "Invalid key");
        match key {
            value if value == RuleStatsKey::Normal as u32 => "Normal",
            value if value == RuleStatsKey::Choice as u32 => "Choice",
            value if value == RuleStatsKey::Minimize as u32 => "Minimize",
            value if value == RuleStatsKey::Acyc as u32 => "Acyc",
            value if value == RuleStatsKey::Heuristic as u32 => "Heuristic",
            _ => "None",
        }
    }

    pub fn up(&mut self, key: RuleStatsKey, amount: i32) {
        self[key as u32] += amount as u32;
    }

    pub fn sum(&self) -> u32 {
        self.key.iter().copied().sum()
    }
}

impl Index<u32> for RuleStats {
    type Output = u32;

    fn index(&self, index: u32) -> &Self::Output {
        &self.key[index as usize]
    }
}

impl IndexMut<u32> for RuleStats {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self.key[index as usize]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BodyStats {
    key: [u32; BODY_STAT_KEY_COUNT],
}

impl BodyStats {
    pub const fn num_keys() -> u32 {
        BODY_STAT_KEY_COUNT as u32
    }

    pub fn to_str(key: u32) -> &'static str {
        assert!(key < Self::num_keys(), "Invalid body type!");
        match key {
            value if value == BodyType::Count as u32 => "Count",
            value if value == BodyType::Sum as u32 => "Sum",
            _ => "Normal",
        }
    }

    pub fn up(&mut self, key: BodyType, amount: i32) {
        self[key as u32] += amount as u32;
    }

    pub fn sum(&self) -> u32 {
        self.key.iter().copied().sum()
    }
}

impl Index<u32> for BodyStats {
    type Output = u32;

    fn index(&self, index: u32) -> &Self::Output {
        &self.key[index as usize]
    }
}

impl IndexMut<u32> for BodyStats {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self.key[index as usize]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LpStats {
    pub rules: [RuleStats; 2],
    pub bodies: [BodyStats; 2],
    pub atoms: u32,
    pub aux_atoms: u32,
    pub disjunctions: [u32; 2],
    pub sccs: u32,
    pub non_hcfs: u32,
    pub gammas: u32,
    pub ufs_nodes: u32,
    eqs_: [u32; 3],
}

impl LpStats {
    pub fn eqs(&self) -> u32 {
        self.eqs_for(VarType::Atom) + self.eqs_for(VarType::Body) + self.eqs_for(VarType::Hybrid)
    }

    pub fn eqs_for(&self, var_type: VarType) -> u32 {
        self.eqs_[(var_type.as_u32() - 1) as usize]
    }

    pub fn inc_eqs(&mut self, var_type: VarType) {
        self.eqs_[(var_type.as_u32() - 1) as usize] += 1;
    }

    pub fn accu(&mut self, other: &Self) {
        self.atoms += other.atoms;
        self.aux_atoms += other.aux_atoms;
        self.ufs_nodes += other.ufs_nodes;
        if self.sccs == PrgNode::SCC_NOT_SET || other.sccs == PrgNode::SCC_NOT_SET {
            self.sccs = other.sccs;
            self.non_hcfs = other.non_hcfs;
            self.gammas = other.gammas;
        } else {
            self.sccs += other.sccs;
            self.non_hcfs += other.non_hcfs;
            self.gammas += other.gammas;
        }
        for index in 0..self.disjunctions.len() {
            self.disjunctions[index] += other.disjunctions[index];
            for key in 0..BodyStats::num_keys() {
                self.bodies[index][key] += other.bodies[index][key];
            }
            for key in 0..RuleStats::num_keys() {
                self.rules[index][key] += other.rules[index][key];
            }
        }
        for (lhs, rhs) in self.eqs_.iter_mut().zip(other.eqs_) {
            *lhs += rhs;
        }
    }

    pub const fn size() -> u32 {
        LP_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        LP_STAT_KEYS
            .get(index as usize)
            .copied()
            .unwrap_or_else(|| panic_lp_stats_index(index))
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "atoms" => StatisticObject::from_value(&self.atoms),
            "atoms_aux" => StatisticObject::from_value(&self.aux_atoms),
            "disjunctions" => StatisticObject::from_value(&self.disjunctions[0]),
            "disjunctions_non_hcf" => StatisticObject::from_value(&self.disjunctions[1]),
            "bodies" => StatisticObject::from_mapped_value(&self.bodies[0], sum_body_stats),
            "bodies_tr" => StatisticObject::from_mapped_value(&self.bodies[1], sum_body_stats),
            "sum_bodies" => StatisticObject::from_value(&self.bodies[0][BodyType::Sum as u32]),
            "sum_bodies_tr" => StatisticObject::from_value(&self.bodies[1][BodyType::Sum as u32]),
            "count_bodies" => StatisticObject::from_value(&self.bodies[0][BodyType::Count as u32]),
            "count_bodies_tr" => {
                StatisticObject::from_value(&self.bodies[1][BodyType::Count as u32])
            }
            "sccs" => StatisticObject::from_value(&self.sccs),
            "sccs_non_hcf" => StatisticObject::from_value(&self.non_hcfs),
            "gammas" => StatisticObject::from_value(&self.gammas),
            "ufs_nodes" => StatisticObject::from_value(&self.ufs_nodes),
            "rules" => StatisticObject::from_mapped_value(&self.rules[0], sum_rule_stats),
            "rules_normal" => {
                StatisticObject::from_value(&self.rules[0][RuleStatsKey::Normal as u32])
            }
            "rules_choice" => {
                StatisticObject::from_value(&self.rules[0][RuleStatsKey::Choice as u32])
            }
            "rules_minimize" => {
                StatisticObject::from_value(&self.rules[0][RuleStatsKey::Minimize as u32])
            }
            "rules_acyc" => StatisticObject::from_value(&self.rules[0][RuleStatsKey::Acyc as u32]),
            "rules_heuristic" => {
                StatisticObject::from_value(&self.rules[0][RuleStatsKey::Heuristic as u32])
            }
            "rules_tr" => StatisticObject::from_mapped_value(&self.rules[1], sum_rule_stats),
            "rules_tr_normal" => {
                StatisticObject::from_value(&self.rules[1][RuleStatsKey::Normal as u32])
            }
            "rules_tr_choice" => {
                StatisticObject::from_value(&self.rules[1][RuleStatsKey::Choice as u32])
            }
            "rules_tr_minimize" => {
                StatisticObject::from_value(&self.rules[1][RuleStatsKey::Minimize as u32])
            }
            "rules_tr_acyc" => {
                StatisticObject::from_value(&self.rules[1][RuleStatsKey::Acyc as u32])
            }
            "rules_tr_heuristic" => {
                StatisticObject::from_value(&self.rules[1][RuleStatsKey::Heuristic as u32])
            }
            "eqs" => StatisticObject::from_mapped_value(self, sum_eqs),
            "eqs_atom" => {
                StatisticObject::from_value(&self.eqs_[(VarType::Atom.as_u32() - 1) as usize])
            }
            "eqs_body" => {
                StatisticObject::from_value(&self.eqs_[(VarType::Body.as_u32() - 1) as usize])
            }
            "eqs_other" => {
                StatisticObject::from_value(&self.eqs_[(VarType::Hybrid.as_u32() - 1) as usize])
            }
            _ => panic_lp_stats_range(key),
        }
    }
}

impl StatisticMap for LpStats {
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
