//! Rust port of `original_clasp/clasp/solver_strategies.h`.

pub use crate::clasp::cli::clasp_cli_options::HeuristicType;
pub use crate::clasp::cli::clasp_cli_options::context_params::{ShareMode, ShortSimpMode};
pub use crate::clasp::cli::clasp_cli_options::heu_params::{DomMod, DomPref, Score, ScoreOther};
pub use crate::clasp::cli::clasp_cli_options::opt_params::{
    BBAlgo, Heuristic as OptHeuristic, Type as OptType, UscAlgo, UscOption, UscTrim,
};
pub use crate::clasp::cli::clasp_cli_options::reduce_strategy::{
    Algorithm as ReduceAlgorithm, Score as ReduceScore,
};
pub use crate::clasp::cli::clasp_cli_options::restart_params::SeqUpdate;
pub use crate::clasp::cli::clasp_cli_options::solver_params::Forget;
pub use crate::clasp::cli::clasp_cli_options::solver_strategies::{
    CCMinAntes, CCMinType, CCRepMode, LbdMode, SignHeu, UpdateMode, WatchInit,
};
use crate::clasp::constraint::{ConstraintInfo, ConstraintScore, Solver, lbd_max};
use crate::clasp::literal::{LitView, VarType, Wsum_t, weight_sum_min};
use crate::clasp::util::misc_types::{
    Event, EventLike, MovingAvg, MovingAvgType, Range32, Subsystem, Verbosity,
};
use crate::potassco::bits;
use crate::potassco::enums::EnumTag;

#[must_use]
pub fn grow_r(idx: u32, g: f64) -> f64 {
    g.powf(f64::from(idx))
}

#[must_use]
pub fn add_r(idx: u32, a: f64) -> f64 {
    a * f64::from(idx)
}

#[must_use]
pub fn luby_r(idx: u32) -> u32 {
    let mut i = idx + 1;
    while (i & (i + 1)) != 0 {
        i -= (1u32 << bits::log2(i)) - 1;
    }
    (i + 1) >> 1
}

