//! Partial Rust port of `original_clasp/clasp/logic_program_types.h` and
//! `original_clasp/src/logic_program_types.cpp`.
//!
//! This module currently ports the self-contained low-level ASP program graph
//! helpers: `PrgNode`, `PrgEdge`, `AtomState`, and `SmallEdgeList`.
//! `RuleTransform`, SCC checking, and the `PrgHead`/`PrgAtom`/`PrgBody` runtime
//! still depend on the unported logic-program, solver, clause, and
//! weight-constraint integration layers.

use crate::clasp::literal::{
    LitView, Literal, Val_t, Var_t, lit_true, value_false, value_free, value_true,
};
use crate::clasp::pod_vector::PodVectorT;
use crate::potassco::basic_types::{Atom, Id, atom};

#[allow(non_camel_case_types)]
pub type Atom_t = Atom;
#[allow(non_camel_case_types)]
pub type Id_t = Id;

#[allow(non_upper_case_globals)]
pub const value_weak_true: Val_t = 3;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NodeType {
    Atom = 0,
    Body = 1,
    Disj = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrgNode {
    node_type: NodeType,
    lit_id: u32,
    id: u32,
    val: Val_t,
    eq: bool,
    seen: bool,
}

impl PrgNode {
    pub const SCC_NOT_SET: u32 = (1u32 << 27) - 1;
    pub const SCC_TRIV: u32 = (1u32 << 27) - 2;
    pub const NO_NODE: u32 = (1u32 << 28) - 1;
    pub const NO_LIT: u32 = 1;

    pub const fn new(id: u32, node_type: NodeType) -> Self {
        assert!(id < Self::NO_NODE);
        Self {
            node_type,
            lit_id: Self::NO_LIT,
            id,
            val: value_free,
            eq: false,
            seen: false,
        }
    }

    pub const fn node_type(self) -> NodeType {
        self.node_type
    }

    pub const fn is_atom(self) -> bool {
        matches!(self.node_type, NodeType::Atom)
    }

    pub const fn relevant(self) -> bool {
        !self.eq
    }

    pub const fn removed(self) -> bool {
        self.eq && self.id == Self::NO_NODE
    }

    pub const fn eq(self) -> bool {
        self.eq && self.id != Self::NO_NODE
    }

    pub const fn seen(self) -> bool {
        self.seen
    }

    pub const fn has_var(self) -> bool {
        self.lit_id != Self::NO_LIT
    }

    pub const fn var(self) -> Var_t {
        self.lit_id >> 1
    }

    pub const fn literal(self) -> Literal {
        Literal::from_id(self.lit_id)
    }

    pub const fn value(self) -> Val_t {
        self.val
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub fn true_lit(self) -> Literal {
        if self.value() == value_free {
            lit_true
        } else {
            self.literal() ^ (self.value() == value_false)
        }
    }

    pub fn set_literal(&mut self, literal: Literal) {
        self.lit_id = literal.id();
    }

    pub fn clear_literal(&mut self, clear_value: bool) {
        self.lit_id = Self::NO_LIT;
        if clear_value {
            self.val = value_free;
        }
    }

    pub fn set_value(&mut self, value: Val_t) {
        self.val = value;
    }

    pub fn set_eq(&mut self, eq_id: u32) {
        self.id = eq_id;
        self.eq = true;
        self.seen = true;
    }

    pub fn mark_removed(&mut self) {
        if !PrgNode::eq(*self) {
            self.set_eq(Self::NO_NODE);
        }
    }

    pub fn set_seen(&mut self, seen: bool) {
        self.seen = seen;
    }

    pub fn reset_id(&mut self, id: u32, seen: bool) {
        self.id = id;
        self.eq = false;
        self.seen = seen;
    }

    pub fn assign_value_impl(&mut self, value: Val_t, no_weak: bool) -> bool {
        let value = if value == value_weak_true && no_weak {
            value_true
        } else {
            value
        };
        if self.value() == value_free
            || value == self.value()
            || (self.value() == value_weak_true && value == value_true)
        {
            self.set_value(value);
            true
        } else {
            value == value_weak_true && self.value() == value_true
        }
    }
}

pub const fn is_scc(scc: u32) -> bool {
    scc < PrgNode::SCC_TRIV
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EdgeType {
    Normal = 0,
    Gamma = 1,
    Choice = 2,
    GammaChoice = 3,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PrgEdge {
    rep: u32,
}

impl Default for PrgEdge {
    fn default() -> Self {
        Self::no_edge()
    }
}

impl PrgEdge {
    pub const fn no_edge() -> Self {
        Self { rep: u32::MAX }
    }

    pub const fn new(node_id: u32, node_type: NodeType, edge_type: EdgeType) -> Self {
        Self {
            rep: (node_id << 4) | ((node_type as u32) << 2) | (edge_type as u32),
        }
    }

    pub const fn node(self) -> u32 {
        self.rep >> 4
    }

    pub const fn edge_type(self) -> EdgeType {
        match self.rep & 3u32 {
            0 => EdgeType::Normal,
            1 => EdgeType::Gamma,
            2 => EdgeType::Choice,
            3 => EdgeType::GammaChoice,
            _ => unreachable!(),
        }
    }

    pub const fn node_type(self) -> NodeType {
        match (self.rep >> 2) & 3u32 {
            0 => NodeType::Atom,
            1 => NodeType::Body,
            2 => NodeType::Disj,
            _ => unreachable!(),
        }
    }

    pub const fn is_normal(self) -> bool {
        (self.rep & 2u32) == 0
    }

    pub const fn is_choice(self) -> bool {
        (self.rep & 2u32) != 0
    }

    pub const fn is_gamma(self) -> bool {
        (self.rep & 1u32) != 0
    }

    pub const fn is_body(self) -> bool {
        matches!(self.node_type(), NodeType::Body)
    }

    pub const fn is_atom(self) -> bool {
        matches!(self.node_type(), NodeType::Atom)
    }

    pub const fn is_disj(self) -> bool {
        matches!(self.node_type(), NodeType::Disj)
    }

    pub const fn is_valid(self) -> bool {
        self.rep != u32::MAX
    }
}

pub type EdgeVec = PodVectorT<PrgEdge>;
pub type EdgeSpan<'a> = &'a [PrgEdge];

pub const fn is_choice(edge_type: EdgeType) -> bool {
    (edge_type as u32) >= EdgeType::Choice as u32
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AtomState {
    state: PodVectorT<u8>,
}

impl AtomState {
    pub const POS_FLAG: u8 = 0x1;
    pub const NEG_FLAG: u8 = 0x2;
    pub const HEAD_FLAG: u8 = 0x4;
    pub const CHOICE_FLAG: u8 = 0x8;
    pub const DISJ_FLAG: u8 = 0x10;
    pub const RULE_MASK: u8 = 0x1F;
    pub const SHOWN_FLAG: u8 = 0x20;
    pub const PROJECT_FLAG: u8 = 0x40;

    pub fn swap(&mut self, other: &mut Self) {
        self.state.swap(&mut other.state);
    }

    pub fn in_head(&self, edge: PrgEdge) -> bool {
        self.is_set(edge.node(), Self::head_flag(edge))
    }

    pub fn in_head_atom(&self, atom: Atom_t) -> bool {
        self.is_set(atom, Self::HEAD_FLAG)
    }

    pub fn in_body(&self, literal: Literal) -> bool {
        self.is_set(literal.var(), Self::POS_FLAG + literal.sign() as u8)
    }

    pub fn is_set(&self, var: Var_t, flag: u8) -> bool {
        (var as usize) < self.state.len() && (self.state[var as usize] & flag) != 0
    }

    pub fn add_to_head(&mut self, atom: Atom_t) {
        self.set(atom, Self::HEAD_FLAG);
    }

    pub fn add_to_head_edge(&mut self, edge: PrgEdge) {
        self.set(edge.node(), Self::head_flag(edge));
    }

    pub fn add_to_body(&mut self, literal: Literal) {
        self.set(literal.var(), Self::POS_FLAG + literal.sign() as u8);
    }

    pub fn add_to_body_slice(&mut self, body: LitView<'_>) {
        for &literal in body {
            self.add_to_body(literal);
        }
    }

    pub fn set(&mut self, var: Var_t, flag: u8) {
        self.grow(var);
        self.state[var as usize] |= flag;
    }

    pub fn clear(&mut self, var: Var_t, flag: u8) {
        if (var as usize) < self.state.len() {
            self.state[var as usize] &= !flag;
        }
    }

    pub fn clear_rule_var(&mut self, var: Var_t) {
        self.clear(var, Self::RULE_MASK);
    }

    pub fn clear_head(&mut self, edge: PrgEdge) {
        self.clear(edge.node(), Self::head_flag(edge));
    }

    pub fn clear_body(&mut self, literal: Literal) {
        self.clear(literal.var(), Self::POS_FLAG + literal.sign() as u8);
    }

    pub fn resize(&mut self, size: u32) {
        self.state.resize(size as usize, 0);
    }

    pub fn clear_rule_with<T, F>(&mut self, values: &[T], mut map_var: F)
    where
        F: FnMut(&T) -> Var_t,
    {
        for value in values {
            self.clear_rule_var(map_var(value));
        }
    }

    pub fn clear_rule_atoms<T>(&mut self, values: &[T])
    where
        T: Copy + crate::potassco::basic_types::AtomOf,
    {
        self.clear_rule_with(values, |value| atom(*value));
    }

    pub fn clear_body_slice(&mut self, body: LitView<'_>) {
        for &literal in body {
            self.clear_body(literal);
        }
    }

    pub fn all_marked(&self, atoms: &[Var_t], flag: u8) -> bool {
        atoms.iter().copied().all(|var| self.is_set(var, flag))
    }

    pub fn body_marked(&self, body: LitView<'_>) -> bool {
        body.iter().copied().all(|literal| self.in_body(literal))
    }

    fn grow(&mut self, var: Var_t) {
        if (var as usize) >= self.state.len() {
            self.state.resize(var as usize + 1, 0);
        }
    }

    const fn head_flag(edge: PrgEdge) -> u8 {
        if edge.is_atom() {
            Self::HEAD_FLAG << edge.is_choice() as u8
        } else {
            Self::DISJ_FLAG
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmallEdgeListTag {
    S0 = 0,
    S1 = 1,
    S2 = 2,
    Large = 3,
}

impl SmallEdgeListTag {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn from_len(len: u32) -> Self {
        match len {
            0 => Self::S0,
            1 => Self::S1,
            2 => Self::S2,
            _ => Self::Large,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SmallEdgeList {
    large: PodVectorT<PrgEdge>,
    small: [PrgEdge; 2],
}

impl SmallEdgeList {
    pub fn empty(&self, tag: SmallEdgeListTag) -> bool {
        self.size(tag) == 0
    }

    pub fn size(&self, tag: SmallEdgeListTag) -> u32 {
        if matches!(tag, SmallEdgeListTag::Large) {
            self.large.len() as u32
        } else {
            tag.as_u32()
        }
    }

    pub fn span(&self, tag: SmallEdgeListTag) -> EdgeSpan<'_> {
        if matches!(tag, SmallEdgeListTag::Large) {
            self.large.as_slice()
        } else {
            &self.small[..tag.as_u32() as usize]
        }
    }

    pub fn data_ptr(&self, tag: SmallEdgeListTag) -> *const PrgEdge {
        if matches!(tag, SmallEdgeListTag::Large) {
            self.large.as_slice().as_ptr()
        } else {
            self.small.as_ptr()
        }
    }

    pub fn push(&mut self, tag: SmallEdgeListTag, edge: PrgEdge) -> SmallEdgeListTag {
        let size = tag.as_u32();
        if size < 2 {
            self.small[size as usize] = edge;
            return SmallEdgeListTag::from_len(size + 1);
        }
        if self.large.is_empty() {
            self.large.reserve(4);
            self.large.push_back(self.small[0]);
            self.large.push_back(self.small[1]);
        }
        self.large.push_back(edge);
        SmallEdgeListTag::Large
    }

    pub fn pop(&mut self, tag: SmallEdgeListTag, n: u32) -> SmallEdgeListTag {
        if !matches!(tag, SmallEdgeListTag::Large) {
            assert!(n <= tag.as_u32());
            return SmallEdgeListTag::from_len(tag.as_u32() - n);
        }
        assert!(n <= self.large.len() as u32);
        self.large
            .resize(self.large.len() - n as usize, PrgEdge::no_edge());
        tag
    }

    pub fn clear(&mut self, tag: SmallEdgeListTag) -> SmallEdgeListTag {
        if matches!(tag, SmallEdgeListTag::Large) {
            self.large = PodVectorT::new();
        }
        SmallEdgeListTag::S0
    }

    /// # Safety
    ///
    /// `last` must either point into the active storage for `tag` or one past the
    /// end of that storage.
    pub unsafe fn shrink_to(
        &mut self,
        tag: SmallEdgeListTag,
        last: *const PrgEdge,
    ) -> SmallEdgeListTag {
        if !matches!(tag, SmallEdgeListTag::Large) {
            let base = self.small.as_ptr();
            let end = base.wrapping_add(tag.as_u32() as usize);
            assert!(last >= base && last <= end);
            let size = unsafe { last.offset_from(base) } as u32;
            return SmallEdgeListTag::from_len(size);
        }
        let base = self.large.as_slice().as_ptr();
        let end = base.wrapping_add(self.large.len());
        assert!(last >= base && last <= end);
        let size = unsafe { last.offset_from(base) } as usize;
        self.large.resize(size, PrgEdge::no_edge());
        tag
    }
}
