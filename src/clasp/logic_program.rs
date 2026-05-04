//! Partial Rust port of `original_clasp/clasp/logic_program.h` and
//! `original_clasp/src/logic_program.cpp`.

use core::ops::{Index, IndexMut};

use crate::potassco::basic_types::{BodyType, HeadType};

const BODY_STAT_KEY_COUNT: usize = BodyType::Count as usize + 1;
const RULE_STAT_KEY_COUNT: usize = RuleStatsKey::KeyNum as usize;

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
