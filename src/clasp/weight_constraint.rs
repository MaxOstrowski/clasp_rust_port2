//! Port target for original_clasp/clasp/weight_constraint.h, original_clasp/src/weight_constraint.cpp.

use crate::clasp::literal::{Weight_t, WeightLitView};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeightLitsRep<'a> {
    pub lits: WeightLitView<'a>,
    pub size: u32,
    pub bound: Weight_t,
    pub reach: Weight_t,
}

impl WeightLitsRep<'_> {
    #[inline]
    pub fn sat(&self) -> bool {
        self.bound <= 0
    }

    #[inline]
    pub fn unsat(&self) -> bool {
        self.reach < self.bound
    }

    #[inline]
    pub fn open(&self) -> bool {
        self.bound > 0 && self.bound <= self.reach
    }

    #[inline]
    pub fn has_weights(&self) -> bool {
        self.size != 0 && self.lits[0].weight > 1
    }

    #[inline]
    pub fn literals(&self) -> WeightLitView<'_> {
        &self.lits[..self.size as usize]
    }
}
