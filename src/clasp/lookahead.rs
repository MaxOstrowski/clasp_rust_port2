//! Partial Rust port of `original_clasp/clasp/lookahead.h`.
//!
//! This module currently ports the solver-independent lookahead state from the
//! upstream header plus the source-level `ScoreLook::countNant()` and
//! `ScoreLook::scoreLits()` scoring helpers from `original_clasp/src/lookahead.cpp`.
//! The propagator logic from `original_clasp/src/lookahead.cpp` still depends on
//! the not-yet-complete solver and shared-context APIs and remains to be ported.

use crate::clasp::constraint::priority_reserved_look;
use crate::clasp::literal::{Literal, Var_t, VarType, lit_true};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VarScore {
    p_val: u32,
    n_val: u32,
    seen: u32,
    tested: u32,
}

impl VarScore {
    pub const MAX_SCORE: u32 = (1u32 << 14) - 1;

    const fn mask(literal: Literal) -> u32 {
        if literal.sign() { 2 } else { 1 }
    }

    pub const fn seen_lit(self, literal: Literal) -> bool {
        (self.seen & Self::mask(literal)) != 0
    }

    pub const fn seen_any(self) -> bool {
        self.seen != 0
    }

    pub fn set_tested(&mut self, literal: Literal) {
        self.tested |= Self::mask(literal);
    }

    pub const fn tested_lit(self, literal: Literal) -> bool {
        (self.tested & Self::mask(literal)) != 0
    }

    pub const fn tested_any(self) -> bool {
        self.tested != 0
    }

    pub const fn tested_both(self) -> bool {
        self.tested == 3
    }

    pub fn set_score(&mut self, literal: Literal, value: u32) {
        self.set_score_impl(literal, value);
        self.set_tested(literal);
    }

    pub fn set_dep_score(&mut self, literal: Literal, score: u32) {
        if !self.seen_lit(literal) || self.score(literal) > score {
            self.set_score_impl(literal, score);
            self.seen |= Self::mask(literal);
        }
    }

    pub const fn score(self, literal: Literal) -> u32 {
        if literal.sign() {
            self.n_val
        } else {
            self.p_val
        }
    }

    pub const fn score_pair(self) -> (u32, u32) {
        if self.n_val > self.p_val {
            (self.n_val, self.p_val)
        } else {
            (self.p_val, self.n_val)
        }
    }

    pub const fn pref_sign(self) -> bool {
        self.n_val > self.p_val
    }

    pub const fn n_val(self) -> u32 {
        self.n_val
    }

    pub const fn p_val(self) -> u32 {
        self.p_val
    }

