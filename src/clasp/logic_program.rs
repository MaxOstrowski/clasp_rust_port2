//! Partial Rust port of `original_clasp/clasp/logic_program.h` and
//! `original_clasp/src/logic_program.cpp`.

use core::ops::{Index, IndexMut};

use crate::potassco::basic_types::{BodyType, HeadType};

const BODY_STAT_KEY_COUNT: usize = BodyType::Count as usize + 1;
const RULE_STAT_KEY_COUNT: usize = RuleStatsKey::KeyNum as usize;

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
