//! Partial Rust port of `original_clasp/clasp/heuristics.h` and
//! `original_clasp/src/heuristics.cpp`.
//!
//! This module currently ports the solver-independent score containers and
//! configuration normalization used by the lookback heuristics: Berkmin,
//! VMTF, VSIDS, and the domain score wrapper. The actual solver-attached
//! heuristic engines remain blocked on the still-incomplete solver,
//! shared-context, and clause infrastructure.

use crate::clasp::constraint::{ConstraintType, TypeSet};
use crate::clasp::literal::{Literal, Var_t};
use crate::clasp::solver_strategies::{DomMod, HeuParams, Score, ScoreOther};
use crate::clasp::util::indexed_priority_queue::IndexedPriorityQueue;
use crate::clasp::util::misc_types::Rng;

pub const BERK_NUM_CANDIDATES: usize = 5;
pub const BERK_MAX_MOMS_VARS: u32 = 9_999;
pub const BERK_MAX_MOMS_DECS: u32 = 50;
pub const BERK_MAX_DECAY: u32 = 65_534;

#[must_use]
pub const fn moms_score_from_counts(positive: u32, negative: u32) -> u32 {
    ((positive * negative) << 10) + (positive + negative)
}

pub fn add_other(types: &mut TypeSet, other: u32) {
    if other != ScoreOther::OtherNo as u32 {
        types.add(ConstraintType::Loop);
    }
    if other == ScoreOther::OtherAll as u32 {
        types.add(ConstraintType::Other);
    }
}

