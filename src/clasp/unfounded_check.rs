//! Partial Rust port of `original_clasp/clasp/unfounded_check.h`.
//!
//! This module currently ports the self-contained helper state used by the
//! upstream unfounded-set checker. The solver-coupled propagator itself remains
//! blocked on the still-incomplete solver/shared-context/runtime integration.

use core::ptr::NonNull;

use crate::clasp::claspfwd::Asp::PrgDepGraph;
use crate::clasp::constraint::ConstraintInfo;
use crate::clasp::constraint::priority_reserved_ufs;
use crate::clasp::literal::{LitVec, VarVec, Weight_t};
use crate::clasp::pod_vector::{PodQueue, PodVectorT};
use crate::clasp::solver::Solver;
use crate::clasp::solver_strategies::FwdCheck;
use crate::potassco::bits::Bitset;

pub const DEFAULT_UNFOUNDED_CHECK_PRIO: u32 = priority_reserved_ufs;
pub type NodeId = u32;
pub type GraphPtr = NonNull<PrgDepGraph>;
pub type ConstGraphPtr = NonNull<PrgDepGraph>;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodyData {
    pub watches: u32,
    pub picked: bool,
    pub lower_or_ext: u32,
}

impl BodyData {
    pub const fn new() -> Self {
        Self {
            watches: 0,
            picked: false,
            lower_or_ext: 0,
        }
    }
}

impl Default for BodyData {
    fn default() -> Self {
        Self::new()
    }
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BodyPtr {
    pub node: Option<NonNull<()>>,
    pub id: NodeId,
}

impl BodyPtr {
    pub const fn new(node: Option<NonNull<()>>, id: NodeId) -> Self {
        Self { node, id }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExtWatch {
    pub body_id: NodeId,
    pub data: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimalityCheck {
    pub fwd: FwdCheck,
    pub high: u32,
    pub low: u32,
    pub next: u32,
    pub scc: u32,
}

impl MinimalityCheck {
    pub const fn new(mut fwd: FwdCheck) -> Self {
        if fwd.high_pct > 100 {
            fwd.high_pct = 100;
        }
        if fwd.high_step == 0 {
            fwd.high_step = u32::MAX;
        }
        Self {
            fwd,
            high: fwd.high_step,
            low: 0,
            next: if fwd.disable != 0 { u32::MAX } else { 0 },
            scc: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct DefaultUnfoundedCheck {
    solver_: Option<NonNull<Solver>>,
    graph_: Option<GraphPtr>,
    mini_: Option<Box<MinimalityCheck>>,
    atoms_: PodVectorT<AtomData>,
    bodies_: PodVectorT<BodyData>,
    todo_: PodQueue<NodeId>,
    ufs_: PodQueue<NodeId>,
    source_q_: PodQueue<NodeId>,
    invalid_: VarVec,
    extended_: Vec<Option<Box<ExtData>>>,
    watches_: PodVectorT<ExtWatch>,
    picked_ext_: VarVec,
    loop_atoms_: LitVec,
    active_clause_: LitVec,
    reasons_: Option<Vec<LitVec>>,
    info_: ConstraintInfo,
    strategy_: ReasonStrategy,
}

impl DefaultUnfoundedCheck {
    pub fn priority(&self) -> u32 {
        DEFAULT_UNFOUNDED_CHECK_PRIO
    }

    pub fn reason_strategy(&self) -> ReasonStrategy {
        self.strategy_
    }

    pub fn set_reason_strategy(&mut self, strategy: ReasonStrategy) {
        self.strategy_ = strategy;
    }

    pub fn graph(&self) -> Option<ConstGraphPtr> {
        self.graph_
    }

    pub fn nodes(&self) -> u32 {
        self.atoms_.len() as u32 + self.bodies_.len() as u32
    }

    pub fn solver_bound(&self) -> bool {
        self.solver_.is_some()
    }

    pub fn has_minimality_check(&self) -> bool {
        self.mini_.is_some()
    }

    pub fn extended_len(&self) -> usize {
        self.extended_.len()
    }

    pub fn active_clause_len(&self) -> usize {
        self.active_clause_.len()
    }

    pub fn reason_slots(&self) -> usize {
        self.reasons_.as_ref().map_or(0, Vec::len)
    }

    pub fn invalid_len(&self) -> usize {
        self.invalid_.len()
    }

    pub fn watch_count(&self) -> usize {
        self.watches_.len()
    }

    pub fn picked_ext_len(&self) -> usize {
        self.picked_ext_.len()
    }

    pub fn loop_atom_count(&self) -> usize {
        self.loop_atoms_.len()
    }

    pub fn todo_count(&self) -> u32 {
        self.todo_.size()
    }

    pub fn ufs_count(&self) -> u32 {
        self.ufs_.size()
    }

    pub fn source_queue_count(&self) -> u32 {
        self.source_q_.size()
    }

    pub fn info(&self) -> ConstraintInfo {
        self.info_
    }
}
