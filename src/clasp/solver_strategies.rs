//! Rust port of the option value types needed by
//! `original_clasp/src/clasp_options.cpp`.

use crate::clasp::util::misc_types::MovingAvgType;

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
