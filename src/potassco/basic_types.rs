//! Rust port of original_clasp/libpotassco/potassco/basic_types.h.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use crate::potassco::bits::BitIndex;
use crate::potassco::enums::EnumTag;

pub type Id = u32;
pub const ID_MAX: Id = Id::MAX;

pub type Atom = u32;
pub const ATOM_MIN: Atom = 1;
pub const ATOM_MAX: Atom = (1u32 << 31) - 1;

pub type Lit = i32;
pub type Weight = i32;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct WeightLit {
    pub lit: Lit,
    pub weight: Weight,
}

impl PartialEq<Lit> for WeightLit {
    fn eq(&self, other: &Lit) -> bool {
        self.lit == *other && self.weight == 1
    }
}

impl PartialOrd<Lit> for WeightLit {
    fn partial_cmp(&self, other: &Lit) -> Option<Ordering> {
        Some(self.cmp(&WeightLit {
            lit: *other,
            weight: 1,
        }))
    }
}

pub type IdSpan<'a> = &'a [Id];
pub type AtomSpan<'a> = &'a [Atom];
pub type LitSpan<'a> = &'a [Lit];
pub type WeightLitSpan<'a> = &'a [WeightLit];

pub fn to_span<T>(value: &T) -> &[T] {
    core::slice::from_ref(value)
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HeadType {
    Disjunctive = 0,
    Choice = 1,
}

impl EnumTag for HeadType {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Disjunctive),
            1 => Some(Self::Choice),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        1
    }

    fn count() -> usize {
        2
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BodyType {
    Normal = 0,
    Sum = 1,
    Count = 2,
}

impl EnumTag for BodyType {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::Sum),
            2 => Some(Self::Count),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        2
    }

    fn count() -> usize {
        3
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TruthValue {
    Free = 0,
    True = 1,
    False = 2,
    Release = 3,
}

impl EnumTag for TruthValue {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Free),
            1 => Some(Self::True),
            2 => Some(Self::False),
            3 => Some(Self::Release),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        3
    }

    fn count() -> usize {
        4
    }

    fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::Free => "free",
            Self::True => "true",
            Self::False => "false",
            Self::Release => "release",
        })
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DomModifier {
    Level = 0,
    Sign = 1,
    Factor = 2,
    Init = 3,
    True = 4,
    False = 5,
}

impl EnumTag for DomModifier {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Level),
            1 => Some(Self::Sign),
            2 => Some(Self::Factor),
            3 => Some(Self::Init),
            4 => Some(Self::True),
            5 => Some(Self::False),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        5
    }

    fn count() -> usize {
        6
    }

    fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::Level => "level",
            Self::Sign => "sign",
            Self::Factor => "factor",
            Self::Init => "init",
            Self::True => "true",
            Self::False => "false",
        })
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AtomArgMode {
    Raw,
    Unquote,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AtomArg {
    First = 0,
    Last = 1,
}

impl EnumTag for AtomArg {
    type Repr = u8;

    fn to_underlying(self) -> Self::Repr {
        self as u8
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::First),
            1 => Some(Self::Last),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        1
    }

    fn count() -> usize {
        2
    }

    fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::First => "first",
            Self::Last => "last",
        })
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct AtomCompare(pub u8);

impl AtomCompare {
    pub const CMP_DEFAULT: Self = Self(0);
    pub const CMP_NATURAL: Self = Self(1);
    pub const CMP_ARITY: Self = Self(2);
}

impl BitOr for AtomCompare {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for AtomCompare {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for AtomCompare {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for AtomCompare {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitXor for AtomCompare {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for AtomCompare {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl Not for AtomCompare {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl BitIndex for AtomCompare {
    fn bit_index(self) -> u32 {
        self.0 as u32
    }
}

pub trait ToI128 {
    fn to_i128(self) -> i128;
}

macro_rules! impl_to_i128 {
	($($ty:ty),+ $(,)?) => {
		$(
			impl ToI128 for $ty {
				fn to_i128(self) -> i128 {
					self as i128
				}
			}
		)+
	};
}

impl_to_i128!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

pub fn valid_atom<T: ToI128>(value: T) -> bool {
    let value = value.to_i128();
    value >= ATOM_MIN as i128 && value <= ATOM_MAX as i128
}

pub trait AtomOf {
    fn atom(self) -> Atom;
}

impl AtomOf for Atom {
    fn atom(self) -> Atom {
        self
    }
}

impl AtomOf for Lit {
    fn atom(self) -> Atom {
        if self >= 0 {
            self as Atom
        } else {
            self.wrapping_neg() as Atom
        }
    }
}

impl AtomOf for WeightLit {
    fn atom(self) -> Atom {
        self.lit.atom()
    }
}

pub fn atom<T: AtomOf>(value: T) -> Atom {
    value.atom()
}

pub trait LitOf {
    fn lit(self) -> Lit;
}

impl LitOf for Atom {
    fn lit(self) -> Lit {
        self as Lit
    }
}

impl LitOf for Lit {
    fn lit(self) -> Lit {
        self
    }
}

impl LitOf for WeightLit {
    fn lit(self) -> Lit {
        self.lit
    }
}

pub fn lit<T: LitOf>(value: T) -> Lit {
    value.lit()
}

pub trait NegOf {
    fn neg(self) -> Lit;
}

impl NegOf for Atom {
    fn neg(self) -> Lit {
        -(self as Lit)
    }
}

impl NegOf for Lit {
    fn neg(self) -> Lit {
        -self
    }
}

pub fn neg<T: NegOf>(value: T) -> Lit {
    value.neg()
}

pub trait WeightOf {
    fn weight(self) -> Weight;
}

impl WeightOf for Atom {
    fn weight(self) -> Weight {
        1
    }
}

impl WeightOf for Lit {
    fn weight(self) -> Weight {
        1
    }
}

impl WeightOf for WeightLit {
    fn weight(self) -> Weight {
        self.weight
    }
}

pub fn weight<T: WeightOf>(value: T) -> Weight {
    value.weight()
}

pub trait AbstractProgram {
    fn init_program(&mut self, _incremental: bool) {}
    fn begin_step(&mut self) {}
    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>);
    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    );
    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>);
    fn output_atom(&mut self, atom: Atom, name: &str);

