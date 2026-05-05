//! Rust port of solver-independent facade types from
//! original_clasp/clasp/clasp_facade.h and original_clasp/src/clasp_facade.cpp.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};
use core::ptr::NonNull;

use crate::clasp::claspfwd::{Asp, ProblemType, ProgramBuilder as ProgramBuilderFwd};
use crate::clasp::constraint::Solver;
use crate::clasp::enumerator::EnumOptions;
use crate::clasp::literal::{LitVec, SumVec};
use crate::clasp::logic_program::AspOptions;
use crate::clasp::mt::parallel_solve::ParallelSolveOptions;
use crate::clasp::parser::ParserOptions;
use crate::clasp::shared_context::SharedContext;
use crate::clasp::solver_strategies::BasicSatConfig;
use crate::clasp::util::misc_types::{Event, EventLike, Subsystem, Verbosity};
use crate::potassco::clingo::{AbstractHeuristic, AbstractStatistics, PropagatorInit};

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

#[derive(Default)]
pub struct SolveOptions {
    pub base: ParallelSolveOptions,
    pub enumeration: EnumOptions,
}

impl SolveOptions {
    pub fn supported_solvers() -> u32 {
        ParallelSolveOptions::supported_solvers()
    }

    pub fn recommended_solvers() -> u32 {
        ParallelSolveOptions::recommended_solvers()
    }

    pub fn num_solver(&self) -> u32 {
        self.base.num_solver()
    }

    pub fn set_solvers(&mut self, solver_count: u32) {
        self.base.set_solvers(solver_count);
    }

    pub fn default_portfolio(&self) -> bool {
        self.base.default_portfolio()
    }

    pub fn models(&self) -> bool {
        self.enumeration.models()
    }

    pub fn optimize(&self) -> bool {
        self.enumeration.optimize()
    }
}

pub trait Configurator {
    fn detach(&mut self, _config: &ClaspConfig) {}

    fn add_propagators(&mut self, solver: &mut Solver) -> bool;

    fn set_heuristic(&mut self, solver: &mut Solver);
}

#[derive(Default)]
struct ConfiguratorSlot {
    ptr: Option<NonNull<()>>,
    notify_detach: bool,
}

pub struct ClaspConfig {
    pub sat: BasicSatConfig,
    pub solve: SolveOptions,
    pub asp: AspOptions,
    pub parse: ParserOptions,
    pub only_pre: bool,
    pub prepared: bool,
    tester: Option<Box<BasicSatConfig>>,
    configurator: ConfiguratorSlot,
}

impl Default for ClaspConfig {
    fn default() -> Self {
        Self {
            sat: BasicSatConfig::new(),
            solve: SolveOptions::default(),
            asp: AspOptions::default(),
            parse: ParserOptions::default(),
            only_pre: false,
            prepared: false,
            tester: None,
            configurator: ConfiguratorSlot::default(),
        }
    }
}

impl ClaspConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        if let Some(tester) = self.tester.as_mut() {
            tester.reset();
        }
        self.sat.reset();
        self.solve = SolveOptions::default();
        self.asp = AspOptions::default();
        self.prepared = false;
    }

    pub fn tester_config(&self) -> Option<&BasicSatConfig> {
        self.tester.as_deref()
    }

    pub fn add_tester_config(&mut self) -> &mut BasicSatConfig {
        self.tester
            .get_or_insert_with(|| Box::new(BasicSatConfig::new()))
    }

    pub fn set_configurator(
        &mut self,
        configurator: Option<&mut dyn Configurator>,
        notify_detach: bool,
    ) {
        self.configurator.ptr = configurator.map(|cfg| NonNull::from(cfg).cast::<()>());
        self.configurator.notify_detach = notify_detach;
    }

    pub fn has_configurator(&self) -> bool {
        self.configurator.ptr.is_some()
    }

    pub fn notify_detach(&self) -> bool {
        self.configurator.notify_detach
    }
}

struct SolveStrategy {
    _private: (),
}

#[derive(Default)]
pub struct SolveHandle {
    strat_: Option<NonNull<SolveStrategy>>,
}