fn saturate(value: f64) -> u64 {
    if value < (u64::MAX as f64) {
        value as u64
    } else {
        u64::MAX
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleType {
    Geom = 0,
    Arith = 1,
    Luby = 2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduleStrategy {
    pub base: u32,
    pub schedule_type: ScheduleType,
    pub idx: u32,
    pub len: u32,
    pub grow: f32,
}

impl Default for ScheduleStrategy {
    fn default() -> Self {
        Self::new(ScheduleType::Geom, 100, 1.5, 0)
    }
}

impl ScheduleStrategy {
    pub fn new(schedule_type: ScheduleType, base: u32, grow: f64, len: u32) -> Self {
        let mut strategy = Self {
            base,
            schedule_type,
            idx: 0,
            len,
            grow: 0.0,
        };
        match schedule_type {
            ScheduleType::Geom => {
                strategy.grow = grow.max(1.0) as f32;
            }
            ScheduleType::Arith => {
                strategy.grow = grow.max(0.0) as f32;
            }
            ScheduleType::Luby => {
                if len != 0 {
                    let pow = (len as f64).log2().ceil().exp2() as u32;
                    strategy.len = pow.saturating_sub(1).saturating_mul(2).max(2);
                }
            }
        }
        strategy
    }

    pub fn luby(unit: u32, limit: u32) -> Self {
        Self::new(ScheduleType::Luby, unit, 0.0, limit)
    }

    pub fn geom(base: u32, grow: f64, limit: u32) -> Self {
        Self::new(ScheduleType::Geom, base, grow, limit)
    }

    pub fn arith(base: u32, grow: u32, limit: u32) -> Self {
        Self::new(ScheduleType::Arith, base, f64::from(grow), limit)
    }

    pub fn fixed(base: u32) -> Self {
        Self::new(ScheduleType::Arith, base, 0.0, 0)
    }

    pub fn none() -> Self {
        Self::new(ScheduleType::Geom, 0, 1.5, 0)
    }

    pub fn def() -> Self {
        Self {
            base: 0,
            schedule_type: ScheduleType::Arith,
            idx: 0,
            len: 0,
            grow: 0.0,
        }
    }

    pub fn disabled(self) -> bool {
        self.base == 0
    }

    pub fn defaulted(self) -> bool {
        self.base == 0 && self.schedule_type == ScheduleType::Arith
    }

    pub fn current(self) -> u64 {
        if self.base == 0 {
            return u64::MAX;
        }
        match self.schedule_type {
            ScheduleType::Geom => {
                saturate(grow_r(self.idx, f64::from(self.grow)) * f64::from(self.base))
            }
            ScheduleType::Arith => {
                add_r(self.idx, f64::from(self.grow)) as u64 + u64::from(self.base)
            }
            ScheduleType::Luby => u64::from(luby_r(self.idx)) * u64::from(self.base),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.idx = self.idx.wrapping_add(1);
        if self.idx != self.len {
            return self.current();
        }
        self.len = (self.len + u32::from(self.idx != 0))
            << u32::from(self.schedule_type == ScheduleType::Luby);
        self.idx = 0;
        self.current()
    }

    pub fn advance_to(&mut self, n: u32) {
        if self.len == 0 || n < self.len {
            self.idx = n;
            return;
        }
        if self.schedule_type != ScheduleType::Luby {
            let d_len = f64::from(self.len);
            let x = ((f64::from(8) * (f64::from(n) + 1.0) + d_len * ((4.0 * d_len) - 4.0)).sqrt()
                - (2.0 * d_len)
                + 1.0)
                / 2.0;
            let x = x as u32;
            self.idx = n - (x * self.len + (((x - 1) as f64 * f64::from(x)) / 2.0) as u32);
            self.len += x;
            return;
        }
        let mut n = n;
        while n >= self.len {
            n -= self.len;
            self.len += 1;
            self.len *= 2;
        }
        self.idx = n;
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchStrategy {
    UseLearning = 0,
    NoLearning = 1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SolverStrategies {
    pub compress: u32,
    pub save_progress: u32,
    pub heu_id: u32,
    pub reverse_arcs: u32,
    pub otfs: u32,
    pub update_lbd: u32,
    pub cc_min_antes: u32,
    pub cc_rep_mode: u32,
    pub cc_min_rec: u32,
    pub cc_min_keep_act: u32,
    pub init_watches: u32,
    pub up_mode: u32,
    pub bump_var_act: u32,
    pub search: u32,
    pub restart_on_model: u32,
    pub reset_on_model: u32,
    pub sign_def: u32,
    pub sign_fix: u32,
    pub has_config: u32,
    pub id: u32,
}

impl SolverStrategies {
    pub fn prepare(&mut self) {
        if self.search == SearchStrategy::NoLearning as u32 {
            self.compress = 0;
            self.save_progress = 0;
            self.reverse_arcs = 0;
            self.otfs = 0;
            self.update_lbd = 0;
            self.cc_min_antes = CCMinAntes::NoAntes as u32;
            self.bump_var_act = 0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VsidsDecay {
    pub init: u32,
    pub bump: u32,
    pub freq: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeuParams {
    pub param: u32,
    pub score: u32,
    pub other: u32,
    pub moms: u32,
    pub nant: u32,
    pub huang: u32,
    pub acids: u32,
    pub dom_pref: u32,
    pub dom_mod: u32,
    pub extra: u32,
}

impl Default for HeuParams {
    fn default() -> Self {
        Self {
            param: 0,
            score: Score::ScoreAuto as u32,
            other: ScoreOther::OtherAuto as u32,
            moms: 1,
            nant: 0,
            huang: 0,
            acids: 0,
            dom_pref: 0,
            dom_mod: 0,
            extra: 0,
        }
    }
}

impl HeuParams {
    pub fn decay(self) -> VsidsDecay {
        VsidsDecay {
            init: self.extra & ((1 << 10) - 1),
            bump: (self.extra >> 10) & ((1 << 7) - 1),
            freq: (self.extra >> 17) & ((1 << 15) - 1),
        }
    }

    pub fn set_decay(&mut self, decay: VsidsDecay) {
        self.extra = (decay.init & ((1 << 10) - 1))
            | ((decay.bump & ((1 << 7) - 1)) << 10)
            | ((decay.freq & ((1 << 15) - 1)) << 17);
    }
}

#[must_use]
pub const fn is_lookback_heuristic(heuristic: HeuristicType) -> bool {
    (heuristic as u32) >= (HeuristicType::Berkmin as u32)
        && (heuristic as u32) < (HeuristicType::Unit as u32)
}

#[must_use]
pub fn is_lookback_heuristic_u32(heuristic: u32) -> bool {
    match HeuristicType::from_underlying(heuristic) {
        Some(value) => is_lookback_heuristic(value),
        None => false,
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartKeep {
    Never = 0,
    Restart = 1,
    Block = 2,
    Always = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestartSchedule {
    pub base: u32,
    pub schedule_type: u32,
    pub idx: u32,
    pub len: u32,
    pub grow: f32,
}

impl Default for RestartSchedule {
    fn default() -> Self {
        Self::from_schedule(ScheduleStrategy::default())
    }
}

impl RestartSchedule {
    pub fn from_schedule(schedule: ScheduleStrategy) -> Self {
        Self {
            base: schedule.base,
            schedule_type: schedule.schedule_type as u32,
            idx: schedule.idx,
            len: schedule.len,
            grow: schedule.grow,
        }
    }

    pub fn dynamic(
        base: u32,
        k: f32,
        lim: u32,
        fast: MovingAvgType,
        keep: RestartKeep,
        slow: MovingAvgType,
        slow_w: u32,
    ) -> Self {
        Self {
            base,
            schedule_type: 3,
            idx: (fast as u32)
                | ((slow as u32) << 3)
                | ((keep as u32 & 3) << 6)
                | (slow_w.min((1 << 24) - 1) << 8),
            len: lim,
            grow: k,
        }
    }

    pub fn disabled(self) -> bool {
        self.base == 0
    }

    pub fn is_dynamic(self) -> bool {
        self.schedule_type == 3
    }

    pub fn as_schedule(self) -> ScheduleStrategy {
        let schedule_type = match self.schedule_type {
            0 => ScheduleType::Geom,
            1 => ScheduleType::Arith,
            2 => ScheduleType::Luby,
            _ => ScheduleType::Geom,
        };
        ScheduleStrategy {
            base: self.base,
            schedule_type,
            idx: self.idx,
            len: self.len,
            grow: self.grow,
        }
    }

    pub fn k(self) -> f32 {
        self.grow
    }

    pub fn lbd_lim(self) -> u32 {
        self.len
    }

    pub fn fast_avg(self) -> MovingAvgType {
        match self.idx & 7 {
            0 => MovingAvgType::AvgSma,
            1 => MovingAvgType::AvgEma,
            2 => MovingAvgType::AvgEmaLog,
            3 => MovingAvgType::AvgEmaSmooth,
            4 => MovingAvgType::AvgEmaLogSmooth,
            _ => MovingAvgType::AvgSma,
        }
    }

    pub fn slow_avg(self) -> MovingAvgType {
        match (self.idx >> 3) & 7 {
            0 => MovingAvgType::AvgSma,
            1 => MovingAvgType::AvgEma,
            2 => MovingAvgType::AvgEmaLog,
            3 => MovingAvgType::AvgEmaSmooth,
            4 => MovingAvgType::AvgEmaLogSmooth,
            _ => MovingAvgType::AvgSma,
        }
    }

    pub fn keep_avg(self) -> RestartKeep {
        match (self.idx >> 6) & 3 {
            1 => RestartKeep::Restart,
            2 => RestartKeep::Block,
            3 => RestartKeep::Always,
            _ => RestartKeep::Never,
        }
    }

    pub fn slow_win(self) -> u32 {
        self.idx >> 8
    }

    pub fn adjust_lim(self) -> u32 {
        if self.lbd_lim() != u32::MAX {
            16_000
        } else {
            u32::MAX
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestartBlock {
    pub window: u32,
    pub fscale: u32,
    pub first: u32,
    pub avg: u32,
}

impl Default for RestartBlock {
    fn default() -> Self {
        Self {
            window: 0,
            fscale: 0,
            first: 0,
            avg: MovingAvgType::AvgEma as u32,
        }
    }
}

impl RestartBlock {
    pub fn scale(self) -> f64 {
        f64::from(self.fscale) / 100.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestartParams {
    pub rs_sched: RestartSchedule,
    pub block: RestartBlock,
    pub counter_restart: u32,
    pub counter_bump: u32,
    pub shuffle: u32,
    pub shuffle_next: u32,
    pub up_restart: u32,
    pub cnt_local: u32,
}

impl Default for RestartParams {
    fn default() -> Self {
        Self {
            rs_sched: RestartSchedule::default(),
            block: RestartBlock::default(),
            counter_restart: 0,
            counter_bump: 9_973,
            shuffle: 0,
            shuffle_next: 0,
            up_restart: SeqUpdate::SeqContinue as u32,
            cnt_local: 0,
        }
    }
}

impl RestartParams {
    pub fn disable(&mut self) {
        *self = Self::default();
        self.rs_sched = RestartSchedule::from_schedule(ScheduleStrategy::none());
    }

    pub fn disabled(self) -> bool {
        self.base() == 0
    }

    pub fn local(self) -> bool {
        self.cnt_local != 0
    }

    pub fn update(self) -> SeqUpdate {
        SeqUpdate::from_underlying(self.up_restart as u8).unwrap_or(SeqUpdate::SeqContinue)
    }

    pub fn base(self) -> u32 {
        self.rs_sched.base
    }

    pub fn prepare(&mut self, with_lookback: bool) -> u32 {
        if !with_lookback || self.disabled() {
            self.disable();
        }
        0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DynamicAdjust {
    pub limit: u32,
    pub restarts: u32,
    pub samples: u32,
    pub rk: f32,
    pub limit_type: u32,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicLimitType {
    LbdLimit = 0,
    LevelLimit = 1,
}

#[derive(Debug)]
pub struct DynamicLimit {
    global_lbd: MovingAvg,
    global_cfl: MovingAvg,
    avg: MovingAvg,
    num: u32,
    keep: RestartKeep,
    pub adjust: DynamicAdjust,
}

impl DynamicLimit {
    pub fn new(
        k: f32,
        window: u32,
        fast: MovingAvgType,
        keep: RestartKeep,
        slow: MovingAvgType,
        slow_win: u32,
        adjust_limit: u32,
    ) -> Self {
        let slow_window = if slow_win != 0 || slow == MovingAvgType::AvgSma {
            slow_win
        } else {
            200 * window.max(1)
        };
        let mut limit = Self {
            global_lbd: MovingAvg::new(slow_window, slow),
            global_cfl: MovingAvg::new(slow_window, slow),
            avg: MovingAvg::new(window.max(1), fast),
            num: 0,
            keep,
            adjust: DynamicAdjust::default(),
        };
        limit.reset_adjust(k, DynamicLimitType::LbdLimit, adjust_limit, false);
        limit
    }

    pub fn reset_adjust(
        &mut self,
        k: f32,
        limit_type: DynamicLimitType,
        limit: u32,
        reset_avg: bool,
    ) {
        self.adjust = DynamicAdjust {
            limit,
            restarts: 0,
            samples: 0,
            rk: k,
            limit_type: limit_type as u32,
        };
        if reset_avg {
            self.num = 0;
            self.avg.clear();
        }
    }

    pub fn block(&mut self) {
        self.reset_run(RestartKeep::Block);
    }

    fn reset_run(&mut self, keep: RestartKeep) {
        self.num = 0;
        if ((self.keep as u32) & (keep as u32)) == 0 {
            self.avg.clear();
        }
    }

    pub fn reset(&mut self) {
        self.global_lbd.clear();
        self.global_cfl.clear();
        self.reset_run(RestartKeep::Never);
    }

    pub fn update(&mut self, conflict_level: u32, lbd: u32) {
        self.adjust.samples = self.adjust.samples.saturating_add(1);
        self.global_cfl.push(conflict_level);
        self.global_lbd.push(lbd);
        self.num = self.num.saturating_add(1);
        let sample = if self.adjust.limit_type == DynamicLimitType::LbdLimit as u32 {
            lbd
        } else {
            conflict_level
        };
        self.avg.push(sample);
    }

    pub fn restart(&mut self, max_lbd: u32, min_k: f32) -> u32 {
        self.adjust.restarts = self.adjust.restarts.saturating_add(1);
        if self.adjust.limit != u32::MAX && self.adjust.samples >= self.adjust.limit {
            let next_type = if max_lbd != 0
                && self.global_average(DynamicLimitType::LbdLimit) > f64::from(max_lbd)
            {
                DynamicLimitType::LevelLimit
            } else {
                DynamicLimitType::LbdLimit
            };
            let mut next_k = self.adjust.rk;
            let mut next_limit = self.adjust.limit;
            if next_type as u32 == self.adjust.limit_type {
                let avg_restart = self.avg_restart();
                let stretched = self.num >= self.adjust.limit;
                if avg_restart >= 16_000.0 {
                    next_k += 0.1;
                    next_limit = 16_000;
                } else if stretched {
                    next_k += 0.05;
                    next_limit = next_limit.max(16_000).saturating_sub(10_000).max(16_000);
                } else if avg_restart >= 4_000.0 {
                    next_k += 0.05;
                } else if avg_restart >= 1_000.0 {
                    next_limit = next_limit.saturating_add(10_000);
                } else if next_k > min_k {
                    next_k -= 0.05;
                }
            }
            self.reset_adjust(next_k, next_type, next_limit, false);
        }
        self.reset_run(RestartKeep::Restart);
        self.adjust.limit
    }

    pub fn run_len(&self) -> u32 {
        self.num
    }

    pub fn reached(&self) -> bool {
        self.run_len() >= self.avg.win()
            && (self.moving_average() * f64::from(self.adjust.rk))
                > self.global_average(self.limit_type())
    }

    pub fn global_average(&self, limit_type: DynamicLimitType) -> f64 {
        match limit_type {
            DynamicLimitType::LbdLimit => self.global_lbd.get(),
            DynamicLimitType::LevelLimit => self.global_cfl.get(),
        }
    }

    pub fn moving_average(&self) -> f64 {
        self.avg.get()
    }

    pub fn avg_restart(&self) -> f64 {
        if self.adjust.restarts == 0 {
            0.0
        } else {
            f64::from(self.adjust.samples) / f64::from(self.adjust.restarts)
        }
    }

    pub fn limit_type(&self) -> DynamicLimitType {
        if self.adjust.limit_type == DynamicLimitType::LevelLimit as u32 {
            DynamicLimitType::LevelLimit
        } else {
            DynamicLimitType::LbdLimit
        }
    }
}

#[derive(Debug)]
pub struct BlockLimit {
    pub avg: MovingAvg,
    pub next: u64,
    pub n: u64,
    pub inc: u32,
    pub r: f32,
}

impl BlockLimit {
    pub fn new(window_size: u32, rf: f64, avg_type: MovingAvgType) -> Self {
        Self {
            avg: MovingAvg::new(window_size, avg_type),
            next: u64::from(window_size),
            n: 0,
            inc: 50,
            r: rf as f32,
        }
    }

    pub fn push(&mut self, n_assign: u32) -> bool {
        self.avg.push(n_assign);
        self.n = self.n.saturating_add(1);
        self.n >= self.next
    }

    pub fn scaled(&self) -> f64 {
        self.avg.get() * f64::from(self.r)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReduceStrategy {
    pub protect: u32,
    pub glue: u32,
    pub f_reduce: u32,
    pub f_restart: u32,
    pub score: u32,
    pub algo: u32,
    pub estimate: u32,
    pub no_glue: u32,
}

impl Default for ReduceStrategy {
    fn default() -> Self {
        Self {
            protect: 0,
            glue: 0,
            f_reduce: 75,
            f_restart: 0,
            score: ReduceScore::ScoreAct as u32,
            algo: ReduceAlgorithm::ReduceLinear as u32,
            estimate: 0,
            no_glue: 0,
        }
    }
}

impl ReduceStrategy {
    pub fn score_act(score: ConstraintScore) -> u32 {
        score.activity()
    }

    pub fn score_lbd(score: ConstraintScore) -> u32 {
        (lbd_max + 1) - score.lbd()
    }

    pub fn score_both(score: ConstraintScore) -> u32 {
        (score.activity() + 1) * Self::score_lbd(score)
    }

    pub fn compare_with(score_mode: u32, lhs: ConstraintScore, rhs: ConstraintScore) -> i32 {
        let first = if score_mode == ReduceScore::ScoreAct as u32 {
            Self::score_act(lhs) as i32 - Self::score_act(rhs) as i32
        } else if score_mode == ReduceScore::ScoreLbd as u32 {
            Self::score_lbd(lhs) as i32 - Self::score_lbd(rhs) as i32
        } else {
            0
        };
        if first != 0 {
            first
        } else {
            Self::score_both(lhs) as i32 - Self::score_both(rhs) as i32
        }
    }

    pub fn compare(self, lhs: ConstraintScore, rhs: ConstraintScore) -> i32 {
        Self::compare_with(self.score, lhs, rhs)
    }

    pub fn as_score(self, value: ConstraintScore) -> u32 {
        if self.score == ReduceScore::ScoreAct as u32 {
            Self::score_act(value)
        } else if self.score == ReduceScore::ScoreLbd as u32 {
            Self::score_lbd(value)
        } else {
            Self::score_both(value)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReduceParams {
    pub cfl_sched: ScheduleStrategy,
    pub grow_sched: ScheduleStrategy,
    pub strategy: ReduceStrategy,
    pub f_init: f32,
    pub f_max: f32,
    pub f_grow: f32,
    pub init_range: Range32,
    pub max_range: u32,
    pub mem_max: u32,
}

impl Default for ReduceParams {
    fn default() -> Self {
        Self {
            cfl_sched: ScheduleStrategy::none(),
            grow_sched: ScheduleStrategy::def(),
            strategy: ReduceStrategy::default(),
            f_init: 1.0 / 3.0,
            f_max: 3.0,
            f_grow: 1.1,
            init_range: Range32::new(10, u32::MAX),
            max_range: u32::MAX,
            mem_max: 0,
        }
    }
}

impl ReduceParams {
    pub fn disable(&mut self) {
        self.cfl_sched = ScheduleStrategy::none();
        self.grow_sched = ScheduleStrategy::none();
        self.strategy.f_reduce = 0;
        self.f_grow = 0.0;
        self.f_init = 0.0;
        self.f_max = 0.0;
        self.init_range = Range32::new(u32::MAX, u32::MAX);
        self.max_range = u32::MAX;
        self.mem_max = 0;
    }

    pub fn f_reduce(self) -> f32 {
        self.strategy.f_reduce as f32 / 100.0
    }

    pub fn f_restart(self) -> f32 {
        self.strategy.f_restart as f32 / 100.0
    }

    pub fn get_limit(base: u32, factor: f64, range: Range32) -> u32 {
        let limited = if factor != 0.0 {
            (f64::from(base) * factor).min(u32::MAX as f64) as u32
        } else {
            u32::MAX
        };
        range.clamp(limited)
    }

    pub fn prepare(&mut self, with_lookback: bool) -> u32 {
        if !with_lookback || self.f_reduce() == 0.0 {
            self.disable();
            return 0;
        }
        if self.cfl_sched.defaulted() && self.grow_sched.disabled() && !self.grow_sched.defaulted()
        {
            self.cfl_sched = ScheduleStrategy::arith(4_000, 600, 0);
        }
        if self.f_max != 0.0 {
            self.f_max = self.f_max.max(self.f_init);
        }
        0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FwdCheck {
    pub high_step: u32,
    pub high_pct: u32,
    pub sign_def: u32,
    pub disable: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolveParams {
    pub restart: RestartParams,
    pub reduce: ReduceParams,
    pub rand_runs: u32,
    pub rand_conf: u32,
    pub rand_prob: f32,
    pub fwd_check: FwdCheck,
}

impl Default for SolveParams {
    fn default() -> Self {
        Self {
            restart: RestartParams::default(),
            reduce: ReduceParams::default(),
            rand_runs: 0,
            rand_conf: 0,
            rand_prob: 0.0,
            fwd_check: FwdCheck::default(),
        }
    }
}

impl SolveParams {
    pub fn prepare(&mut self, with_lookback: bool) -> u32 {
        self.restart.prepare(with_lookback) | self.reduce.prepare(with_lookback)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OptParams {
    pub type_: u32,
    pub heus: u32,
    pub algo: u32,
    pub trim: u32,
    pub opts: u32,
    pub t_lim: u32,
    pub k_lim: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SatPreParams {
    pub type_: u32,
    pub lim_iters: u32,
    pub lim_time: u32,
    pub lim_frozen: u32,
    pub lim_clause: u32,
    pub lim_occ: u32,
}

impl Default for SatPreParams {
    fn default() -> Self {
        Self {
            type_: 0,
            lim_iters: 0,
            lim_time: 0,
            lim_frozen: 0,
            lim_clause: 4000,
            lim_occ: 0,
        }
    }
}

impl SatPreParams {
    pub fn clause_limit(self, num_clauses: u32) -> bool {
        self.lim_clause != 0 && num_clauses > (self.lim_clause * 1000)
    }

    pub fn occ_limit(self, pos: u32, neg: u32) -> bool {
        self.lim_occ != 0 && pos > (self.lim_occ - 1) && neg > (self.lim_occ - 1)
    }

    pub fn bce(self) -> u32 {
        if self.type_ != 0 { self.type_ - 1 } else { 0 }
    }

    pub fn disable_bce(&mut self) {
        self.type_ = self.type_.min(1);
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortMode {
    ShortImplicit = 0,
    ShortExplicit = 1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContextParams {
    pub sat_pre: SatPreParams,
    pub share_mode: u8,
    pub short_mode: u8,
    pub short_simp: u8,
    pub seed: u8,
    pub has_config: u8,
    pub cli_config: u8,
    pub stats: u8,
    pub reserved: u8,
}

pub trait Configuration {
    fn context(&self) -> &ContextParams;
    fn num_solver(&self) -> u32;
    fn num_search(&self) -> u32;
    fn solver(&self, idx: u32) -> &SolverParams;
    fn search(&self, idx: u32) -> &SolveParams;
}

pub trait UserConfiguration: Configuration {
    fn add_solver(&mut self, idx: u32) -> &mut SolverParams;
    fn add_search(&mut self, idx: u32) -> &mut SolveParams;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BasicSatConfig {
    pub context_options: ContextParams,
    solver: Vec<SolverParams>,
    search: Vec<SolveParams>,
}

impl BasicSatConfig {
    pub fn new() -> Self {
        Self {
            context_options: ContextParams {
                share_mode: ShareMode::ShareAuto as u8,
                short_mode: ShortMode::ShortImplicit as u8,
                seed: 1,
                ..ContextParams::default()
            },
            solver: vec![SolverParams::default()],
            search: vec![SolveParams::default()],
        }
    }

    pub fn prepare(&mut self) -> u32 {
        let mut warn = 0;
        let search_len = self.search.len() as u32;
        for (index, solver) in self.solver.iter_mut().enumerate() {
            warn |= solver.prepare();
            warn |= self.search[(index as u32 % search_len) as usize]
                .prepare(solver.search != SearchStrategy::NoLearning as u32);
            if solver.update_lbd == LbdMode::LbdFixed as u32
                && self.search[(index as u32 % search_len) as usize]
                    .reduce
                    .strategy
                    .protect
                    != 0
            {
                warn |= 8;
            }
        }
        warn
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn resize(&mut self, solver_count: u32, search_count: u32) {
        self.solver
            .resize(solver_count as usize, SolverParams::default());
        self.search
            .resize(search_count as usize, SolveParams::default());
        for (index, solver) in self.solver.iter_mut().enumerate() {
            solver.id = index as u32;
        }
    }
}

impl Configuration for BasicSatConfig {
    fn context(&self) -> &ContextParams {
        &self.context_options
    }

    fn num_solver(&self) -> u32 {
        self.solver.len() as u32
    }

    fn num_search(&self) -> u32 {
        self.search.len() as u32
    }

    fn solver(&self, idx: u32) -> &SolverParams {
        &self.solver[idx as usize % self.solver.len()]
    }

    fn search(&self, idx: u32) -> &SolveParams {
        &self.search[idx as usize % self.search.len()]
    }
}

impl UserConfiguration for BasicSatConfig {
    fn add_solver(&mut self, idx: u32) -> &mut SolverParams {
        while idx as usize >= self.solver.len() {
            let next = SolverParams {
                id: self.solver.len() as u32,
                ..SolverParams::default()
            };
            self.solver.push(next);
        }
        &mut self.solver[idx as usize]
    }

    fn add_search(&mut self, idx: u32) -> &mut SolveParams {
        if idx as usize >= self.search.len() {
            self.search.resize(idx as usize + 1, SolveParams::default());
        }
        &mut self.search[idx as usize]
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchLimits<'a> {
    pub used: u64,
    pub restart_conflicts: u64,
    pub dynamic: Option<&'a DynamicLimit>,
    pub block: Option<&'a BlockLimit>,
    pub local: bool,
    pub conflicts: u64,
    pub memory: u64,
    pub learnts: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct SolveEvent {
    pub base: Event,
    pub solver: *const Solver,
}

impl SolveEvent {
    pub fn new<T: 'static>(solver: &Solver, verbosity: Verbosity) -> Self {
        Self {
            base: Event::for_type::<T>(Subsystem::SubsystemSolve, verbosity),
            solver: solver as *const Solver,
        }
    }
}

impl EventLike for SolveEvent {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConflictEvent<'a> {
    pub base: SolveEvent,
    pub learnt: LitView<'a>,
    pub info: ConstraintInfo,
}

#[derive(Clone, Copy, Debug)]
struct ConflictEventTag;

impl<'a> ConflictEvent<'a> {
    pub fn new(solver: &Solver, learnt: LitView<'a>, info: ConstraintInfo) -> Self {
        Self {
            base: SolveEvent::new::<ConflictEventTag>(solver, Verbosity::VerbosityQuiet),
            learnt,
            info,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Model;

pub trait ModelHandler {
    fn on_model(&mut self, solver: &Solver, model: &Model) -> bool;

    fn on_unsat(&mut self, _solver: &Solver, _model: &Model) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LowerBound {
    pub level: u32,
    pub bound: Wsum_t,
}

impl Default for LowerBound {
    fn default() -> Self {
        Self {
            level: 0,
            bound: weight_sum_min,
        }
    }
}

impl LowerBound {
    pub fn active(self) -> bool {
        self.bound != weight_sum_min
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolverParams {
    pub compress: u32,
    pub save_progress: u32,
    pub heu_id: u32,
    pub reverse_arcs: u32,
    pub otfs: u32,
    pub update_lbd: u32,
    pub cc_min_antes: u32,
    pub cc_rep_mode: u32,
    pub cc_min_rec: u32,
    pub cc_min_keep_act: u32,
    pub init_watches: u32,
    pub up_mode: u32,
    pub bump_var_act: u32,
    pub search: u32,
    pub restart_on_model: u32,
    pub reset_on_model: u32,
    pub sign_def: u32,
    pub sign_fix: u32,
    pub has_config: u32,
    pub id: u32,
    pub heuristic: HeuParams,
    pub opt: OptParams,
    pub seed: u32,
    pub look_ops: u32,
    pub look_type: u32,
    pub loop_rep: u32,
    pub acyc_fwd: u32,
    pub forget_set: u32,
    pub reserved: u32,
}

impl Default for SolverParams {
    fn default() -> Self {
        Self {
            compress: 0,
            save_progress: 0,
            heu_id: HeuristicType::Def as u32,
            reverse_arcs: 0,
            otfs: 0,
            update_lbd: LbdMode::LbdFixed as u32,
            cc_min_antes: CCMinAntes::AllAntes as u32,
            cc_rep_mode: CCRepMode::CcNoReplace as u32,
            cc_min_rec: 0,
            cc_min_keep_act: 0,
            init_watches: WatchInit::WatchRand as u32,
            up_mode: UpdateMode::UpdateOnPropagate as u32,
            bump_var_act: 0,
            search: SearchStrategy::UseLearning as u32,
            restart_on_model: 0,
            reset_on_model: 0,
            sign_def: SignHeu::SignAtom as u32,
            sign_fix: 0,
            has_config: 0,
            id: 0,
            heuristic: HeuParams::default(),
            opt: OptParams::default(),
            seed: 1,
            look_ops: 0,
            look_type: 0,
            loop_rep: 0,
            acyc_fwd: 0,
            forget_set: 0,
            reserved: 0,
        }
    }
}

impl SolverParams {
    pub fn prepare(&mut self) -> u32 {
        let mut result = 0;
        if self.search == SearchStrategy::NoLearning as u32
            && is_lookback_heuristic_u32(self.heu_id)
        {
            self.heu_id = HeuristicType::None as u32;
            result |= 1;
        }
        if self.heu_id == HeuristicType::Unit as u32 {
            if VarType::from_underlying(self.look_type).is_none() {
                result |= 2;
                self.look_type = VarType::Atom as u32;
            }
            self.look_ops = 0;
        }
        if self.heu_id != HeuristicType::Domain as u32
            && (self.heuristic.dom_pref != 0 || self.heuristic.dom_mod != 0)
        {
            result |= 4;
            self.heuristic.dom_pref = 0;
            self.heuristic.dom_mod = 0;
        }
        SolverStrategies {
            compress: self.compress,
            save_progress: self.save_progress,
            heu_id: self.heu_id,
            reverse_arcs: self.reverse_arcs,
            otfs: self.otfs,
            update_lbd: self.update_lbd,
            cc_min_antes: self.cc_min_antes,
            cc_rep_mode: self.cc_rep_mode,
            cc_min_rec: self.cc_min_rec,
            cc_min_keep_act: self.cc_min_keep_act,
            init_watches: self.init_watches,
            up_mode: self.up_mode,
            bump_var_act: self.bump_var_act,
            search: self.search,
            restart_on_model: self.restart_on_model,
            reset_on_model: self.reset_on_model,
            sign_def: self.sign_def,
            sign_fix: self.sign_fix,
            has_config: self.has_config,
            id: self.id,
        }
        .prepare();
        if self.search == SearchStrategy::NoLearning as u32 {
            self.compress = 0;
            self.save_progress = 0;
            self.reverse_arcs = 0;
            self.otfs = 0;
            self.update_lbd = 0;
            self.cc_min_antes = CCMinAntes::NoAntes as u32;
            self.bump_var_act = 0;
        }
        result
    }

    pub fn forget_heuristic(self) -> bool {
        bits::test_mask(self.forget_set, Forget::ForgetHeuristic as u32)
    }

    pub fn forget_signs(self) -> bool {
        bits::test_mask(self.forget_set, Forget::ForgetSigns as u32)
    }

    pub fn forget_activities(self) -> bool {
        bits::test_mask(self.forget_set, Forget::ForgetActivities as u32)
    }

    pub fn forget_learnts(self) -> bool {
        bits::test_mask(self.forget_set, Forget::ForgetLearnts as u32)
    }

    pub fn set_id(&mut self, solver_id: u32) -> &mut Self {
        self.id = solver_id;
        self
    }
}

impl OptParams {
    pub fn new(opt_type: OptType) -> Self {
        Self {
            type_: opt_type as u32,
            ..Self::default()
        }
    }

    pub fn supports_splitting(self) -> bool {
        self.type_ != OptType::TypeUsc as u32
    }

    pub fn has_option(self, option: UscOption) -> bool {
        (self.opts & option as u32) != 0
    }

    pub fn has_heuristic(self, heuristic: OptHeuristic) -> bool {
        (self.heus & heuristic as u32) != 0
    }
}