    fn output_term(&mut self, _term_id: Id, _name: &str) {
        panic!("output term not supported");
    }

    fn output(&mut self, _term_id: Id, _condition: LitSpan<'_>) {
        panic!("output term not supported");
    }

    fn project(&mut self, _atoms: AtomSpan<'_>) {
        panic!("projection not supported");
    }

    fn external(&mut self, _atom: Atom, _value: TruthValue) {
        panic!("externals not supported");
    }

    fn assume(&mut self, _lits: LitSpan<'_>) {
        panic!("assumptions not supported");
    }

    fn heuristic(
        &mut self,
        _atom: Atom,
        _modifier: DomModifier,
        _bias: i32,
        _priority: u32,
        _condition: LitSpan<'_>,
    ) {
        panic!("heuristic directive not supported");
    }

    fn acyc_edge(&mut self, _source: i32, _target: i32, _condition: LitSpan<'_>) {
        panic!("edge directive not supported");
    }

    fn theory_term_number(&mut self, _term_id: Id, _number: i32) {
        panic!("theory data not supported");
    }

    fn theory_term_symbol(&mut self, _term_id: Id, _name: &str) {
        panic!("theory data not supported");
    }

    fn theory_term_compound(&mut self, _term_id: Id, _functor_id: i32, _args: IdSpan<'_>) {
        panic!("theory data not supported");
    }

    fn theory_element(&mut self, _element_id: Id, _terms: IdSpan<'_>, _cond: LitSpan<'_>) {
        panic!("theory data not supported");
    }

    fn theory_atom(&mut self, _atom_or_zero: Id, _term_id: Id, _elements: IdSpan<'_>) {
        panic!("theory data not supported");
    }

    fn theory_atom_guarded(
        &mut self,
        _atom_or_zero: Id,
        _term_id: Id,
        _elements: IdSpan<'_>,
        _op: Id,
        _rhs: Id,
    ) {
        panic!("theory data not supported");
    }

    fn end_step(&mut self) {}
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

fn match_term<'a>(input: &mut &'a str) -> Option<&'a str> {
    let bytes = input.as_bytes();
    let mut pos = 0usize;
    let mut paren = 0usize;
    while pos < bytes.len() {
        match bytes[pos] {
            b'(' => {
                paren += 1;
            }
            b')' => {
                if paren == 0 {
                    break;
                }
                paren -= 1;
            }
            b'"' => {
                let mut quoted = false;
                pos += 1;
                while pos < bytes.len() {
                    let c = bytes[pos];
                    if c == b'"' && !quoted {
                        break;
                    }
                    quoted = !quoted && c == b'\\';
                    pos += 1;
                }
                if pos == bytes.len() {
                    break;
                }
            }
            b',' if paren == 0 => break,
            _ => {}
        }
        pos += 1;
    }
    let matched = &input[..pos];
    *input = &input[pos..];
    if matched.is_empty() {
        None
    } else {
        Some(matched)
    }
}

fn atom_arg(arg: &str, mode: AtomArgMode) -> &str {
    if mode == AtomArgMode::Raw || !arg.starts_with('"') || !arg.ends_with('"') || arg.len() < 2 {
        arg
    } else {
        &arg[1..arg.len() - 1]
    }
}

pub fn pop_arg<'a>(args: &mut &'a str, arg_pos: AtomArg, mode: AtomArgMode) -> &'a str {
    if arg_pos == AtomArg::First {
        if let Some(matched) = match_term(args) {
            if args.starts_with(',') {
                *args = &args[1..];
            }
            return atom_arg(matched, mode);
        }
        return "";
    }

