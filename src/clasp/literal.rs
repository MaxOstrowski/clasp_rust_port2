//! Rust port of `original_clasp/clasp/literal.h`.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitOr, BitXor, Not};

use crate::clasp::pod_vector::PodVectorT;
use crate::potassco::enums::{EnumMetadata, EnumTag, HasEnumEntries, make_entries};

pub type VarT = u32;
#[allow(non_camel_case_types)]
pub type Var_t = VarT;

#[allow(non_upper_case_globals)]
pub const var_max: VarT = 1u32 << 30;
#[allow(non_upper_case_globals)]
pub const sent_var: VarT = 0;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VarType {
    Atom = 1u32,
    Body = 2u32,
    Hybrid = 3u32,
}

impl VarType {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl EnumTag for VarType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            1 => Some(Self::Atom),
            2 => Some(Self::Body),
            3 => Some(Self::Hybrid),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Entries(Self::entries_metadata()))
    }
}

impl HasEnumEntries for VarType {
    fn entries_metadata() -> crate::potassco::enums::EnumEntries<Self> {
        static ENTRIES: &[(VarType, &str)] = &[
            (VarType::Atom, "atom"),
            (VarType::Body, "body"),
            (VarType::Hybrid, "hybrid"),
        ];
        make_entries(ENTRIES)
    }
}

impl BitOr for VarType {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::from_underlying(self.as_u32() | rhs.as_u32()).expect("invalid VarType combination")
    }
}

impl BitAnd for VarType {
    type Output = u32;

    fn bitand(self, rhs: Self) -> Self::Output {
        self.as_u32() & rhs.as_u32()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Literal {
    rep: u32,
}

impl Literal {
    const SIGN_MASK: u32 = 2u32;
    const FLAG_MASK: u32 = 1u32;
    const ID_MAX: u32 = (1u32 << 31) - 1;

    pub const fn new(var: VarT, sign: bool) -> Self {
        assert!(var < var_max);
        Self {
            rep: (var << Self::SIGN_MASK) + ((sign as u32) << Self::FLAG_MASK),
        }
    }

    pub const fn var(self) -> VarT {
        self.rep >> Self::SIGN_MASK
    }

    pub const fn sign(self) -> bool {
        (self.rep & Self::SIGN_MASK) != 0
    }

    pub const fn id(self) -> u32 {
        self.rep >> Self::FLAG_MASK
    }

    pub const fn rep(self) -> u32 {
        self.rep
    }

    pub fn rep_mut(&mut self) -> &mut u32 {
        &mut self.rep
    }

    pub const fn from_id(id: u32) -> Self {
        assert!(id <= Self::ID_MAX);
        Self {
            rep: id << Self::FLAG_MASK,
        }
    }

    pub const fn from_rep(rep: u32) -> Self {
        Self { rep }
    }

    pub fn swap(&mut self, other: &mut Self) {
        core::mem::swap(&mut self.rep, &mut other.rep);
    }

    pub fn flag(&mut self) -> &mut Self {
        self.rep |= Self::FLAG_MASK;
        self
    }

    pub fn unflag(&mut self) -> &mut Self {
        self.rep &= !Self::FLAG_MASK;
        self
    }

    pub const fn flagged(self) -> bool {
        (self.rep & Self::FLAG_MASK) != 0
    }

    pub const fn complement(self) -> Self {
        Self {
            rep: (self.rep & !Self::FLAG_MASK) ^ Self::SIGN_MASK,
        }
    }
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for Literal {}

impl PartialOrd for Literal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Literal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id().cmp(&other.id())
    }
}

impl Hash for Literal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl Not for Literal {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.complement()
    }
}

impl BitXor<bool> for Literal {
    type Output = Self;

    fn bitxor(self, rhs: bool) -> Self::Output {
        Self::from_id(self.id() ^ rhs as u32)
    }
}

impl BitXor<Literal> for bool {
    type Output = Literal;

    fn bitxor(self, rhs: Literal) -> Self::Output {
        rhs ^ self
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", to_int(*self))
    }
}

pub fn swap(lhs: &mut Literal, rhs: &mut Literal) {
    lhs.swap(rhs);
}

pub const fn neg_lit(var: VarT) -> Literal {
    Literal::new(var, true)
}

pub const fn pos_lit(var: VarT) -> Literal {
    Literal::new(var, false)
}

pub const fn to_lit(value: i32) -> Literal {
    if value < 0 {
        neg_lit((-value) as VarT)
    } else {
        pos_lit(value as VarT)
    }
}

pub const fn to_int(lit: Literal) -> i32 {
    if lit.sign() {
        -(lit.var() as i32)
    } else {
        lit.var() as i32
    }
}

#[allow(non_upper_case_globals)]
pub const lit_true: Literal = pos_lit(sent_var);
#[allow(non_upper_case_globals)]
pub const lit_false: Literal = neg_lit(sent_var);