    fn set_score_impl(&mut self, literal: Literal, value: u32) {
        let value = value.min(Self::MAX_SCORE);
        if literal.sign() {
            self.n_val = value;
        } else {
            self.p_val = value;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreLookMode {
    #[default]
    ScoreMax,
    ScoreMaxMin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreLookVarInfo {
    pub var_type: VarType,
    pub nant: bool,
}

impl ScoreLookVarInfo {
    pub const fn new(var_type: VarType, nant: bool) -> Self {
        Self { var_type, nant }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreLook {
    pub score: Vec<VarScore>,
    pub deps: Vec<Var_t>,
    pub types: VarType,
    pub best: Var_t,
    pub limit: u32,
    pub mode: ScoreLookMode,
    pub add_deps: bool,
    pub nant: bool,
}

impl Default for ScoreLook {
    fn default() -> Self {
        Self {
            score: Vec::new(),
            deps: Vec::new(),
            types: VarType::Atom,
            best: 0,
            limit: u32::MAX,
            mode: ScoreLookMode::ScoreMax,
            add_deps: true,
            nant: false,
        }
    }
}

impl ScoreLook {
    fn is_any(lhs: VarType, rhs: VarType) -> bool {
        (lhs.as_u32() & rhs.as_u32()) != 0
    }

    pub fn valid_var(&self, var: Var_t) -> bool {
        (var as usize) < self.score.len()
    }

    pub fn count_nant_with<F>(literals: &[Literal], mut info_for: F) -> u32
    where
        F: FnMut(Var_t) -> ScoreLookVarInfo,
    {
        1 + literals
            .iter()
            .filter(|literal| info_for(literal.var()).nant)
            .count() as u32
    }

    pub fn score_lits_with<F>(&mut self, literals: &[Literal], mut info_for: F)
    where
        F: FnMut(Var_t) -> ScoreLookVarInfo,
    {
        assert!(!literals.is_empty());
        let score = if !self.nant {
            literals.len() as u32
        } else {
            Self::count_nant_with(literals, &mut info_for)
        };
        let first = literals[0];
        let var = first.var();
        assert!(self.valid_var(var));
        self.score[var as usize].set_score(first, score);
        if self.add_deps {
            if (self.score[var as usize].tested_both() || self.mode == ScoreLookMode::ScoreMax)
                && self.greater(var, self.best)
            {
                self.best = var;
            }
            for literal in literals {
                let dep_var = literal.var();
                if self.valid_var(dep_var) && Self::is_any(info_for(dep_var).var_type, self.types) {
                    if !self.score[dep_var as usize].seen_any() {
                        self.deps.push(dep_var);
                    }
                    self.score[dep_var as usize].set_dep_score(*literal, score);
                }
            }
        }
    }

    pub fn clear_deps(&mut self) {
        while let Some(var) = self.deps.pop() {
            if let Some(score) = self.score.get_mut(var as usize) {
                *score = VarScore::default();
            }
        }
        self.best = 0;
        self.limit = u32::MAX;
    }

    pub fn greater(&self, lhs: Var_t, rhs: Var_t) -> bool {
        let (rhs_max, rhs_min) = self.score[rhs as usize].score_pair();
        match self.mode {
            ScoreLookMode::ScoreMax => self.greater_max(lhs, rhs_max),
            ScoreLookMode::ScoreMaxMin => self.greater_max_min(lhs, rhs_max, rhs_min),
        }
    }

    pub fn greater_max(&self, var: Var_t, max: u32) -> bool {
        let score = self.score[var as usize];
        score.n_val() > max || score.p_val() > max
    }

    pub fn greater_max_min(&self, lhs: Var_t, max: u32, min: u32) -> bool {
        let (lhs_max, lhs_min) = self.score[lhs as usize].score_pair();
        lhs_min > min || (lhs_min == min && lhs_max > max)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LookaheadParams {
    pub var_type: VarType,
    pub lim: u32,
    pub top_level_imps: bool,
    pub restrict_nant: bool,
}

impl Default for LookaheadParams {
    fn default() -> Self {
        Self::new(VarType::Atom)
    }
}

impl LookaheadParams {
    pub const fn new(var_type: VarType) -> Self {
        Self {
            var_type,
            lim: 0,
            top_level_imps: true,
            restrict_nant: false,
        }
    }

    pub const fn lookahead(mut self, var_type: VarType) -> Self {
        self.var_type = var_type;
        self
    }

    pub const fn add_imps(mut self, enabled: bool) -> Self {
        self.top_level_imps = enabled;
        self
    }

    pub const fn nant(mut self, enabled: bool) -> Self {
        self.restrict_nant = enabled;
        self
    }

    pub const fn limit(mut self, limit: u32) -> Self {
        self.lim = limit;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lookahead {
    pub score: ScoreLook,
    nodes: Vec<LitNode>,
    saved: Vec<NodeId>,
    imps: Vec<Literal>,
    last: NodeId,
    pos: NodeId,
    top: u32,
    limit: u32,
}

impl Lookahead {
    pub const PRIO: u32 = priority_reserved_look;
    const HEAD_ID: NodeId = 0;
    const UNDO_ID: NodeId = 1;

    pub fn is_type(var_type: u32) -> bool {
        var_type != 0 && var_type <= VarType::Hybrid.as_u32()
    }

    pub fn new(params: LookaheadParams) -> Self {
        let mode = if params.var_type != VarType::Hybrid {
            ScoreLookMode::ScoreMaxMin
        } else {
            ScoreLookMode::ScoreMax
        };
        let mut nodes = vec![LitNode::new(lit_true), LitNode::new(lit_true)];
        nodes[Self::HEAD_ID as usize].next = Self::HEAD_ID;
        nodes[Self::UNDO_ID as usize].next = u32::MAX;
        if params.top_level_imps {
            nodes[Self::HEAD_ID as usize].lit.flag();
        }
        Self {
            score: ScoreLook {
                types: params.var_type,
                mode,
                nant: params.restrict_nant,
                ..ScoreLook::default()
            },
            nodes,
            saved: Vec::new(),
            imps: Vec::new(),
            last: Self::HEAD_ID,
            pos: Self::HEAD_ID,
            top: u32::MAX - 1,
            limit: params.lim,
        }
    }

    fn node(&self, node_id: NodeId) -> &LitNode {
        &self.nodes[node_id as usize]
    }

    fn node_mut(&mut self, node_id: NodeId) -> &mut LitNode {
        &mut self.nodes[node_id as usize]
    }

    fn head(&self) -> &LitNode {
        self.node(Self::HEAD_ID)
    }

    fn head_mut(&mut self) -> &mut LitNode {
        self.node_mut(Self::HEAD_ID)
    }

    fn undo_mut(&mut self) -> &mut LitNode {
        self.node_mut(Self::UNDO_ID)
    }

    pub fn clear(&mut self) {
        self.score.clear_deps();
        while self.saved.pop().is_some() {}
        let head = *self.head();
        self.nodes = vec![head, head];
        self.head_mut().next = Self::HEAD_ID;
        self.undo_mut().next = u32::MAX;
        self.last = Self::HEAD_ID;
        self.top = u32::MAX;
        self.imps.clear();
        self.pos = Self::HEAD_ID;
    }

    pub fn empty(&self) -> bool {
        self.head().next == Self::HEAD_ID
    }

    pub fn append(&mut self, mut literal: Literal, test_both: bool) {
        let next = self.nodes.len() as NodeId;
        self.node_mut(self.last).next = next;
        if test_both {
            literal.flag();
        }
        self.nodes.push(LitNode::new(literal));
        self.last = next;
        self.node_mut(self.last).next = Self::HEAD_ID;
    }

    pub const fn priority(&self) -> u32 {
        Self::PRIO
    }

    pub const fn has_limit(&self) -> bool {
        self.limit != 0
    }

    pub fn top_level_imps(&self) -> bool {
        self.head().lit.flagged()
    }

    pub const fn limit(&self) -> u32 {
        self.limit
    }
}

type NodeId = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LitNode {
    lit: Literal,
    next: NodeId,
}

impl LitNode {
    const fn new(lit: Literal) -> Self {
        Self {
            lit,
            next: u32::MAX,
        }
    }
}