#[must_use]
pub fn init_decay(param: u32) -> f64 {
    let mut decay = if param == 0 { 0.95 } else { f64::from(param) };
    while decay > 1.0 {
        decay /= 10.0;
    }
    decay
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BerkminHScore {
    pub occ: i32,
    pub act: u16,
    pub dec: u16,
}

impl BerkminHScore {
    pub const fn new(decay: u32) -> Self {
        Self {
            occ: 0,
            act: 0,
            dec: decay as u16,
        }
    }

    pub fn inc_act(&mut self, global_decay: u32, huang: bool, sign: bool) {
        self.occ += 1 - (i32::from(sign) << 1);
        self.decay(global_decay, huang);
        self.act = self.act.wrapping_add(1);
    }

    pub fn inc_occ(&mut self, sign: bool) {
        self.occ += 1 - (i32::from(sign) << 1);
    }

    pub fn decay(&mut self, global_decay: u32, huang: bool) -> u32 {
        let delta = global_decay.saturating_sub(u32::from(self.dec));
        if delta != 0 {
            let shift = delta & 31;
            self.act = ((u32::from(self.act)).wrapping_shr(shift)) as u16;
            self.dec = global_decay as u16;
            if huang {
                self.occ /= 1_i32.wrapping_shl(shift);
            }
        }
        u32::from(self.act)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BerkminOrder {
    pub score: Vec<BerkminHScore>,
    pub decay: u32,
    pub huang: bool,
    pub nant: bool,
    pub res_score: u8,
}

impl Default for BerkminOrder {
    fn default() -> Self {
        Self {
            score: Vec::new(),
            decay: 0,
            huang: false,
            nant: false,
            res_score: Score::ScoreMultiSet as u8,
        }
    }
}

impl BerkminOrder {
    pub fn decayed_score(&mut self, var: Var_t) -> u32 {
        self.score[var as usize].decay(self.decay, self.huang)
    }

    pub fn occ(&self, var: Var_t) -> i32 {
        self.score[var as usize].occ
    }

    pub fn inc(&mut self, literal: Literal, in_nant: bool) {
        if !self.nant || in_nant {
            self.score[literal.var() as usize].inc_act(self.decay, self.huang, literal.sign());
        }
    }

    pub fn inc_occ(&mut self, literal: Literal) {
        self.score[literal.var() as usize].inc_occ(literal.sign());
    }

    pub fn compare(&mut self, lhs: Var_t, rhs: Var_t) -> i32 {
        self.decayed_score(lhs) as i32 - self.decayed_score(rhs) as i32
    }

    pub fn reset_decay(&mut self) {
        for score in self.score.iter_mut().skip(1) {
            score.decay(self.decay, self.huang);
            score.dec = 0;
        }
        self.decay = 0;
    }
}

pub struct BerkminOrderCompare<'a> {
    order: &'a mut BerkminOrder,
}

impl<'a> BerkminOrderCompare<'a> {
    pub fn new(order: &'a mut BerkminOrder) -> Self {
        Self { order }
    }

    pub fn prefers(&mut self, lhs: Var_t, rhs: Var_t) -> bool {
        let comparison = self
            .order
            .decayed_score(lhs)
            .cmp(&self.order.decayed_score(rhs));
        comparison.is_gt() || (comparison.is_eq() && lhs < rhs)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BerkminConfig {
    pub order: BerkminOrder,
    pub max_berkmin: u32,
    pub types: TypeSet,
}

impl BerkminConfig {
    pub fn new(params: HeuParams) -> Self {
        let mut config = Self {
            order: BerkminOrder::default(),
            max_berkmin: 0,
            types: TypeSet::new(),
        };
        config.set_config(params);
        config
    }

    pub fn set_config(&mut self, params: HeuParams) {
        self.max_berkmin = if params.param == 0 {
            u32::MAX
        } else {
            params.param
        };
        self.order.nant = params.nant != 0;
        self.order.huang = params.huang != 0;
        self.order.res_score = if params.score == Score::ScoreAuto as u32 {
            Score::ScoreMultiSet as u8
        } else {
            params.score as u8
        };
        self.types = TypeSet::new();
        add_other(&mut self.types, params.other);
        if params.moms != 0 {
            self.types.add(ConstraintType::Static);
        }
    }
}

pub struct ClaspBerkmin {
    order_: BerkminOrder,
    cache_: Vec<Var_t>,
    free_lits_: Vec<Literal>,
    free_other_lits_: Vec<Literal>,
    top_conflict_: u32,
    top_other_: u32,
    front_: Var_t,
    cache_front_: usize,
    cache_size_: u32,
    num_vsids_: u32,
    max_berkmin_: u32,
    types_: TypeSet,
    rng_: Rng,
}

impl Default for ClaspBerkmin {
    fn default() -> Self {
        Self::new(HeuParams::default())
    }
}

impl ClaspBerkmin {
    pub fn new(params: HeuParams) -> Self {
        let mut heuristic = Self {
            order_: BerkminOrder::default(),
            cache_: Vec::new(),
            free_lits_: Vec::new(),
            free_other_lits_: Vec::new(),
            top_conflict_: 0,
            top_other_: 0,
            front_: 0,
            cache_front_: 0,
            cache_size_: BERK_NUM_CANDIDATES as u32,
            num_vsids_: 0,
            max_berkmin_: u32::MAX,
            types_: TypeSet::new(),
            rng_: Rng::default(),
        };
        heuristic.set_config(params);
        heuristic
    }

    pub fn set_config(&mut self, params: HeuParams) {
        let config = BerkminConfig::new(params);
        self.order_ = config.order;
        self.max_berkmin_ = config.max_berkmin;
        self.types_ = config.types;
        self.cache_size_ = BERK_NUM_CANDIDATES as u32;
    }

    pub fn order(&self) -> &BerkminOrder {
        &self.order_
    }

    pub fn cache_len(&self) -> usize {
        self.cache_.len()
    }

    pub fn free_lits_len(&self) -> usize {
        self.free_lits_.len()
    }

    pub fn free_other_lits_len(&self) -> usize {
        self.free_other_lits_.len()
    }

    pub fn top_conflict(&self) -> u32 {
        self.top_conflict_
    }

    pub fn top_other(&self) -> u32 {
        self.top_other_
    }

    pub fn front(&self) -> Var_t {
        self.front_
    }

    pub fn cache_front(&self) -> usize {
        self.cache_front_
    }

    pub fn cache_size(&self) -> u32 {
        self.cache_size_
    }

    pub fn num_vsids(&self) -> u32 {
        self.num_vsids_
    }

    pub fn max_berkmin(&self) -> u32 {
        self.max_berkmin_
    }

    pub fn types(&self) -> TypeSet {
        self.types_
    }

    pub fn rng_seed(&self) -> u32 {
        self.rng_.seed()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VmtfVarInfo {
    pub prev: Var_t,
    pub next: Var_t,
    pub act: u32,
    pub occ: i32,
    pub decay: u32,
}

impl VmtfVarInfo {
    pub const fn in_list(self) -> bool {
        self.prev != self.next
    }

    pub fn activity_mut(&mut self, global_decay: u32) -> &mut u32 {
        let delta = global_decay.saturating_sub(self.decay);
        if delta != 0 {
            self.act = self.act.wrapping_shr((delta << 1) & 31);
            self.decay = global_decay;
        }
        &mut self.act
    }

    pub fn activity(&mut self, global_decay: u32) -> u32 {
        *self.activity_mut(global_decay)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmtfConfig {
    pub n_move: u32,
    pub types: TypeSet,
    pub score_type: u32,
    pub nant: bool,
}

impl VmtfConfig {
    pub fn new(params: HeuParams) -> Self {
        let mut config = Self {
            n_move: 8,
            types: TypeSet::new(),
            score_type: 0,
            nant: false,
        };
        config.set_config(params);
        config
    }

    pub fn set_config(&mut self, params: HeuParams) {
        self.n_move = if params.param == 0 {
            8
        } else {
            params.param.max(2)
        };
        self.score_type = if params.score != Score::ScoreAuto as u32 {
            params.score
        } else {
            Score::ScoreMin as u32
        };
        self.nant = params.nant != 0;
        self.types = TypeSet::new();
        add_other(
            &mut self.types,
            if params.other != ScoreOther::OtherAuto as u32 {
                params.other
            } else {
                ScoreOther::OtherNo as u32
            },
        );
        if params.moms != 0 {
            self.types.add(ConstraintType::Static);
        }
        if self.score_type == Score::ScoreMin as u32 {
            self.types.add(ConstraintType::Conflict);
        }
    }
}

pub struct ClaspVmtf {
    score_: Vec<VmtfVarInfo>,
    mtf_: Vec<Var_t>,
    front_: Var_t,
    decay_: u32,
    n_move_: u32,
    types_: TypeSet,
    sc_type_: u32,
    n_list_: u32,
    nant_: bool,
}

impl Default for ClaspVmtf {
    fn default() -> Self {
        Self::new(HeuParams::default())
    }
}

impl ClaspVmtf {
    pub fn new(params: HeuParams) -> Self {
        let mut heuristic = Self {
            score_: Vec::new(),
            mtf_: Vec::new(),
            front_: 0,
            decay_: 0,
            n_move_: 8,
            types_: TypeSet::new(),
            sc_type_: 0,
            n_list_: 0,
            nant_: false,
        };
        heuristic.set_config(params);
        heuristic
    }

    pub fn set_config(&mut self, params: HeuParams) {
        let config = VmtfConfig::new(params);
        self.n_move_ = config.n_move;
        self.types_ = config.types;
        self.sc_type_ = config.score_type;
        self.nant_ = config.nant;
    }

    pub fn score_slots(&self) -> usize {
        self.score_.len()
    }

    pub fn mtf_len(&self) -> usize {
        self.mtf_.len()
    }

    pub fn front(&self) -> Var_t {
        self.front_
    }

    pub fn decay(&self) -> u32 {
        self.decay_
    }

    pub fn n_move(&self) -> u32 {
        self.n_move_
    }

    pub fn types(&self) -> TypeSet {
        self.types_
    }

    pub fn score_type(&self) -> u32 {
        self.sc_type_
    }

    pub fn n_list(&self) -> u32 {
        self.n_list_
    }

    pub fn nant(&self) -> bool {
        self.nant_
    }
}

fn vsids_shell_cmp(_: Var_t, _: Var_t) -> bool {
    false
}

type VsidsVarOrder = IndexedPriorityQueue<Var_t, fn(Var_t, Var_t) -> bool>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VsidsScore {
    pub value: f64,
}

impl VsidsScore {
    pub const fn new(value: f64) -> Self {
        Self { value }
    }

    pub const fn get(self) -> f64 {
        self.value
    }

    pub fn set(&mut self, value: f64) {
        self.value = value;
    }

    pub fn apply_factor(_scores: &[Self], _var: Var_t, factor: f64) -> f64 {
        factor
    }
}

pub trait ActivityScore {
    fn get(&self) -> f64;
    fn set(&mut self, value: f64);
}

impl ActivityScore for VsidsScore {
    fn get(&self) -> f64 {
        self.value
    }

    fn set(&mut self, value: f64) {
        self.value = value;
    }
}

impl PartialOrd for VsidsScore {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VsidsDynDecay {
    pub curr: f64,
    pub stop: f64,
    pub bump: u32,
    pub freq: u16,
    pub next: u16,
}

impl VsidsDynDecay {
    pub fn advance(&mut self, decay: &mut f64) {
        if self.next == 0 {
            return;
        }
        self.next -= 1;
        if self.next == 0 {
            self.curr += f64::from(self.bump) / 100.0;
            *decay = 1.0 / self.curr;
            if self.curr < self.stop {
                self.next = self.freq;
            }
        }
    }
}

pub fn normalize_activity_scores<T: ActivityScore>(scores: &mut [T], inc: &mut f64) {
    const SCALE: f64 = 1e-100;
    const DENORMAL_GUARD: f64 = f64::MIN_POSITIVE * 1e100;

    *inc *= SCALE;
    for score in scores {
        let mut value = score.get();
        if value > 0.0 {
            value += DENORMAL_GUARD;
            value *= SCALE;
        }
        score.set(value);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VsidsConfig {
    pub types: TypeSet,
    pub score_type: u32,
    pub dyn_decay: VsidsDynDecay,
    pub decay: f64,
    pub inc: f64,
    pub acids: bool,
    pub nant: bool,
}

impl Default for VsidsConfig {
    fn default() -> Self {
        Self {
            types: TypeSet::new(),
            score_type: Score::ScoreMin as u32,
            dyn_decay: VsidsDynDecay::default(),
            decay: 1.0 / 0.95,
            inc: 1.0,
            acids: false,
            nant: false,
        }
    }
}

impl VsidsConfig {
    pub fn new(params: HeuParams) -> Self {
        let mut config = Self::default();
        config.set_config(params);
        config
    }

    pub fn set_config(&mut self, params: HeuParams) {
        self.types = TypeSet::new();
        add_other(
            &mut self.types,
            if params.other != ScoreOther::OtherAuto as u32 {
                params.other
            } else {
                ScoreOther::OtherNo as u32
            },
        );
        self.score_type = if params.score != Score::ScoreAuto as u32 {
            params.score
        } else {
            Score::ScoreMin as u32
        };
        self.dyn_decay = VsidsDynDecay::default();
        let decay_info = params.decay();
        let mut decay = init_decay(params.param);
        if decay_info.init != 0 && decay_info.init != params.param && decay_info.freq != 0 {
            let dynamic = init_decay(decay_info.init);
            self.dyn_decay.curr = decay.min(dynamic);
            self.dyn_decay.stop = decay.max(dynamic);
            self.dyn_decay.bump = decay_info.bump;
            self.dyn_decay.freq = decay_info.freq as u16;
            self.dyn_decay.next = decay_info.freq as u16;
            decay = self.dyn_decay.curr;
        }
        self.decay = 1.0 / decay;
        self.acids = params.acids != 0;
        self.nant = params.nant != 0;
        if params.moms != 0 {
            self.types.add(ConstraintType::Static);
        }
        if self.score_type == Score::ScoreMin as u32 {
            self.types.add(ConstraintType::Conflict);
        }
    }
}

pub struct ClaspVsidsBase<T> {
    score_: Vec<T>,
    occ_: Vec<i32>,
    vars_: VsidsVarOrder,
    dyn_: VsidsDynDecay,
    decay_: f64,
    inc_: f64,
    types_: TypeSet,
    sc_type_: u32,
    acids_: bool,
    nant_: bool,
}

impl<T> Default for ClaspVsidsBase<T> {
    fn default() -> Self {
        Self::new(HeuParams::default())
    }
}

impl<T> ClaspVsidsBase<T> {
    pub fn new(params: HeuParams) -> Self {
        let mut heuristic = Self {
            score_: Vec::new(),
            occ_: Vec::new(),
            vars_: IndexedPriorityQueue::new(vsids_shell_cmp),
            dyn_: VsidsDynDecay::default(),
            decay_: 1.0 / 0.95,
            inc_: 1.0,
            types_: TypeSet::new(),
            sc_type_: Score::ScoreMin as u32,
            acids_: false,
            nant_: false,
        };
        heuristic.set_config(params);
        heuristic
    }

    pub fn set_config(&mut self, params: HeuParams) {
        let config = VsidsConfig::new(params);
        self.dyn_ = config.dyn_decay;
        self.decay_ = config.decay;
        self.inc_ = config.inc;
        self.types_ = config.types;
        self.sc_type_ = config.score_type;
        self.acids_ = config.acids;
        self.nant_ = config.nant;
    }

    pub fn score_slots(&self) -> usize {
        self.score_.len()
    }

    pub fn occ_slots(&self) -> usize {
        self.occ_.len()
    }

    pub fn var_order_len(&self) -> usize {
        self.vars_.size()
    }

    pub fn dyn_decay(&self) -> VsidsDynDecay {
        self.dyn_
    }

    pub fn decay(&self) -> f64 {
        self.decay_
    }

    pub fn inc(&self) -> f64 {
        self.inc_
    }

    pub fn types(&self) -> TypeSet {
        self.types_
    }

    pub fn score_type(&self) -> u32 {
        self.sc_type_
    }

    pub fn acids(&self) -> bool {
        self.acids_
    }

    pub fn nant(&self) -> bool {
        self.nant_
    }
}

pub type ClaspVsids = ClaspVsidsBase<VsidsScore>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DomScore {
    pub value: f64,
    pub level: i16,
    pub factor: i16,
    pub dom_p: u32,
    pub sign: bool,
    pub init: bool,
}

impl DomScore {
    pub const DOM_MAX: u32 = (1u32 << 30) - 1;

    pub const fn new(value: f64) -> Self {
        Self {
            value,
            level: 0,
            factor: 1,
            dom_p: Self::DOM_MAX,
            sign: false,
            init: false,
        }
    }

    pub const fn get(self) -> f64 {
        self.value
    }

    pub fn set(&mut self, value: f64) {
        self.value = value;
    }

    pub const fn is_dom(self) -> bool {
        self.dom_p != Self::DOM_MAX
    }

    pub fn set_dom(&mut self, key: u32) {
        self.dom_p = key;
    }

    pub fn apply_factor(scores: &[Self], var: Var_t, factor: f64) -> f64 {
        let score = scores[var as usize];
        if score.factor == 1 {
            factor
        } else {
            f64::from(score.factor) * factor
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainAction {
    pub var: Var_t,
    pub mod_: DomMod,
    pub undo: u32,
    pub next: bool,
    pub bias: i16,
    pub prio: u16,
}

impl DomainAction {
    pub const UNDO_NIL: u32 = (1u32 << 31) - 1;

    pub const fn new(
        var: Var_t,
        mod_: DomMod,
        undo: u32,
        next: bool,
        bias: i16,
        prio: u16,
    ) -> Self {
        Self {
            var,
            mod_,
            undo,
            next,
            bias,
            prio,
        }
    }
}

impl ActivityScore for DomScore {
    fn get(&self) -> f64 {
        self.value
    }

    fn set(&mut self, value: f64) {
        self.value = value;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainDomPrio {
    prio: [u16; 4],
}

impl DomainDomPrio {
    pub const fn new() -> Self {
        Self { prio: [0; 4] }
    }

    pub fn clear(&mut self) {
        self.prio = [0; 4];
    }
}

impl core::ops::Index<usize> for DomainDomPrio {
    type Output = u16;

    fn index(&self, index: usize) -> &Self::Output {
        &self.prio[index]
    }
}

impl core::ops::IndexMut<usize> for DomainDomPrio {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.prio[index]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainFrame {
    pub dl: u32,
    pub head: u32,
}

impl DomainFrame {
    pub const fn new(dl: u32, head: u32) -> Self {
        Self { dl, head }
    }
}

impl PartialOrd for DomScore {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for DomScore {}

impl Ord for DomScore {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.level.cmp(&other.level).then_with(|| {
            self.value
                .partial_cmp(&other.value)
                .unwrap_or(core::cmp::Ordering::Equal)
        })
    }
}

pub struct VsidsCmpScore<'a, T> {
    scores: &'a [T],
}

impl<'a, T> VsidsCmpScore<'a, T> {
    pub const fn new(scores: &'a [T]) -> Self {
        Self { scores }
    }
}

impl<T> VsidsCmpScore<'_, T>
where
    T: PartialOrd,
{
    pub fn prefers(&self, lhs: Var_t, rhs: Var_t) -> bool {
        self.scores[lhs as usize] > self.scores[rhs as usize]
    }
}

pub struct DomainHeuristic {
    base: ClaspVsidsBase<DomScore>,
    prios_: Vec<DomainDomPrio>,
    actions_: Vec<DomainAction>,
    frames_: Vec<DomainFrame>,
    dom_seen_: u32,
    def_max_: u32,
    def_mod_: u16,
    def_pref_: u16,
}

impl Default for DomainHeuristic {
    fn default() -> Self {
        Self::new(HeuParams::default())
    }
}

impl DomainHeuristic {
    pub fn new(params: HeuParams) -> Self {
        Self {
            base: ClaspVsidsBase::new(params),
            prios_: Vec::new(),
            actions_: Vec::new(),
            frames_: Vec::new(),
            dom_seen_: 0,
            def_max_: 0,
            def_mod_: 0,
            def_pref_: 0,
        }
    }

    pub fn set_default_mod(&mut self, mod_: DomMod, pref_set: u32) {
        self.def_mod_ = mod_ as u16;
        self.def_pref_ = pref_set as u16;
    }

    pub fn set_config(&mut self, params: HeuParams) {
        self.base.set_config(params);
    }

    pub fn score(&self, var: Var_t) -> &DomScore {
        &self.base.score_[var as usize]
    }

    pub fn base(&self) -> &ClaspVsidsBase<DomScore> {
        &self.base
    }

    pub fn prio_table_len(&self) -> usize {
        self.prios_.len()
    }

    pub fn action_len(&self) -> usize {
        self.actions_.len()
    }

    pub fn frame_len(&self) -> usize {
        self.frames_.len()
    }

    pub fn dom_seen(&self) -> u32 {
        self.dom_seen_
    }

    pub fn def_max(&self) -> u32 {
        self.def_max_
    }

    pub fn def_mod(&self) -> u16 {
        self.def_mod_
    }

    pub fn def_pref(&self) -> u16 {
        self.def_pref_
    }
}