impl SolveHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_attached(&self) -> bool {
        self.strat_.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    pub facade: Option<NonNull<ClaspFacade>>,
    pub total_time: f64,
    pub cpu_time: f64,
    pub solve_time: f64,
    pub unsat_time: f64,
    pub sat_time: f64,
    pub kill_time: f64,
    pub num_enum: u64,
    pub num_optimal: u64,
    pub step: u32,
    pub result: SolveResult,
}

impl Summary {
    pub fn init(&mut self, facade: &ClaspFacade) {
        self.facade = Some(NonNull::from(facade));
    }

    pub fn lp_step(&self) -> Option<&Asp::LpStats> {
        None
    }

    pub fn lp_stats(&self) -> Option<&Asp::LpStats> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StepStart {
    pub base: Event,
    pub facade: Option<NonNull<ClaspFacade>>,
}

impl StepStart {
    pub fn new(facade: &ClaspFacade) -> Self {
        Self {
            base: Event::for_type::<Self>(Subsystem::SubsystemFacade, Verbosity::VerbosityQuiet),
            facade: Some(NonNull::from(facade)),
        }
    }
}

impl EventLike for StepStart {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StepReady {
    pub base: Event,
    pub summary: Option<NonNull<Summary>>,
}

impl StepReady {
    pub fn new(summary: &Summary) -> Self {
        Self {
            base: Event::for_type::<Self>(Subsystem::SubsystemFacade, Verbosity::VerbosityQuiet),
            summary: Some(NonNull::from(summary)),
        }
    }
}

impl EventLike for StepReady {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Prepare {
    pub base: Event,
    pub facade: Option<NonNull<ClaspFacade>>,
}

impl Prepare {
    pub fn new(facade: &mut ClaspFacade) -> Self {
        Self {
            base: Event::for_type::<Self>(Subsystem::SubsystemFacade, Verbosity::VerbosityQuiet),
            facade: Some(NonNull::from(facade)),
        }
    }
}

impl EventLike for Prepare {
    fn event(&self) -> &Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct FacadeStatistics {
    _private: (),
}

struct SolveData {
    _private: (),
}

pub struct ClaspFacade {
    pub ctx: SharedContext,
    type_: ProblemType,
    step_: Summary,
    assume_: LitVec,
    lower_: SumVec,
    config_: Option<NonNull<ClaspConfig>>,
    builder_: Option<NonNull<ProgramBuilderFwd>>,
    propagators_: Vec<NonNull<dyn PropagatorInit>>,
    heuristic_: Option<NonNull<dyn AbstractHeuristic>>,
    accu_: Option<Box<Summary>>,
    stats_: Option<NonNull<FacadeStatistics>>,
    solve_: Option<NonNull<SolveData>>,
}

impl Default for ClaspFacade {
    fn default() -> Self {
        Self {
            ctx: SharedContext::default(),
            type_: ProblemType::Sat,
            step_: Summary::default(),
            assume_: LitVec::new(),
            lower_: SumVec::new(),
            config_: None,
            builder_: None,
            propagators_: Vec::new(),
            heuristic_: None,
            accu_: None,
            stats_: None,
            solve_: None,
        }
    }
}

impl ClaspFacade {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn problem_type(&self) -> ProblemType {
        self.type_
    }

    pub fn summary(&self) -> &Summary {
        &self.step_
    }

    pub fn assumptions(&self) -> &[crate::clasp::literal::Literal] {
        self.assume_.as_slice()
    }

    pub fn lower_bounds(&self) -> &[i64] {
        self.lower_.as_slice()
    }

    pub fn config(&self) -> Option<&ClaspConfig> {
        self.config_.map(|ptr| {
            // SAFETY: This layout-only slot is only reborrowed when the caller
            // already manages the pointed-to configuration lifetime.
            unsafe { ptr.as_ref() }
        })
    }

    pub fn program(&self) -> Option<NonNull<ProgramBuilderFwd>> {
        self.builder_
    }

    pub fn num_propagators(&self) -> usize {
        self.propagators_.len()
    }

    pub fn has_heuristic(&self) -> bool {
        self.heuristic_.is_some()
    }

    pub fn has_accu_summary(&self) -> bool {
        self.accu_.is_some()
    }

    pub fn has_stats(&self) -> bool {
        self.stats_.is_some()
    }

    pub fn has_solve_state(&self) -> bool {
        self.solve_.is_some()
    }
}

pub type Result = SolveResult;
pub type FacadeStatisticsView = dyn AbstractStatistics;