    let bytes = args.as_bytes();
    let mut pos = bytes.len();
    let mut paren = 0i32;
    while pos > 0 {
        pos -= 1;
        let c = bytes[pos];
        if c == b',' && paren == 0 {
            break;
        }
        match c {
            b'"' => {
                let mut quoted = false;
                while pos > 0 {
                    pos -= 1;
                    let c2 = bytes[pos];
                    if c2 == b'"' && !quoted {
                        break;
                    }
                    quoted = !quoted && c2 == b'\\';
                }
            }
            b')' => paren += 1,
            b'(' => paren -= 1,
            _ => {}
        }
    }

    let matched = &args[pos..];
    *args = &args[..pos];
    let matched = if pos > 0 && matched.starts_with(',') {
        &matched[1..]
    } else {
        matched
    };
    atom_arg(matched, mode)
}

pub fn atom_symbol(atom_text: &str) -> (&str, i32, &str) {
    let id_end = atom_text.find('(').unwrap_or(atom_text.len());
    let id = &atom_text[..id_end];
    let args = &atom_text[id_end..];
    if args.len() < 3 || !args.ends_with(')') {
        let arity = if !args.is_empty() && args != "()" {
            -1
        } else {
            0
        };
        return (id, arity, "");
    }

    let mut remainder = &args[1..];
    let mut arity = 1;
    let mut out_args = &remainder[..remainder.len() - 1];
    while match_term(&mut remainder).is_some() {
        if remainder.len() > 2 && remainder.starts_with(',') {
            arity += 1;
            remainder = &remainder[1..];
        } else {
            break;
        }
    }

    if remainder != ")" {
        out_args = "";
        arity = -1;
    }
    (id, arity, out_args)
}

pub fn predicate(atom_text: &str) -> (&str, i32) {
    let (name, arity, _) = atom_symbol(atom_text);
    (name, arity)
}

pub fn cmp_atom(mut lhs: &str, mut rhs: &str, cmp: AtomCompare) -> Ordering {
    if (cmp & AtomCompare::CMP_ARITY) == AtomCompare::CMP_ARITY {
        let (left_id, left_arity) = predicate(lhs);
        let (right_id, right_arity) = predicate(rhs);
        match left_arity.cmp(&right_arity) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
        match left_id.cmp(right_id) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
        lhs = &lhs[left_id.len()..];
        rhs = &rhs[right_id.len()..];
    }

    if (cmp & AtomCompare::CMP_NATURAL) == AtomCompare::CMP_NATURAL {
        let mut x = 0usize;
        while x < lhs.len().min(rhs.len()) {
            let lb = lhs.as_bytes()[x];
            let rb = rhs.as_bytes()[x];
            if is_digit(lb) && is_digit(rb) {
                let lhs_non_zero = lhs[x..]
                    .bytes()
                    .position(|c| c != b'0')
                    .map(|pos| x + pos)
                    .unwrap_or(lhs.len());
                let rhs_non_zero = rhs[x..]
                    .bytes()
                    .position(|c| c != b'0')
                    .map(|pos| x + pos)
                    .unwrap_or(rhs.len());
                let mut lhs_digits = &lhs[lhs_non_zero..];
                let mut rhs_digits = &rhs[rhs_non_zero..];
                let mut numeric_cmp = Ordering::Equal;
                let mut lp = 0usize;
                let mut rp = 0usize;
                loop {
                    let l = lhs_digits.as_bytes().get(lp).copied().unwrap_or(0);
                    let r = rhs_digits.as_bytes().get(rp).copied().unwrap_or(0);
                    let ld = is_digit(l);
                    let rd = is_digit(r);
                    lp += usize::from(ld || l != 0);
                    rp += usize::from(rd || r != 0);
                    if !ld || !rd {
                        let resolved = if rd {
                            if !ld { Ordering::Less } else { numeric_cmp }
                        } else if ld {
                            Ordering::Greater
                        } else {
                            numeric_cmp
                        };
                        if resolved != Ordering::Equal {
                            if x == 0 || lhs.as_bytes()[x - 1] != b'-' {
                                return resolved;
                            }
                            return match resolved {
                                Ordering::Less => Ordering::Greater,
                                Ordering::Greater => Ordering::Less,
                                Ordering::Equal => Ordering::Equal,
                            };
                        }
                        lhs_digits = &lhs_digits[lp.saturating_sub(usize::from(l == 0))..];
                        rhs_digits = &rhs_digits[rp.saturating_sub(usize::from(r == 0))..];
                        lhs = lhs_digits;
                        rhs = rhs_digits;
                        x = 0;
                        break;
                    }
                    if numeric_cmp == Ordering::Equal {
                        numeric_cmp = l.cmp(&r);
                    }
                }
                continue;
            }

            match lb.cmp(&rb) {
                Ordering::Equal => {
                    x += 1;
                }
                ordering => return ordering,
            }
        }
        return lhs.len().cmp(&rhs.len());
    }

    lhs.cmp(rhs)
}

impl fmt::Display for WeightLit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.lit, self.weight)
    }
}
