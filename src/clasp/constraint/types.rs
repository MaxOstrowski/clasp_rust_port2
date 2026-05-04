// Types and enums for constraint module
// (Extracted from constraint.rs)

use crate::potassco::bits::{BitIndex, Bitset};

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConstraintType {
    #[default]
    Static = 0,
    Conflict = 1,
    Loop = 2,
    Other = 3,
}

impl ConstraintType {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Static),
            1 => Some(Self::Conflict),
            2 => Some(Self::Loop),
            3 => Some(Self::Other),
            _ => None,
        }
    }
}

impl BitIndex for ConstraintType {
    fn bit_index(self) -> u32 {
        self as u32
    }
}

pub type TypeSet = Bitset<u32, ConstraintType>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClauseOwnerKind {
    #[default]
    Unknown,
    Explicit,
    Shared,
}
