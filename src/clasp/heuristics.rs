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
use crate::clasp::solver_strategies::{HeuParams, Score, ScoreOther};

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

impl ActivityScore for DomScore {
    fn get(&self) -> f64 {
        self.value
    }

    fn set(&mut self, value: f64) {
        self.value = value;
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