pub const fn is_sentinel(lit: Literal) -> bool {
    lit.var() == sent_var
}

pub const fn encode_lit(lit: Literal) -> i32 {
    if !lit.sign() {
        (lit.var() + 1) as i32
    } else {
        -((lit.var() + 1) as i32)
    }
}

pub const fn decode_var(value: i32) -> VarT {
    value.unsigned_abs() - 1
}

pub const fn decode_lit(value: i32) -> Literal {
    Literal::new(decode_var(value), value < 0)
}

pub const fn hash_id(mut key: u32) -> u32 {
    key = (!key).wrapping_add(key << 15);
    key ^= key >> 11;
    key = key.wrapping_add(key << 3);
    key ^= key >> 5;
    key = key.wrapping_add(key << 10);
    key ^= key >> 16;
    key
}

pub const fn hash_lit(lit: Literal) -> u32 {
    hash_id(lit.id())
}

pub type WeightT = i32;
#[allow(non_camel_case_types)]
pub type Weight_t = WeightT;
pub type WsumT = i64;
#[allow(non_camel_case_types)]
pub type Wsum_t = WsumT;

#[allow(non_upper_case_globals)]
pub const weight_min: WeightT = i32::MIN;
#[allow(non_upper_case_globals)]
pub const weight_max: WeightT = i32::MAX;
#[allow(non_upper_case_globals)]
pub const weight_sum_min: WsumT = i64::MIN;
#[allow(non_upper_case_globals)]
pub const weight_sum_max: WsumT = i64::MAX;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct WeightLiteral {
    pub lit: Literal,
    pub weight: WeightT,
}

impl fmt::Display for WeightLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.lit, self.weight)
    }
}

pub type SpanView<'a, T> = &'a [T];
pub type VarVec = PodVectorT<VarT>;
pub type VarView<'a> = SpanView<'a, VarT>;
pub type LitVec = PodVectorT<Literal>;
pub type LitView<'a> = SpanView<'a, Literal>;
pub type WeightVec = PodVectorT<WeightT>;
pub type WeightView<'a> = SpanView<'a, WeightT>;
pub type SumVec = PodVectorT<WsumT>;
pub type SumView<'a> = SpanView<'a, WsumT>;
pub type WeightLitVec = PodVectorT<WeightLiteral>;
pub type WeightLitView<'a> = SpanView<'a, WeightLiteral>;

pub type ValT = u8;
#[allow(non_camel_case_types)]
pub type Val_t = ValT;
pub type ValueVec = PodVectorT<ValT>;
pub type ValueView<'a> = SpanView<'a, ValT>;

#[allow(non_upper_case_globals)]
pub const value_free: ValT = 0;
#[allow(non_upper_case_globals)]
pub const value_true: ValT = 1;
#[allow(non_upper_case_globals)]
pub const value_false: ValT = 2;

pub const fn true_value(lit: Literal) -> ValT {
    1u8 + lit.sign() as u8
}

pub const fn false_value(lit: Literal) -> ValT {
    2u8 - lit.sign() as u8
}

pub const fn val_sign(value: ValT) -> bool {
    value != value_true
}

#[allow(non_snake_case)]
pub const fn negLit(var: VarT) -> Literal {
    neg_lit(var)
}

#[allow(non_snake_case)]
pub const fn posLit(var: VarT) -> Literal {
    pos_lit(var)
}

#[allow(non_snake_case)]
pub const fn toLit(value: i32) -> Literal {
    to_lit(value)
}

#[allow(non_snake_case)]
pub const fn toInt(lit: Literal) -> i32 {
    to_int(lit)
}

#[allow(non_snake_case)]
pub const fn isSentinel(lit: Literal) -> bool {
    is_sentinel(lit)
}

#[allow(non_snake_case)]
pub const fn encodeLit(lit: Literal) -> i32 {
    encode_lit(lit)
}

#[allow(non_snake_case)]
pub const fn decodeVar(value: i32) -> VarT {
    decode_var(value)
}

#[allow(non_snake_case)]
pub const fn decodeLit(value: i32) -> Literal {
    decode_lit(value)
}

#[allow(non_snake_case)]
pub const fn hashId(key: u32) -> u32 {
    hash_id(key)
}

#[allow(non_snake_case)]
pub const fn hashLit(lit: Literal) -> u32 {
    hash_lit(lit)
}

#[allow(non_snake_case)]
pub const fn trueValue(lit: Literal) -> ValT {
    true_value(lit)
}

#[allow(non_snake_case)]
pub const fn falseValue(lit: Literal) -> ValT {
    false_value(lit)
}

#[allow(non_snake_case)]
pub const fn valSign(value: ValT) -> bool {
    val_sign(value)
}
