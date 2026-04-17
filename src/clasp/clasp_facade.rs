//! Rust port of solver-independent facade types from
//! original_clasp/clasp/clasp_facade.h and original_clasp/src/clasp_facade.cpp.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolveStatus {
    Unknown = 0,
    Sat = 1,
    Unsat = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolveResultExt {
    Exhaust = 4,
    Interrupt = 8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SolveResult {
    pub flags: u8,
    pub signal: u8,
}

impl SolveResult {
    pub const fn new(flags: u8, signal: u8) -> Self {
        Self { flags, signal }
    }

    pub const fn from_status(status: SolveStatus) -> Self {
        Self {
            flags: status as u8,
            signal: 0,
        }
    }

    pub const fn sat(self) -> bool {
        (self.flags & SolveStatus::Sat as u8) != 0
    }

    pub const fn unsat(self) -> bool {
        (self.flags & SolveStatus::Unsat as u8) != 0
    }

    pub const fn unknown(self) -> bool {
        (self.flags & 0b11) == SolveStatus::Unknown as u8
    }

    pub const fn exhausted(self) -> bool {
        (self.flags & SolveResultExt::Exhaust as u8) != 0
    }

    pub const fn interrupted(self) -> bool {
        (self.flags & SolveResultExt::Interrupt as u8) != 0
    }

    pub const fn status(self) -> SolveStatus {
        match self.flags & 0b11 {
            1 => SolveStatus::Sat,
            2 => SolveStatus::Unsat,
            _ => SolveStatus::Unknown,
        }
    }
}

impl From<SolveStatus> for SolveResult {
    fn from(value: SolveStatus) -> Self {
        Self::from_status(value)
    }
}

impl From<SolveResult> for SolveStatus {
    fn from(value: SolveResult) -> Self {
        value.status()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SolveMode(u32);

impl SolveMode {
    pub const DEF: Self = Self(0);
    pub const ASYNC: Self = Self(1);
    pub const YIELD: Self = Self(2);
    pub const ASYNC_YIELD: Self = Self(Self::ASYNC.0 | Self::YIELD.0);

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, rhs: Self) -> bool {
        (self.0 & rhs.0) == rhs.0
    }

    pub const fn is_default(self) -> bool {
        self.0 == Self::DEF.0
    }
}

impl BitOr for SolveMode {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for SolveMode {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for SolveMode {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for SolveMode {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitXor for SolveMode {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for SolveMode {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl Not for SolveMode {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}
