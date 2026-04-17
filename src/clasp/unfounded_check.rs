//! Partial Rust port of `original_clasp/clasp/unfounded_check.h`.
//!
//! This module currently ports the self-contained helper state used by the
//! upstream unfounded-set checker. The solver-coupled propagator itself remains
//! blocked on the still-incomplete solver/shared-context/runtime integration.

use crate::clasp::constraint::priority_reserved_ufs;
use crate::clasp::literal::Weight_t;
use crate::potassco::bits::Bitset;

pub const DEFAULT_UNFOUNDED_CHECK_PRIO: u32 = priority_reserved_ufs;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReasonStrategy {
    #[default]
    CommonReason = 0,
    OnlyReason = 1,
    DistinctReason = 2,
    SharedReason = 3,
    NoReason = 4,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UfsType {
    None = 0,
    Poly = 1,
    NonPoly = 2,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchType {
    SourceFalse = 0,
    HeadFalse = 1,
    HeadTrue = 2,
    SubgoalFalse = 3,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BodyData {
    pub watches: u32,
    pub picked: bool,
    pub lower_or_ext: u32,
}

pub type ExtSet = Bitset<u32, u32>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtData {
    pub lower: Weight_t,
    pub slack: Weight_t,
    flags: Vec<ExtSet>,
}

impl ExtData {
    pub fn new(preds: u32, bound: Weight_t) -> Self {
        let max_count = ExtSet::MAX_COUNT as usize;
        let words = (preds as usize).div_ceil(max_count);
        Self {
            lower: bound,
            slack: -bound,
            flags: vec![ExtSet::new(); words],
        }
    }

    pub const fn word(idx: u32) -> usize {
        (idx / ExtSet::MAX_COUNT) as usize
    }

    pub const fn pos(idx: u32) -> u32 {
        idx % ExtSet::MAX_COUNT
    }

    pub fn in_ws(&self, idx: u32) -> bool {
        self.flags
            .get(Self::word(idx))
            .is_some_and(|set| set.contains(Self::pos(idx)))
    }

    pub fn add_to_ws(&mut self, idx: u32, weight: Weight_t) -> bool {
        let word = Self::word(idx);
        if self.flags[word].add(Self::pos(idx)) {
            self.lower -= weight;
        }
        self.lower <= 0
    }

    pub fn remove_from_ws(&mut self, idx: u32, weight: Weight_t) {
        let word = Self::word(idx);
        if self.flags[word].remove(Self::pos(idx)) {
            self.lower += weight;
        }
    }

    pub fn word_count(&self) -> usize {
        self.flags.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtomData {
    source: u32,
    pub todo: bool,
    pub ufs: bool,
    valid_source: bool,
}

impl AtomData {
    pub const NIL_SOURCE: u32 = (1u32 << 29) - 1;

    pub const fn watch(self) -> u32 {
        self.source
    }

    pub const fn has_source(self) -> bool {
        self.valid_source
    }

    pub fn mark_source_invalid(&mut self) {
        self.valid_source = false;
    }

    pub fn resurrect_source(&mut self) {
        self.valid_source = true;
    }

    pub fn set_source(&mut self, body: u32) {
        self.source = body;
        self.valid_source = true;
    }
}

impl Default for AtomData {
    fn default() -> Self {
        Self {
            source: Self::NIL_SOURCE,
            todo: false,
            ufs: false,
            valid_source: false,
        }
    }
}
