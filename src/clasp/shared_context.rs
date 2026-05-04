//! Partial Rust port of `original_clasp/clasp/shared_context.h` and
//! `original_clasp/src/shared_context.cpp`.
//!
//! This module ports the `ProblemStats` aggregate together with the minimal
//! Bundle A runtime seam needed before clause runtime: variable metadata,
//! a short-implication graph, and a concrete `SharedContext` owning the master
//! solver.

use crate::clasp::asp_preprocessor::SatPreprocessor;
use crate::clasp::clause::SharedLiterals;
use crate::clasp::cli::clasp_cli_options::{
    HeuristicType, ProjectMode,
    context_params::{ShareMode, ShortSimpMode},
};
use crate::clasp::constraint::{Antecedent, Constraint, ConstraintInfo, ConstraintType};
use crate::clasp::dependency_graph::ExtDepGraph;
use crate::clasp::literal::{Literal, VarType, lit_false, lit_true, true_value, value_free};
use crate::clasp::literal::{WeightLiteral, WeightT};
use crate::clasp::minimize_constraint::{MinimizeBuilder, SharedMinimizeData};
use crate::clasp::solver::Solver;
use crate::clasp::solver_strategies::{
    BasicSatConfig, Configuration, DomPref, Model, ModelHandler, ShortMode,
};
use crate::clasp::solver_types::SolverStats;
use crate::clasp::statistics::{StatisticMap, StatisticObject};
use crate::clasp::util::misc_types::{EnterEvent, EventLike, Range32, Subsystem, Verbosity};
use crate::potassco::basic_types::DomModifier;
use crate::potassco::bits::{
    store_clear_mask, store_set_mask, store_toggle_bit, test_any, test_mask,
};

const PROBLEM_STAT_KEYS: [&str; 8] = [
    "vars",
    "vars_eliminated",
    "vars_frozen",
    "constraints",
    "constraints_binary",
    "constraints_ternary",
    "acyc_edges",
    "complexity",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemVarStats {
    pub num: u32,
    pub eliminated: u32,
    pub frozen: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemConstraintStats {
    pub other: u32,
    pub binary: u32,
    pub ternary: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProblemStats {
    pub vars: ProblemVarStats,
    pub constraints: ProblemConstraintStats,
    pub acyc_edges: u32,
    pub complexity: u32,
}

impl ProblemStats {
    pub const fn num_constraints(&self) -> u32 {
        self.constraints.other + self.constraints.binary + self.constraints.ternary
    }

    pub fn diff(&mut self, other: &Self) {
        self.vars.num = self.vars.num.abs_diff(other.vars.num);
        self.vars.eliminated = self.vars.eliminated.abs_diff(other.vars.eliminated);
        self.vars.frozen = self.vars.frozen.abs_diff(other.vars.frozen);
        self.constraints.other = self.constraints.other.abs_diff(other.constraints.other);
        self.constraints.binary = self.constraints.binary.abs_diff(other.constraints.binary);
        self.constraints.ternary = self.constraints.ternary.abs_diff(other.constraints.ternary);
        self.acyc_edges = self.acyc_edges.abs_diff(other.acyc_edges);
    }

    pub fn accu(&mut self, other: &Self) {
        self.vars.num += other.vars.num;
        self.vars.eliminated += other.vars.eliminated;
        self.vars.frozen += other.vars.frozen;
        self.constraints.other += other.constraints.other;
        self.constraints.binary += other.constraints.binary;
        self.constraints.ternary += other.constraints.ternary;
        self.acyc_edges += other.acyc_edges;
    }

    pub const fn size() -> u32 {
        PROBLEM_STAT_KEYS.len() as u32
    }

    pub fn key(index: u32) -> &'static str {
        PROBLEM_STAT_KEYS
            .get(index as usize)
            .copied()
            .expect("problem statistic key index out of bounds")
    }

    pub fn at(&self, key: &str) -> StatisticObject<'_> {
        match key {
            "vars" => StatisticObject::from_value(&self.vars.num),
            "vars_eliminated" => StatisticObject::from_value(&self.vars.eliminated),
            "vars_frozen" => StatisticObject::from_value(&self.vars.frozen),
            "constraints" => StatisticObject::from_value(&self.constraints.other),
            "constraints_binary" => StatisticObject::from_value(&self.constraints.binary),
            "constraints_ternary" => StatisticObject::from_value(&self.constraints.ternary),
            "acyc_edges" => StatisticObject::from_value(&self.acyc_edges),
            "complexity" => StatisticObject::from_value(&self.complexity),
            _ => panic!("unknown ProblemStats key: {key}"),
        }
    }
}

impl StatisticMap for ProblemStats {
    fn size(&self) -> u32 {
        Self::size()
    }

    fn key(&self, index: u32) -> &str {
        Self::key(index)
    }

    fn at<'a>(&'a self, key: &str) -> StatisticObject<'a> {
        Self::at(self, key)
    }
}

pub trait EventObserver {
    fn on_event(&mut self, event: &dyn EventLike);
}

struct NoopEventObserver;

impl EventObserver for NoopEventObserver {
    fn on_event(&mut self, _event: &dyn EventLike) {}
}

pub struct EventHandler {
    verb: u16,
    sys: u16,
    observer: Box<dyn EventObserver>,
}

impl std::fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHandler")
            .field("verb", &self.verb)
            .field("sys", &self.sys)
            .finish_non_exhaustive()
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(Verbosity::VerbosityQuiet)
    }
}

impl EventHandler {
    const VERB_MASK: u32 = 15;
    const VERB_SHIFT: u32 = 2;

    pub fn new(verbosity: Verbosity) -> Self {
        Self::with_observer(verbosity, NoopEventObserver)
    }

    pub fn with_observer<O>(verbosity: Verbosity, observer: O) -> Self
    where
        O: EventObserver + 'static,
    {
        let mut handler = Self {
            verb: 0,
            sys: 0,
            observer: Box::new(observer),
        };
        let level = verbosity as u32;
        if level != 0 {
            let replicated = level | (level << 4) | (level << 8) | (level << 12);
            handler.verb = replicated as u16;
        }
        handler
    }

    pub fn set_observer<O>(&mut self, observer: O)
    where
        O: EventObserver + 'static,
    {
        self.observer = Box::new(observer);
    }

    pub fn set_verbosity(&mut self, sys: Subsystem, verb: Verbosity) {
        let shift = (sys as u32) << Self::VERB_SHIFT;
        let mut bits = u32::from(self.verb);
        store_clear_mask(&mut bits, Self::VERB_MASK << shift);
        store_set_mask(&mut bits, (verb as u32) << shift);
        self.verb = bits as u16;
    }

    pub fn set_active(&mut self, sys: Subsystem) -> bool {
        if sys as u16 != self.sys {
            self.sys = sys as u16;
            let verb = if sys == Subsystem::SubsystemSolve {
                Verbosity::VerbosityLow
            } else {
                Verbosity::VerbosityHigh
            };
            let enter = EnterEvent::new(sys, verb);
            self.dispatch(&enter);
            return true;
        }
        false
    }

    pub fn active(&self) -> Subsystem {
        match self.sys as u32 {
            0 => Subsystem::SubsystemFacade,
            1 => Subsystem::SubsystemLoad,
            2 => Subsystem::SubsystemPrepare,
            3 => Subsystem::SubsystemSolve,
            value => panic!("invalid subsystem id: {value}"),
        }
    }

    pub fn verbosity(&self, sys: Subsystem) -> u32 {
        (u32::from(self.verb) >> ((sys as u32) << Self::VERB_SHIFT)) & Self::VERB_MASK
    }

    pub fn dispatch(&mut self, event: &dyn EventLike) {
        if event.event().verb <= self.verbosity(self.event_subsystem(event)) {
            self.observer.on_event(event);
        }
    }

    fn event_subsystem(&self, event: &dyn EventLike) -> Subsystem {
        match event.event().system {
            0 => Subsystem::SubsystemFacade,
            1 => Subsystem::SubsystemLoad,
            2 => Subsystem::SubsystemPrepare,
            3 => Subsystem::SubsystemSolve,
            value => panic!("invalid subsystem id: {value}"),
        }
    }
}

impl ModelHandler for EventHandler {
    fn on_model(&mut self, _solver: &Solver, _model: &Model) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogEventType {
    Message = b'M' as isize,
    Warning = b'W' as isize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SolveMode {
    #[default]
    SolveOnce = 0,
    SolveMulti = 1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReportMode {
    #[default]
    Default = 0,
    Conflict = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputType {
    Var = 0,
    Pred = 1,
    Term = 2,
    Theory = 3,
}

pub trait OutputTheory {
    fn first(&mut self, model: &Model) -> Option<&str>;
    fn next(&mut self) -> Option<&str>;

    fn output_type(&self) -> OutputType {
        OutputType::Theory
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputPredicateEntry {
    pub name: String,
    pub cond: Literal,
    pub user: u32,
}

enum OutputTheoryEntry {
    Borrowed(*mut dyn OutputTheory),
    Owned(Box<dyn OutputTheory>),
}

impl core::fmt::Debug for OutputTheoryEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Borrowed(_) => f.write_str("BorrowedTheory"),
            Self::Owned(_) => f.write_str("OwnedTheory"),
        }
    }
}

impl OutputTheoryEntry {
    fn ptr(&self) -> *mut dyn OutputTheory {
        match self {
            Self::Borrowed(ptr) => *ptr,
            Self::Owned(theory) => &**theory as *const dyn OutputTheory as *mut dyn OutputTheory,
        }
    }
}

#[derive(Debug)]
pub struct OutputTable {
    preds: Vec<OutputPredicateEntry>,
    theories: Vec<OutputTheoryEntry>,
    projected: Vec<Literal>,
    vars: Range32,
    project_mode: ProjectMode,
    hide: char,
}

impl Default for OutputTable {
    fn default() -> Self {
        Self {
            preds: Vec::new(),
            theories: Vec::new(),
            projected: Vec::new(),
            vars: Range32::new(0, 0),
            project_mode: ProjectMode::Implicit,
            hide: '\0',
        }
    }
}

impl OutputTable {
    pub fn add_predicate(&mut self, name: impl Into<String>, cond: Literal, user: u32) {
        self.preds.push(OutputPredicateEntry {
            name: name.into(),
            cond,
            user,
        });
    }

    pub fn add_theory(&mut self, theory: &'static mut dyn OutputTheory) {
        self.theories.push(OutputTheoryEntry::Borrowed(theory));
    }

    pub fn add_owned_theory(&mut self, theory: Box<dyn OutputTheory>) {
        self.theories.push(OutputTheoryEntry::Owned(theory));
    }

    pub fn remove_theory(&mut self, theory: &'static mut dyn OutputTheory) -> bool {
        let target = theory as *mut dyn OutputTheory;
        let before = self.theories.len();
        self.theories
            .retain(|entry| !std::ptr::addr_eq(entry.ptr(), target));
        before != self.theories.len()
    }

    pub fn set_var_range(&mut self, vars: Range32) {
        self.vars = vars;
    }

    pub fn set_project_mode(&mut self, mode: ProjectMode) {
        self.project_mode = mode;
    }

    pub fn set_filter(&mut self, hide: char) {
        self.hide = hide;
    }

    pub fn filter_name(&self, name: &str) -> bool {
        name.is_empty() || (self.hide != '\0' && name.starts_with(self.hide))
    }

    pub fn filter(&mut self, start_pos: u32) -> u32 {
        let start = start_pos.min(self.num_preds()) as usize;
        let hide = self.hide;
        let mut filtered = self.preds[..start].to_vec();
        let mut removed = 0u32;
        for pred in self.preds.drain(start..) {
            let drop = pred.cond == lit_false
                || pred.name.is_empty()
                || (hide != '\0' && pred.name.starts_with(hide));
            if drop {
                removed += 1;
            } else {
                filtered.push(pred);
            }
        }
        self.preds = filtered;
        removed
    }

    pub fn set_predicate_condition(&mut self, index: u32, cond: Literal) {
        self.preds[index as usize].cond = cond;
    }

    pub fn sort_predicates_by<F>(&mut self, mut cmp: F, start_pos: u32)
    where
        F: FnMut(&OutputPredicateEntry, &OutputPredicateEntry) -> core::cmp::Ordering,
    {
        let start = start_pos.min(self.num_preds()) as usize;
        self.preds[start..].sort_by(|lhs, rhs| cmp(lhs, rhs));
    }

    pub fn pred_range(&self) -> &[OutputPredicateEntry] {
        &self.preds
    }

    pub fn vars_range(&self) -> core::ops::Range<u32> {
        self.vars.lo..self.vars.hi
    }

    pub fn project_mode(&self) -> ProjectMode {
        if self.project_mode != ProjectMode::Implicit {
            self.project_mode
        } else if self.has_project() {
            ProjectMode::Project
        } else {
            ProjectMode::Output
        }
    }

    pub fn has_project(&self) -> bool {
        !self.projected.is_empty()
    }

    pub fn proj_range(&self) -> &[Literal] {
        &self.projected
    }

    pub fn add_project(&mut self, literal: Literal) {
        self.projected.push(literal);
    }

    pub fn clear_project(&mut self) {
        self.projected.clear();
    }

    pub fn size(&self) -> u32 {
        self.num_preds() + self.num_vars()
    }

    pub fn num_preds(&self) -> u32 {
        self.preds.len() as u32
    }

    pub fn num_vars(&self) -> u32 {
        self.vars.hi.saturating_sub(self.vars.lo)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainValue {
    cond: Literal,
    var: u32,
    kind: u8,
    composite: bool,
    bias: i16,
    prio: u16,
}

impl DomainValue {
    pub fn new(var: u32, kind: DomModifier, bias: i16, prio: u16, cond: Literal) -> Self {
        Self {
            cond,
            var,
            kind: if (kind as u8) <= (DomModifier::Init as u8) {
                kind as u8
            } else {
                u8::from(kind == DomModifier::False)
            },
            composite: matches!(kind, DomModifier::True | DomModifier::False),
            bias,
            prio,
        }
    }

    pub fn has_condition(&self) -> bool {
        self.cond.var() != 0
    }

    pub fn cond(&self) -> Literal {
        self.cond
    }

    pub fn var(&self) -> u32 {
        self.var
    }

    pub fn kind(&self) -> DomModifier {
        if !self.composite {
            match self.kind {
                0 => DomModifier::Level,
                1 => DomModifier::Sign,
                2 => DomModifier::Factor,
                _ => DomModifier::Init,
            }
        } else if self.kind == 0 {
            DomModifier::True
        } else {
            DomModifier::False
        }
    }

    pub fn bias(&self) -> i16 {
        self.bias
    }

    pub fn prio(&self) -> u16 {
        self.prio
    }

    pub fn composite(&self) -> bool {
        self.composite
    }
}

#[derive(Debug, Default)]
pub struct DomainTable {
    entries: Vec<DomainValue>,
    seen: u32,
}

impl DomainTable {
    pub fn add(&mut self, var: u32, kind: DomModifier, bias: i16, prio: u16, cond: Literal) {
        if cond != lit_false && (kind != DomModifier::Init || cond == lit_true) {
            self.entries
                .push(DomainValue::new(var, kind, bias, prio, cond));
        }
    }

    pub fn simplify(&mut self) -> u32 {
        if self.seen as usize >= self.entries.len() {
            return self.size();
        }
        self.entries[self.seen as usize..].sort_by(|lhs, rhs| {
            lhs.cond()
                .cmp(&rhs.cond())
                .then_with(|| lhs.var().cmp(&rhs.var()))
        });
        let mut merged = self.entries[..self.seen as usize].to_vec();
        let mut index = self.seen as usize;
        while index < self.entries.len() {
            let var = self.entries[index].var();
            let cond = self.entries[index].cond();
            let mut next = index + 1;
            while next < self.entries.len()
                && self.entries[next].var() == var
                && self.entries[next].cond() == cond
            {
                next += 1;
            }
            if next - index == 1 {
                merged.push(self.entries[index]);
            } else {
                const NO_BIAS: i16 = i16::MAX;
                let mut prio = [0u16; 4];
                let mut bias = [NO_BIAS; 4];
                for entry in &self.entries[index..next] {
                    if !entry.composite() {
                        let slot = entry.kind as usize;
                        if entry.prio() >= prio[slot] {
                            prio[slot] = entry.prio();
                            bias[slot] = entry.bias();
                        }
                    } else {
                        if entry.prio() >= prio[DomModifier::Level as usize] {
                            prio[DomModifier::Level as usize] = entry.prio();
                            bias[DomModifier::Level as usize] = entry.bias();
                        }
                        if entry.prio() >= prio[DomModifier::Sign as usize] {
                            prio[DomModifier::Sign as usize] = entry.prio();
                            bias[DomModifier::Sign as usize] = if entry.kind() == DomModifier::True
                            {
                                1
                            } else {
                                -1
                            };
                        }
                    }
                }
                let mut start = 0usize;
                if bias[DomModifier::Level as usize] != NO_BIAS
                    && bias[DomModifier::Sign as usize] != NO_BIAS
                    && bias[DomModifier::Sign as usize] != 0
                    && prio[DomModifier::Level as usize] == prio[DomModifier::Sign as usize]
                {
                    merged.push(DomainValue::new(
                        var,
                        if bias[DomModifier::Sign as usize] > 0 {
                            DomModifier::True
                        } else {
                            DomModifier::False
                        },
                        bias[DomModifier::Level as usize],
                        prio[DomModifier::Level as usize],
                        cond,
                    ));
                    start = DomModifier::Sign as usize + 1;
                }
                for slot in start..=DomModifier::Init as usize {
                    if bias[slot] != NO_BIAS {
                        let kind = match slot as u8 {
                            0 => DomModifier::Level,
                            1 => DomModifier::Sign,
                            2 => DomModifier::Factor,
                            _ => DomModifier::Init,
                        };
                        merged.push(DomainValue::new(var, kind, bias[slot], prio[slot], cond));
                    }
                }
            }
            index = next;
        }
        self.entries = merged;
        self.seen = self.size();
        self.seen
    }

    pub fn reset(&mut self) {
        self.entries.clear();
        self.seen = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn size(&self) -> u32 {
        self.entries.len() as u32
    }

    pub fn iter(&self) -> impl Iterator<Item = &DomainValue> {
        self.entries.iter()
    }

    pub fn apply_default<F>(ctx: &SharedContext, mut action: F, def_filter: u32)
    where
        F: FnMut(Literal, u32, u32),
    {
        if (def_filter & DomPref::PrefShow as u32) != 0 || def_filter == 0 {
            let pref = if def_filter != 0 {
                DomPref::PrefShow as u32
            } else {
                DomPref::PrefAtom as u32
            };
            for pred in ctx.output.pred_range() {
                if def_filter != 0 || pred.user != 0 {
                    action(pred.cond, pref, pref);
                }
            }
            if def_filter != 0 {
                for var in ctx.output.vars_range() {
                    action(Literal::new(var, false), pref, pref);
                }
            } else {
                for var in ctx.vars() {
                    if ctx.var_info(var).atom() {
                        action(Literal::new(var, false), pref, pref);
                    }
                }
            }
        }
        if (def_filter & DomPref::PrefMin as u32) != 0 {
            if let Some(minimize) = ctx.minimize_no_create() {
                let mut last_weight = i32::MIN;
                let mut strat = DomPref::PrefShow as u32;
                for weight_lit in minimize.iter() {
                    if weight_lit.weight != last_weight && strat > DomPref::PrefDisj as u32 {
                        strat -= 1;
                        last_weight = weight_lit.weight;
                    }
                    action(weight_lit.lit, DomPref::PrefMin as u32, strat);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DistributorPolicy {
    pub size: u32,
    pub lbd: u32,
    pub types: u32,
}

impl DistributorPolicy {
    pub const fn new(size: u32, lbd: u32, types: u32) -> Self {
        Self { size, lbd, types }
    }

    pub const fn is_candidate_raw(&self, size: u32, lbd: u32, constraint_type: u32) -> bool {
        size <= self.size && lbd <= self.lbd && (self.types & constraint_type) != 0
    }

    pub const fn is_candidate_type(
        &self,
        size: u32,
        lbd: u32,
        constraint_type: ConstraintType,
    ) -> bool {
        self.is_candidate_raw(size, lbd, constraint_type.as_u32())
    }

    pub fn is_candidate_info(&self, size: u32, extra: ConstraintInfo) -> bool {
        size <= 3 || self.is_candidate_type(size, extra.lbd(), extra.constraint_type())
    }
}

pub trait Distributor {
    fn policy(&self) -> DistributorPolicy;

    fn is_candidate_raw(&self, size: u32, lbd: u32, constraint_type: u32) -> bool {
        self.policy().is_candidate_raw(size, lbd, constraint_type)
    }

    fn is_candidate_type(&self, size: u32, lbd: u32, constraint_type: ConstraintType) -> bool {
        self.policy().is_candidate_type(size, lbd, constraint_type)
    }

    fn is_candidate_info(&self, size: u32, extra: ConstraintInfo) -> bool {
        self.policy().is_candidate_info(size, extra)
    }

    fn publish(&mut self, source: &Solver, lits: *mut SharedLiterals);
    fn receive(&mut self, input: &Solver, out: &mut [*mut SharedLiterals]) -> u32;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEvent {
    pub base: crate::clasp::util::misc_types::Event,
    pub solver: Option<*const Solver>,
    pub msg: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedConflictEvent {
    pub base: crate::clasp::util::misc_types::Event,
    pub solver: *const Solver,
    pub learnt: Vec<Literal>,
    pub info: ConstraintInfo,
}

impl LogEvent {
    pub fn new(
        system: Subsystem,
        verbosity: Verbosity,
        kind: LogEventType,
        solver: Option<&Solver>,
        msg: impl Into<String>,
    ) -> Self {
        let mut base = crate::clasp::util::misc_types::Event::for_type::<Self>(system, verbosity);
        base.op = kind as u32;
        Self {
            base,
            solver: solver.map(|solver| solver as *const Solver),
            msg: msg.into(),
        }
    }

    pub fn is_warning(&self) -> bool {
        self.base.op == LogEventType::Warning as u32
    }
}

impl SharedConflictEvent {
    pub fn new(solver: &Solver, learnt: &[Literal], info: ConstraintInfo) -> Self {
        Self {
            base: crate::clasp::util::misc_types::Event::for_type::<Self>(
                Subsystem::SubsystemSolve,
                Verbosity::VerbosityQuiet,
            ),
            solver: solver as *const Solver,
            learnt: learnt.to_vec(),
            info,
        }
    }
}

impl EventLike for LogEvent {
    fn event(&self) -> &crate::clasp::util::misc_types::Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl EventLike for SharedConflictEvent {
    fn event(&self) -> &crate::clasp::util::misc_types::Event {
        &self.base
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VarInfo {
    pub rep: u8,
}

impl VarInfo {
    pub const FLAG_POS: u8 = 0x01;
    pub const FLAG_NEG: u8 = 0x02;
    pub const FLAG_INPUT: u8 = 0x04;
    pub const FLAG_BODY: u8 = 0x08;
    pub const FLAG_EQ: u8 = 0x10;
    pub const FLAG_NANT: u8 = 0x20;
    pub const FLAG_FROZEN: u8 = 0x40;
    pub const FLAG_OUTPUT: u8 = 0x80;

    pub const fn new(rep: u8) -> Self {
        Self { rep }
    }

    pub fn type_(self) -> VarType {
        if self.has(Self::FLAG_EQ) {
            VarType::Hybrid
        } else if self.has(Self::FLAG_BODY) {
            VarType::Body
        } else {
            VarType::Atom
        }
    }

    pub fn r#type(self) -> VarType {
        self.type_()
    }

    pub fn atom(self) -> bool {
        !matches!(self.type_(), VarType::Body)
    }

    pub fn nant(self) -> bool {
        self.has(Self::FLAG_NANT)
    }

    pub fn frozen(self) -> bool {
        self.has(Self::FLAG_FROZEN)
    }

    pub fn input(self) -> bool {
        self.has(Self::FLAG_INPUT)
    }

    pub fn output(self) -> bool {
        self.has(Self::FLAG_OUTPUT)
    }

    pub fn preferred_sign(self) -> bool {
        !self.has(Self::FLAG_BODY)
    }

    pub fn has(self, flag: u8) -> bool {
        test_mask(self.rep, flag)
    }

    pub fn has_any(self, flags: u8) -> bool {
        test_any(self.rep, flags)
    }

    pub fn has_all(self, flags: u8) -> bool {
        test_mask(self.rep, flags)
    }

    pub fn set(&mut self, flag: u8) {
        store_set_mask(&mut self.rep, flag);
    }

    pub fn clear(&mut self, flag: u8) {
        store_clear_mask(&mut self.rep, flag);
    }

    pub fn toggle(&mut self, flag: u8) {
        store_toggle_bit(&mut self.rep, flag.trailing_zeros());
    }

    pub fn set_to(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.set(flag);
        } else {
            self.clear(flag);
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ShortImplicationNode {
    binary: Vec<Literal>,
    ternary: Vec<[Literal; 2]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortImplicationsGraph {
    graph: Vec<ShortImplicationNode>,
    binary: [u32; 2],
    ternary: [u32; 2],
    shared: bool,
    simp_mode: ShortSimpMode,
}

impl Default for ShortImplicationsGraph {
    fn default() -> Self {
        Self {
            graph: Vec::new(),
            binary: [0; 2],
            ternary: [0; 2],
            shared: false,
            simp_mode: ShortSimpMode::SimpNo,
        }
    }
}

impl ShortImplicationsGraph {
    pub fn for_each<F>(&self, literal: Literal, mut op: F) -> bool
    where
        F: FnMut(Literal, Literal, Literal) -> bool,
    {
        let Some(node) = self.graph.get(literal.id() as usize) else {
            return true;
        };
        for &other in &node.binary {
            if !op(literal, other, lit_false) {
                return false;
            }
        }
        for &[first, second] in &node.ternary {
            if !op(literal, first, second) {
                return false;
            }
        }
        true
    }

    pub fn resize(&mut self, nodes: u32) {
        self.graph
            .resize_with(nodes as usize, ShortImplicationNode::default);
    }

    pub fn mark_shared(&mut self, shared: bool) {
        self.shared = shared;
    }

    pub const fn shared(&self) -> bool {
        self.shared
    }

    pub fn set_simp_mode(&mut self, mode: ShortSimpMode) {
        self.simp_mode = mode;
    }

    pub const fn simp_mode(&self) -> ShortSimpMode {
        self.simp_mode
    }

    pub fn size(&self) -> u32 {
        self.graph.len() as u32
    }

    pub const fn num_binary(&self) -> u32 {
        self.binary[0]
    }

    pub const fn num_ternary(&self) -> u32 {
        self.ternary[0]
    }

    pub const fn num_learnt(&self) -> u32 {
        self.binary[1] + self.ternary[1]
    }

    pub fn num_edges(&self, literal: Literal) -> u32 {
        self.graph
            .get(literal.id() as usize)
            .map(|node| (node.binary.len() + node.ternary.len()) as u32)
            .unwrap_or(0)
    }

    pub fn add(&mut self, lits: &[Literal], learnt: bool) -> bool {
        assert!((2..=3).contains(&lits.len()));
        let mut normalized = lits.to_vec();
        for lit in &mut normalized {
            lit.unflag();
        }
        let index = usize::from(learnt);
        let max_id = normalized.iter().map(|lit| (!*lit).id()).max().unwrap_or(0) + 1;
        if self.graph.len() < max_id as usize {
            self.resize(max_id);
        }
        let simplify = self.simp_mode == ShortSimpMode::SimpAll
            || (learnt && self.simp_mode == ShortSimpMode::SimpLearnt);
        let first = normalized[0];
        let second = normalized[1];
        if simplify && self.has_binary_arc(!first, second) {
            return true;
        }
        if normalized.len() == 3 {
            let third = normalized[2];
            if simplify && self.has_ternary_arc(!first, Self::canonical_pair(second, third)) {
                return true;
            }
        }
        let mut stored = normalized;
        if learnt {
            for lit in &mut stored {
                lit.flag();
            }
        }
        let added = if lits.len() == 2 {
            self.add_binary(stored[0], stored[1])
        } else {
            self.add_ternary(stored[0], stored[1], stored[2])
        };
        if added {
            if lits.len() == 2 {
                self.binary[index] += 1;
            } else {
                self.ternary[index] += 1;
            }
        }
        added
    }

    pub fn remove(&mut self, lits: &[Literal], learnt: bool) {
        assert!((2..=3).contains(&lits.len()));
        if lits.len() == 2 {
            if self.remove_binary_clause(lits[0], lits[1]) {
                self.binary[usize::from(learnt)] =
                    self.binary[usize::from(learnt)].saturating_sub(1);
            }
        } else if self.remove_ternary_clause(lits[0], lits[1], lits[2]) {
            self.ternary[usize::from(learnt)] = self.ternary[usize::from(learnt)].saturating_sub(1);
        }
    }

    pub fn remove_true(&mut self, solver: &Solver, literal: Literal) {
        let neg_index = (!literal).id() as usize;
        let pos_index = literal.id() as usize;
        let binaries = self
            .graph
            .get(neg_index)
            .map(|node| node.binary.clone())
            .unwrap_or_default();
        let sat_ternaries = self
            .graph
            .get(neg_index)
            .map(|node| node.ternary.clone())
            .unwrap_or_default();
        let cond_ternaries = self
            .graph
            .get(pos_index)
            .map(|node| node.ternary.clone())
            .unwrap_or_default();

        for other in binaries {
            self.remove_binary_arc(other, literal);
        }
        for pair in sat_ternaries {
            self.remove_ternary_arc(solver, pair, literal);
        }
        for pair in cond_ternaries {
            self.remove_ternary_arc(solver, pair, !literal);
        }

        if let Some(node) = self.graph.get_mut(neg_index) {
            node.binary.clear();
            node.ternary.clear();
        }
        if let Some(node) = self.graph.get_mut(pos_index) {
            node.binary.clear();
            node.ternary.clear();
        }
    }

    pub fn propagate(&self, solver: &mut Solver, literal: Literal) -> bool {
        let Some(node) = self.graph.get(literal.id() as usize) else {
            return true;
        };
        for &other in &node.binary {
            if solver.value(other.var()) != true_value(other)
                && !solver.force(other, Antecedent::from_literal(literal))
            {
                return false;
            }
        }
        for &[first, second] in &node.ternary {
            let first_value = solver.value(first.var());
            if first_value == true_value(first) {
                continue;
            }
            let second_value = solver.value(second.var());
            if second_value == true_value(second) {
                continue;
            }
            if first_value == value_free && second_value == value_free {
                continue;
            }
            if first_value != value_free {
                if !solver.force(second, Antecedent::from_literals(literal, !first)) {
                    return false;
                }
            } else if !solver.force(first, Antecedent::from_literals(literal, !second)) {
                return false;
            }
        }
        true
    }

    pub fn propagate_bin(
        &self,
        assignment: &mut crate::clasp::solver_types::Assignment,
        literal: Literal,
        level: u32,
    ) -> bool {
        let Some(node) = self.graph.get(literal.id() as usize) else {
            return true;
        };
        for &other in &node.binary {
            if !assignment.assign(other, level, Antecedent::from_literal(literal)) {
                return false;
            }
        }
        true
    }

    pub fn reverse_arc(
        &self,
        solver: &Solver,
        literal: Literal,
        max_level: u32,
        out: &mut Antecedent,
    ) -> bool {
        let Some(node) = self.graph.get(literal.id() as usize) else {
            return false;
        };
        for &other in &node.binary {
            if Self::is_reverse_literal(solver, other, max_level) {
                *out = Antecedent::from_literal(!other);
                return true;
            }
        }
        for &[first, second] in &node.ternary {
            if Self::is_reverse_literal(solver, first, max_level)
                && Self::is_reverse_literal(solver, second, max_level)
            {
                *out = Antecedent::from_literals(!first, !second);
                return true;
            }
        }
        false
    }

    fn add_binary(&mut self, first: Literal, second: Literal) -> bool {
        let first_node = &mut self.graph[(!first).id() as usize].binary;
        if first_node.contains(&second) {
            return false;
        }
        first_node.push(second);
        self.graph[(!second).id() as usize].binary.push(first);
        true
    }

    fn add_ternary(&mut self, first: Literal, second: Literal, third: Literal) -> bool {
        let first_pair = Self::canonical_pair(second, third);
        if self.graph[(!first).id() as usize]
            .ternary
            .contains(&first_pair)
        {
            return false;
        }
        self.graph[(!first).id() as usize].ternary.push(first_pair);
        self.graph[(!second).id() as usize]
            .ternary
            .push(Self::canonical_pair(first, third));
        self.graph[(!third).id() as usize]
            .ternary
            .push(Self::canonical_pair(first, second));
        true
    }

    fn canonical_pair(left: Literal, right: Literal) -> [Literal; 2] {
        if left.id() <= right.id() {
            [left, right]
        } else {
            [right, left]
        }
    }

    fn has_binary_arc(&self, watch: Literal, target: Literal) -> bool {
        self.graph
            .get(watch.id() as usize)
            .is_some_and(|node| node.binary.contains(&target))
    }

    fn has_ternary_arc(&self, watch: Literal, target: [Literal; 2]) -> bool {
        self.graph
            .get(watch.id() as usize)
            .is_some_and(|node| node.ternary.contains(&target))
    }

    fn remove_binary_clause(&mut self, first: Literal, second: Literal) -> bool {
        let left = self.erase_binary((!first).id() as usize, second);
        let right = self.erase_binary((!second).id() as usize, first);
        left | right
    }

    fn remove_ternary_clause(&mut self, first: Literal, second: Literal, third: Literal) -> bool {
        let mut removed = false;
        removed |= self.erase_ternary((!first).id() as usize, Self::canonical_pair(second, third));
        removed |= self.erase_ternary((!second).id() as usize, Self::canonical_pair(first, third));
        removed |= self.erase_ternary((!third).id() as usize, Self::canonical_pair(first, second));
        removed
    }

    fn remove_binary_arc(&mut self, other: Literal, satisfied: Literal) {
        self.binary[usize::from(other.flagged())] =
            self.binary[usize::from(other.flagged())].saturating_sub(1);
        let _ = self.erase_binary((!other).id() as usize, satisfied);
    }

    fn remove_ternary_arc(&mut self, solver: &Solver, pair: [Literal; 2], literal: Literal) {
        let learnt = usize::from(pair[0].flagged() || pair[1].flagged());
        self.ternary[learnt] = self.ternary[learnt].saturating_sub(1);
        for lit in pair {
            if let Some(node) = self.graph.get_mut((!lit).id() as usize) {
                let remove_index = node.ternary.iter().position(|candidate| {
                    candidate[0].id() == literal.id() || candidate[1].id() == literal.id()
                });
                if let Some(index) = remove_index {
                    node.ternary.swap_remove(index);
                }
            }
        }
        if solver.is_false(literal)
            && solver.value(pair[0].var()) == value_free
            && solver.value(pair[1].var()) == value_free
        {
            let clause = [pair[0], pair[1]];
            let _ = self.add(&clause, learnt != 0);
        }
    }

    fn erase_binary(&mut self, node_id: usize, target: Literal) -> bool {
        self.graph.get_mut(node_id).is_some_and(|node| {
            if let Some(index) = node
                .binary
                .iter()
                .position(|candidate| *candidate == target)
            {
                node.binary.swap_remove(index);
                true
            } else {
                false
            }
        })
    }

    fn erase_ternary(&mut self, node_id: usize, target: [Literal; 2]) -> bool {
        self.graph.get_mut(node_id).is_some_and(|node| {
            if let Some(index) = node
                .ternary
                .iter()
                .position(|candidate| *candidate == target)
            {
                node.ternary.swap_remove(index);
                true
            } else {
                false
            }
        })
    }

    fn is_reverse_literal(solver: &Solver, literal: Literal, max_level: u32) -> bool {
        solver.is_false(literal)
            && (solver.seen_literal(literal) || solver.level(literal.var()) < max_level)
    }
}

#[derive(Debug)]
struct SharedContextMinimize {
    builder: MinimizeBuilder,
    product: Option<Box<SharedMinimizeData>>,
}

impl Default for SharedContextMinimize {
    fn default() -> Self {
        Self {
            builder: MinimizeBuilder::new(),
            product: None,
        }
    }
}

impl SharedContextMinimize {
    fn add(&mut self, prio: WeightT, lit: WeightLiteral) {
        let _ = self.builder.add_literal(prio, lit);
    }

    fn get(&mut self, ctx: &mut SharedContext) {
        if self.builder.empty() {
            return;
        }
        if let Some(product) = self.product.take() {
            let _ = self.builder.add_shared(&product);
        }
        self.product = self.builder.build(ctx);
    }
}

pub struct SharedContext {
    stats: ProblemStats,
    output: OutputTable,
    dom_table: DomainTable,
    var_info: Vec<VarInfo>,
    btig: ShortImplicationsGraph,
    ext_graph: Option<ExtDepGraph>,
    config: BasicSatConfig,
    sat_prepro: Option<Box<SatPreprocessor>>,
    mini: Option<SharedContextMinimize>,
    master: Box<Solver>,
    // Attached solvers must keep a stable address because helper objects like
    // ClauseCreator cache raw solver pointers across later SharedContext calls.
    #[allow(clippy::vec_box)]
    solvers: Vec<Box<Solver>>,
    step_literal: Literal,
    frozen: bool,
    preserve_models: bool,
    preserve_shown: bool,
    preserve_heuristic: bool,
    progress: Option<EventHandler>,
    report_mode: ReportMode,
    share_problem: bool,
    share_learnts: bool,
    solve_mode: SolveMode,
    last_top_level: u32,
    winner: u32,
    distributor: Option<Box<dyn Distributor>>,
}

impl core::fmt::Debug for SharedContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SharedContext")
            .field("stats", &self.stats)
            .field("num_vars", &self.num_vars())
            .field("num_solvers", &self.num_solvers())
            .field("step_literal", &self.step_literal)
            .field("frozen", &self.frozen)
            .field("report_mode", &self.report_mode)
            .field("solve_mode", &self.solve_mode)
            .field("last_top_level", &self.last_top_level)
            .field("winner", &self.winner)
            .finish_non_exhaustive()
    }
}

impl Default for SharedContext {
    fn default() -> Self {
        let mut context = Self {
            stats: ProblemStats::default(),
            output: OutputTable::default(),
            dom_table: DomainTable::default(),
            var_info: vec![VarInfo::default()],
            btig: ShortImplicationsGraph::default(),
            ext_graph: None,
            config: BasicSatConfig::new(),
            sat_prepro: None,
            mini: None,
            master: Box::new(Solver::new()),
            solvers: Vec::new(),
            step_literal: lit_true,
            frozen: false,
            preserve_models: false,
            preserve_shown: false,
            preserve_heuristic: false,
            progress: None,
            report_mode: ReportMode::Default,
            share_problem: false,
            share_learnts: false,
            solve_mode: SolveMode::SolveOnce,
            last_top_level: 0,
            winner: 0,
            distributor: None,
        };
        context.refresh_solver_links();
        context
    }
}

impl SharedContext {
    fn mark_mask(literal: Literal) -> u8 {
        if literal.sign() {
            VarInfo::FLAG_NEG
        } else {
            VarInfo::FLAG_POS
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_solve_mode(&mut self, mode: SolveMode) {
        self.solve_mode = mode;
    }

    pub fn solve_mode(&self) -> SolveMode {
        self.solve_mode
    }

    pub fn default_dom_pref(&self) -> u32 {
        let solver = self.config.solver(0);
        if solver.heu_id == HeuristicType::Domain as u32 && solver.heuristic.dom_mod != 0 {
            solver.heuristic.dom_pref
        } else {
            1u32 << 31
        }
    }

    pub fn configuration(&self) -> &BasicSatConfig {
        &self.config
    }

    pub fn configuration_mut(&mut self) -> &mut BasicSatConfig {
        &mut self.config
    }

    pub fn output(&self) -> &OutputTable {
        &self.output
    }

    pub fn output_mut(&mut self) -> &mut OutputTable {
        &mut self.output
    }

    pub fn dom_table(&self) -> &DomainTable {
        &self.dom_table
    }

    pub fn dom_table_mut(&mut self) -> &mut DomainTable {
        &mut self.dom_table
    }

    pub fn ext_graph(&self) -> Option<&ExtDepGraph> {
        self.ext_graph.as_ref()
    }

    pub fn set_ext_graph(&mut self, ext_graph: Option<ExtDepGraph>) {
        self.ext_graph = ext_graph;
    }

    pub fn set_configuration(&mut self, mut config: BasicSatConfig) {
        let solver_count = self.num_solvers().max(1);
        let search_count = config.num_search().max(1);
        let _ = config.prepare();
        config.resize(solver_count, search_count);
        self.config = config;
        self.set_share_mode(match self.config.context().share_mode {
            1 => ShareMode::ShareProblem,
            2 => ShareMode::ShareLearnt,
            3 => ShareMode::ShareAll,
            4 => ShareMode::ShareAuto,
            _ => ShareMode::ShareNo,
        });
        self.set_short_mode(
            if self.config.context().short_mode == ShortMode::ShortExplicit as u8 {
                ShortMode::ShortExplicit
            } else {
                ShortMode::ShortImplicit
            },
            match self.config.context().short_simp {
                1 => ShortSimpMode::SimpLearnt,
                2 => ShortSimpMode::SimpAll,
                _ => ShortSimpMode::SimpNo,
            },
        );
        self.enable_stats(u32::from(self.config.context().stats));
        self.master.reset_config();
        for solver in &mut self.solvers {
            solver.reset_config();
        }
    }

    pub fn set_share_mode(&mut self, mode: ShareMode) {
        self.config.context_options.share_mode = mode as u8;
        let effective = if mode == ShareMode::ShareAuto && self.concurrency() > 1 {
            ShareMode::ShareAll
        } else {
            mode
        };
        match effective {
            ShareMode::ShareNo | ShareMode::ShareAuto => {
                self.set_physical_share_modes(false, false)
            }
            ShareMode::ShareProblem => self.set_physical_share_modes(true, false),
            ShareMode::ShareLearnt => self.set_physical_share_modes(false, true),
            ShareMode::ShareAll => self.set_physical_share_modes(true, true),
        }
    }

    pub fn set_short_mode(&mut self, mode: ShortMode, simp: ShortSimpMode) {
        self.config.context_options.short_mode = mode as u8;
        self.config.context_options.short_simp = simp as u8;
        self.btig.set_simp_mode(simp);
    }

    pub fn enable_stats(&mut self, level: u32) {
        if level != 0 {
            let _ = self.master.stats_mut().enable_extended();
            for solver in &mut self.solvers {
                let _ = solver.stats_mut().enable_extended();
            }
        }
    }

    pub fn sat_prepro(&self) -> Option<&SatPreprocessor> {
        self.sat_prepro.as_deref()
    }

    pub fn sat_prepro_mut(&mut self) -> Option<&mut SatPreprocessor> {
        self.sat_prepro.as_deref_mut()
    }

    pub fn add_minimize(&mut self, lit: WeightLiteral, prio: WeightT) {
        self.mini
            .get_or_insert_with(SharedContextMinimize::default)
            .add(prio, lit);
    }

    pub fn has_minimize(&self) -> bool {
        self.mini.is_some()
    }

    pub fn minimize(&mut self) -> Option<&SharedMinimizeData> {
        let mut mini = self.mini.take()?;
        mini.get(self);
        self.mini = Some(mini);
        self.mini.as_ref().and_then(|mini| mini.product.as_deref())
    }

    pub fn minimize_no_create(&self) -> Option<&SharedMinimizeData> {
        self.mini.as_ref().and_then(|mini| mini.product.as_deref())
    }

    pub fn remove_minimize(&mut self) {
        self.mini = None;
    }

    pub fn set_event_handler(&mut self, handler: Option<EventHandler>) {
        self.progress = handler;
    }

    pub fn set_event_handler_with_mode(
        &mut self,
        handler: Option<EventHandler>,
        report_mode: ReportMode,
    ) {
        self.progress = handler;
        self.report_mode = report_mode;
    }

    pub fn set_sat_prepro(&mut self, sat_prepro: Option<Box<SatPreprocessor>>) {
        self.sat_prepro = sat_prepro;
    }

    pub fn seed_solvers(&self) -> bool {
        self.config.context().seed != 0
    }

    pub fn winner(&self) -> u32 {
        self.winner
    }

    pub fn set_winner(&mut self, solver_id: u32) {
        self.winner = solver_id.min(self.concurrency());
    }

    pub fn distributor(&self) -> Option<&dyn Distributor> {
        self.distributor.as_deref()
    }

    pub fn set_distributor(&mut self, distributor: Option<Box<dyn Distributor>>) {
        self.distributor = distributor;
    }

    pub fn stats(&self) -> &ProblemStats {
        &self.stats
    }

    pub fn stats_mut(&mut self) -> &mut ProblemStats {
        &mut self.stats
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn set_preserve_models(&mut self, enabled: bool) {
        self.preserve_models = enabled;
    }

    pub fn preserve_models(&self) -> bool {
        self.preserve_models
    }

    pub fn set_preserve_shown(&mut self, enabled: bool) {
        self.preserve_shown = enabled;
    }

    pub fn preserve_shown(&self) -> bool {
        self.preserve_shown
    }

    pub fn set_preserve_heuristic(&mut self, enabled: bool) {
        self.preserve_heuristic = enabled;
    }

    pub fn preserve_heuristic(&self) -> bool {
        self.preserve_heuristic
    }

    pub fn ok(&self) -> bool {
        self.master_ref().decision_level() != 0
            || !self.master_ref().has_conflict()
            || self.master_ref().has_stop_conflict()
    }

    pub fn is_extended(&self) -> bool {
        self.stats.vars.frozen != 0
    }

    pub fn add_var(&mut self) -> u32 {
        self.add_typed_var(VarType::Atom, VarInfo::FLAG_NANT | VarInfo::FLAG_INPUT)
    }

    pub fn add_typed_var(&mut self, var_type: VarType, flags: u8) -> u32 {
        let mut info = VarInfo::new(flags);
        info.clear(VarInfo::FLAG_POS | VarInfo::FLAG_NEG);
        if matches!(var_type, VarType::Body) {
            info.set(VarInfo::FLAG_BODY);
        }
        if matches!(var_type, VarType::Hybrid) {
            info.set(VarInfo::FLAG_EQ);
        }
        self.var_info.push(info);
        self.stats.vars.num = self.num_vars() - u32::from(self.step_literal.var() != 0);
        let var = self.num_vars();
        self.master.acquire_problem_var(var);
        var
    }

    pub fn valid_var(&self, var: u32) -> bool {
        var != 0 && (var as usize) < self.var_info.len()
    }

    pub fn num_vars(&self) -> u32 {
        self.var_info.len().saturating_sub(1) as u32
    }

    pub fn vars(&self) -> impl Iterator<Item = u32> + '_ {
        1..(self.num_vars() + 1)
    }

    pub fn num_eliminated_vars(&self) -> u32 {
        self.stats.vars.eliminated
    }

    pub fn var_info(&self, var: u32) -> VarInfo {
        self.var_info[var as usize]
    }

    pub fn eliminated(&self, var: u32) -> bool {
        assert!(self.valid_var(var));
        !self.master_ref().assignment().valid(var)
    }

    pub fn marked(&self, literal: Literal) -> bool {
        assert!(self.valid_var(literal.var()));
        self.var_info(literal.var()).has(Self::mark_mask(literal))
    }

    pub fn mark(&mut self, literal: Literal) {
        assert!(self.valid_var(literal.var()));
        self.var_info[literal.var() as usize].set(Self::mark_mask(literal));
    }

    pub fn unmark_literal(&mut self, literal: Literal) {
        assert!(self.valid_var(literal.var()));
        self.var_info[literal.var() as usize].clear(Self::mark_mask(literal));
    }

    pub fn unmark_var(&mut self, var: u32) {
        assert!(self.valid_var(var));
        self.var_info[var as usize].clear(VarInfo::FLAG_POS | VarInfo::FLAG_NEG);
    }

    pub fn set_frozen(&mut self, var: u32, frozen: bool) {
        assert!(self.valid_var(var));
        let info = &mut self.var_info[var as usize];
        if info.frozen() != frozen {
            info.set_to(VarInfo::FLAG_FROZEN, frozen);
            if frozen {
                self.stats.vars.frozen += 1;
            } else {
                self.stats.vars.frozen -= 1;
            }
        }
    }

    pub fn step_literal(&self) -> Literal {
        self.step_literal
    }

    pub fn request_step_var(&mut self) {
        if self.step_literal == lit_true {
            self.step_literal = lit_false;
        }
    }

    pub fn require_step_var(&mut self) -> Literal {
        if self.step_literal.var() == 0 {
            let mut info = VarInfo::default();
            info.set(VarInfo::FLAG_FROZEN);
            self.var_info.push(info);
            self.stats.vars.frozen += 1;
            self.step_literal = Literal::new(self.num_vars(), false);
            self.btig.resize((self.num_vars() + 1) << 1);
        }
        self.step_literal
    }

    pub fn eliminate(&mut self, var: u32) {
        assert!(self.valid_var(var));
        assert!(!self.frozen);
        assert_eq!(self.master_ref().decision_level(), 0);
        if !self.eliminated(var) {
            self.stats.vars.eliminated += 1;
            self.master.eliminate_var(var);
        }
    }

    pub fn unfreeze(&mut self) -> bool {
        if !self.frozen() {
            return true;
        }
        self.frozen = false;
        self.btig.mark_shared(false);
        let root_level = self.master.root_level();
        self.master.pop_root_level(root_level)
            && self.btig.propagate(&mut self.master, lit_true)
            && self.unfreeze_step()
    }

    fn unfreeze_step(&mut self) -> bool {
        let tag = self.step_literal.var();
        let master_params = *self.config.solver(self.master.id());
        let _ = self.master.end_step(self.last_top_level, master_params);
        if tag == 0 {
            return !self.master.has_conflict();
        }
        self.btig.remove_true(&self.master, !self.step_literal);
        for solver in &mut self.solvers {
            if tag == 0 || solver.valid_var(tag) {
                let params = *self.config.solver(solver.id());
                let _ = solver.end_step(self.last_top_level, params);
            }
        }
        if !self.valid_var(tag + 1) {
            self.var_info[tag as usize] = VarInfo::default();
            self.pop_vars(1);
            self.stats.vars.num += 1;
        } else {
            debug_assert!(self.master.is_false(self.step_literal));
        }
        self.step_literal = lit_false;
        !self.master.has_conflict()
    }

    pub fn master(&mut self) -> &mut Solver {
        self.refresh_solver_links();
        &mut self.master
    }

    pub fn master_ref(&self) -> &Solver {
        &self.master
    }

    pub fn num_solvers(&self) -> u32 {
        1 + self.solvers.len() as u32
    }

    pub fn has_solver(&self, id: u32) -> bool {
        id == 0 || (id as usize) <= self.solvers.len()
    }

    pub fn concurrency(&self) -> u32 {
        self.num_solvers()
    }

    pub fn set_concurrency(&mut self, num_solvers: u32) {
        let target = num_solvers.max(1);
        while self.num_solvers() < target {
            let _ = self.push_solver();
        }
        while self.num_solvers() > target {
            let _ = self.solvers.pop();
        }
        let search_count = self.config.num_search().max(1);
        self.config.resize(target, search_count);
        if self.config.context().share_mode == ShareMode::ShareAuto as u8 {
            self.set_share_mode(ShareMode::ShareAuto);
        }
        self.refresh_solver_links();
    }

    pub fn push_solver(&mut self) -> &mut Solver {
        let solver_count = self.num_solvers() + 1;
        let search_count = self.config.num_search().max(1);
        self.config.resize(solver_count, search_count);
        let mut solver = Box::new(Solver::new());
        solver.set_id(self.num_solvers());
        solver.set_shared_context(self as *mut SharedContext);
        self.solvers.push(solver);
        self.solvers
            .last_mut()
            .expect("pushed solver must be present")
    }

    pub fn solver(&mut self, id: u32) -> Option<&mut Solver> {
        self.refresh_solver_links();
        if id == 0 {
            Some(&mut self.master)
        } else {
            self.solvers.get_mut(id as usize - 1).map(Box::as_mut)
        }
    }

    pub fn solver_ref(&self, id: u32) -> Option<&Solver> {
        if id == 0 {
            Some(&self.master)
        } else {
            self.solvers.get(id as usize - 1).map(Box::as_ref)
        }
    }

    pub fn start_add_constraints(&mut self) -> &mut Solver {
        self.start_add_constraints_with_guess(100)
    }

    pub fn start_add_constraints_with_guess(&mut self, constraint_guess: u32) -> &mut Solver {
        if !self.unfreeze() {
            return &mut self.master;
        }
        self.refresh_solver_links();
        let mut expected_size = (self.num_vars() + 1) << 1;
        if self.step_literal == lit_false
            || (self.step_literal == lit_true && self.solve_mode == SolveMode::SolveMulti)
        {
            expected_size += 2;
        }
        self.btig.resize(expected_size);
        let params = *self.config.solver(self.master.id());
        self.master.start_init(constraint_guess, params);
        &mut self.master
    }

    pub fn end_init(&mut self) -> bool {
        self.end_init_with_attach_all(false)
    }

    pub fn end_init_with_attach_all(&mut self, attach_all: bool) -> bool {
        self.refresh_solver_links();
        if self.master.strategies().has_config == 0 {
            let params = *self.config.solver(self.master.id());
            self.master.start_init(self.num_constraints(), params);
        }
        let master_stats = self.master_ref().stats().clone();
        Self::init_stats_from(&master_stats, &mut self.master);
        if self.step_literal == lit_false {
            self.require_step_var();
        }
        self.master.acquire_problem_var(self.num_vars());
        if self.step_literal.var() != 0 {
            let _ = self.master.force(!self.step_literal, Antecedent::new());
        }
        let mut ok = !self.master.has_conflict();
        if ok {
            let mut temp = self.sat_prepro.take();
            ok = temp.as_mut().is_none_or(|pre| pre.preprocess(self)) && self.master.end_init();
            self.sat_prepro = temp;
        }
        self.stats.constraints.other = self.master.num_constraints();
        self.stats.constraints.binary = self.btig.num_binary();
        self.stats.constraints.ternary = self.btig.num_ternary();
        self.stats.acyc_edges = self.ext_graph.as_ref().map_or(0, ExtDepGraph::edges);
        self.last_top_level = self
            .master_ref()
            .num_assigned_vars()
            .saturating_sub(u32::from(self.step_literal.var() != 0));
        self.stats.complexity = self.stats.complexity.max(self.problem_complexity());
        self.master.set_db_index(self.master.num_constraints());
        self.btig.mark_shared(self.concurrency() > 1);
        self.frozen = true;
        if !ok || !attach_all {
            return ok;
        }
        let solver_count = self.num_solvers();
        for solver_id in 1..solver_count {
            if !self.attach(solver_id) {
                return false;
            }
        }
        true
    }

    pub fn num_binary(&self) -> u32 {
        self.btig.num_binary()
    }

    pub fn num_ternary(&self) -> u32 {
        self.btig.num_ternary()
    }

    pub fn num_unary(&self) -> u32 {
        self.last_top_level
    }

    pub fn num_learnt_short(&self) -> u32 {
        self.btig.num_learnt()
    }

    pub fn num_constraints(&self) -> u32 {
        self.num_binary() + self.num_ternary() + self.master.num_constraints()
    }

    pub fn allow_implicit(&self, constraint_type: ConstraintType) -> bool {
        if matches!(constraint_type, ConstraintType::Static) {
            !self.physical_share_problem()
        } else {
            self.config.context().short_mode != ShortMode::ShortExplicit as u8
        }
    }

    pub fn add_imp(&mut self, lits: &[Literal], constraint_type: ConstraintType) -> i32 {
        if !self.allow_implicit(constraint_type) {
            return -1;
        }
        let learnt = !matches!(constraint_type, ConstraintType::Static);
        if !learnt && !self.frozen() {
            if let Some(mut sat_prepro) = self.sat_prepro.take() {
                let added = sat_prepro.add_clause(lits);
                self.sat_prepro = Some(sat_prepro);
                return i32::from(added);
            }
        }
        i32::from(self.btig.add(lits, learnt))
    }

    pub fn propagate(&mut self) -> bool {
        if !self.master.propagate() {
            return false;
        }
        if self.frozen() {
            return true;
        }
        let mut temp = self.sat_prepro.take();
        let ok = temp.as_mut().is_none_or(|pre| pre.propagate(self));
        self.sat_prepro = temp;
        ok
    }

    pub fn add_binary(&mut self, first: Literal, second: Literal) -> bool {
        self.add_imp(&[first, second], ConstraintType::Static) > 0
    }

    pub fn add_unary(&mut self, literal: Literal) -> bool {
        assert!(!self.frozen() || !self.physical_share_problem());
        self.master().acquire_problem_var(literal.var());
        self.master().force(literal, Antecedent::new())
    }

    pub fn add(&mut self, constraint: Box<Constraint>) {
        assert!(!self.frozen(), "Cannot add constraints to frozen program");
        let mut constraint = constraint;
        constraint.attach_to_solver(self.master());
        self.master().add_constraint(Box::into_raw(constraint));
    }

    pub fn add_ternary(&mut self, first: Literal, second: Literal, third: Literal) -> bool {
        self.add_imp(&[first, second, third], ConstraintType::Static) > 0
    }

    pub fn remove_imp(&mut self, lits: &[Literal], learnt: bool) {
        self.btig.remove(lits, learnt);
    }

    pub fn set_short_simp_mode(&mut self, mode: ShortSimpMode) {
        self.btig.set_simp_mode(mode);
    }

    pub fn reverse_arc(
        &self,
        solver: &Solver,
        literal: Literal,
        max_level: u32,
    ) -> Option<Antecedent> {
        let mut out = Antecedent::new();
        self.btig
            .reverse_arc(solver, literal, max_level, &mut out)
            .then_some(out)
    }

    pub fn physical_share_problem(&self) -> bool {
        self.share_problem
    }

    pub fn set_physical_share_problem(&mut self, enabled: bool) {
        self.share_problem = enabled;
    }

    pub fn set_physical_share_learnts(&mut self, enabled: bool) {
        self.share_learnts = enabled;
    }

    pub fn set_physical_share_modes(&mut self, problem: bool, learnts: bool) {
        self.share_problem = problem;
        self.share_learnts = learnts;
    }

    pub fn physical_share(&self, constraint_type: ConstraintType) -> bool {
        if matches!(constraint_type, ConstraintType::Static) {
            self.share_problem
        } else {
            self.share_learnts
        }
    }

    pub fn short_implications(&self) -> &ShortImplicationsGraph {
        &self.btig
    }

    pub fn attach(&mut self, solver_id: u32) -> bool {
        if !self.frozen || !self.has_solver(solver_id) {
            return false;
        }
        self.refresh_solver_links();
        if solver_id == 0 {
            return true;
        }

        let master_stats = self.master_ref().stats().clone();
        let master_num_vars = self.master_ref().num_vars();
        let master_watch_cap = ((master_num_vars + 1) << 1) as usize;
        let attached_start = self
            .sat_prepro
            .as_ref()
            .map(|_| {
                self.solver_ref(solver_id)
                    .map_or(0, Solver::num_vars)
                    .saturating_add(1)
            })
            .unwrap_or(u32::MAX);
        let master_trail = self
            .master_ref()
            .trail_view(0)
            .iter()
            .copied()
            .filter(|lit| !self.master_ref().aux_var(lit.var()))
            .collect::<Vec<_>>();
        let master_db = self.master_ref().constraint_db().to_vec();
        let master_enum = self.master_ref().enumeration_constraint();
        let other_ptr = match self.solver(solver_id) {
            Some(solver) => solver as *mut Solver,
            None => return false,
        };
        let ok = unsafe {
            let other = &mut *other_ptr;
            other.detach_local_runtime();
            Self::init_stats_from(&master_stats, other);
            other.begin_init();
            other.acquire_problem_var(master_num_vars);
            other.reserve_watch_capacity(master_watch_cap);
            if other.has_conflict() {
                false
            } else {
                let mut attached = true;
                for literal in &master_trail {
                    if !other.force(*literal, Antecedent::new()) {
                        attached = false;
                        break;
                    }
                }
                if attached && attached_start <= master_num_vars {
                    for var in attached_start..=master_num_vars {
                        if self.eliminated(var) && other.value(var) == value_free {
                            other.eliminate_var(var);
                        }
                    }
                }
                if attached && !other.clone_db(&master_db) {
                    attached = false;
                }
                if attached {
                    let enumeration = master_enum.and_then(|constraint_ptr| {
                        constraint_ptr
                            .as_ref()
                            .and_then(|constraint| constraint.clone_attach(other))
                            .map(Box::into_raw)
                    });
                    other.set_enumeration_constraint(enumeration);
                }
                attached && other.end_init() && {
                    other.set_db_index(other.num_constraints());
                    true
                }
            }
        };
        if !ok {
            self.detach(solver_id, false);
        }
        ok
    }

    /// Compatibility shim for the upstream `attach(Solver&)` overload.
    ///
    /// This wrapper is unsafe because Rust cannot safely express taking a
    /// mutable borrow of the context while simultaneously referencing a solver
    /// owned by that same context.
    ///
    /// # Safety
    ///
    /// `solver` must be either null or a valid pointer to a `Solver` that is
    /// still owned by this `SharedContext` for the duration of the call.
    pub unsafe fn attach_solver(&mut self, solver: *mut Solver) -> bool {
        if solver.is_null() {
            return false;
        }
        let solver = unsafe { &*solver };
        let belongs_to_self = solver.shared_context().is_some_and(|shared| {
            std::ptr::eq(shared as *const SharedContext, self as *const SharedContext)
        });
        belongs_to_self && self.attach(solver.id())
    }

    pub fn detach(&mut self, solver_id: u32, _reset: bool) {
        if solver_id == 0 {
            return;
        }
        self.refresh_solver_links();
        if let Some(solver) = self.solver(solver_id) {
            solver.detach_local_runtime();
        }
    }

    /// Compatibility shim for the upstream `detach(Solver&, bool)` overload.
    ///
    /// This wrapper is unsafe for the same aliasing reason as
    /// [`SharedContext::attach_solver`].
    ///
    /// # Safety
    ///
    /// `solver` must be either null or a valid pointer to a `Solver` that is
    /// still owned by this `SharedContext` for the duration of the call.
    pub unsafe fn detach_solver(&mut self, solver: *mut Solver, reset: bool) {
        if solver.is_null() {
            return;
        }
        let solver = unsafe { &*solver };
        if solver.shared_context().is_some_and(|shared| {
            std::ptr::eq(shared as *const SharedContext, self as *const SharedContext)
        }) {
            self.detach(solver.id(), reset);
        }
    }

    pub fn simplify(&mut self, assigned: &[Literal], shuffle: bool) {
        if !self.physical_share_problem() && !assigned.is_empty() {
            for &literal in assigned {
                if literal.id() < self.btig.size() {
                    self.btig.remove_true(&self.master, literal);
                }
            }
        }

        let master_ptr = self.master() as *mut Solver;
        let old_len = self.master_ref().num_constraints();
        let mut removed_before = Vec::new();
        let mut removed = 0u32;
        unsafe {
            let db = (*master_ptr).constraint_db_mut();
            let mut write = 0usize;
            for read in 0..db.len() {
                let constraint_ptr = db[read];
                let remove = constraint_ptr
                    .as_mut()
                    .is_some_and(|constraint| constraint.simplify(&mut *master_ptr, shuffle));
                if remove {
                    removed_before.push(read as u32);
                    removed += 1;
                    crate::clasp::constraint::Constraint::destroy_raw(
                        constraint_ptr,
                        Some(&mut *master_ptr),
                        false,
                    );
                } else {
                    db[write] = constraint_ptr;
                    write += 1;
                }
            }
            db.truncate(write);
            (*master_ptr).set_db_index(db.len() as u32);
        }
        if removed != 0 {
            for solver in &mut self.solvers {
                let db_index = solver.db_index();
                if db_index == old_len {
                    solver.set_db_index(db_index.saturating_sub(removed));
                } else if db_index != 0 {
                    let removed_in_prefix = removed_before
                        .iter()
                        .filter(|&&index| index < db_index)
                        .count() as u32;
                    solver.set_db_index(db_index.saturating_sub(removed_in_prefix));
                }
            }
        }
        self.stats.constraints.other = self.master_ref().num_constraints();
    }

    pub fn remove_constraint(&mut self, index: u32, detach: bool) {
        let master_ptr = self.master() as *mut Solver;
        let constraint_ptr = unsafe {
            let db = (*master_ptr).constraint_db_mut();
            assert!((index as usize) < db.len());
            db.remove(index as usize)
        };
        for solver in &mut self.solvers {
            if index < solver.db_index() {
                solver.set_db_index(solver.db_index().saturating_sub(1));
            }
        }
        unsafe {
            (*master_ptr).set_db_index((*master_ptr).num_constraints());
            crate::clasp::constraint::Constraint::destroy_raw(
                constraint_ptr,
                Some(&mut *master_ptr),
                detach,
            );
        }
        self.stats.constraints.other = self.master_ref().num_constraints();
    }

    pub fn problem_complexity(&self) -> u32 {
        if self.is_extended() {
            let mut total = self.num_binary() + self.num_ternary();
            for &constraint_ptr in self.master_ref().constraint_db() {
                unsafe {
                    if let Some(constraint) = constraint_ptr.as_ref() {
                        total += constraint.estimate_complexity(self.master_ref());
                    }
                }
            }
            total
        } else {
            self.num_constraints()
        }
    }

    pub fn init_stats(&self, solver: &mut Solver) {
        let master_stats = self.master_ref().stats().clone();
        Self::init_stats_from(&master_stats, solver);
    }

    pub fn solver_stats(&self, solver_id: u32) -> &SolverStats {
        self.solver_ref(solver_id)
            .unwrap_or_else(|| panic!("solver id out of range: {solver_id}"))
            .stats()
    }

    pub fn accu_stats<'a>(&self, out: &'a mut SolverStats) -> &'a SolverStats {
        out.accu_with_enable(self.master_ref().stats(), true);
        for solver in &self.solvers {
            out.accu_with_enable(solver.stats(), true);
        }
        out
    }

    pub fn add_post(&self, solver: &mut Solver) -> bool {
        let belongs_to_self = solver.shared_context().is_some_and(|shared| {
            std::ptr::eq(shared as *const SharedContext, self as *const SharedContext)
        });
        assert!(belongs_to_self, "solver not attached");
        self.config.add_post(solver)
    }

    pub fn set_heuristic(&self, solver: &mut Solver) {
        let belongs_to_self = solver.shared_context().is_some_and(|shared| {
            std::ptr::eq(shared as *const SharedContext, self as *const SharedContext)
        });
        assert!(belongs_to_self, "solver not attached");
        self.config.set_heuristic(solver);
    }

    pub fn preprocess_short(&mut self) -> bool {
        let master_ptr = self.master() as *mut Solver;
        let mut seen = Vec::new();
        let mut rewrites = Vec::new();
        let num_vars = unsafe { (*master_ptr).assignment().num_vars() };
        for var in 1..num_vars {
            if unsafe { (*master_ptr).assignment().value(var) } != value_free {
                continue;
            }
            for literal in [Literal::new(var, false), Literal::new(var, true)] {
                if self.marked(literal) {
                    continue;
                }
                rewrites.clear();
                let mut q_front = unsafe { (*master_ptr).assignment().assigned() };
                let mut ok = unsafe {
                    (*master_ptr).assignment_mut().assign(
                        literal,
                        0,
                        Antecedent::from_literal(lit_true),
                    )
                };
                while ok {
                    let assigned = unsafe { (*master_ptr).assignment().assigned() };
                    if q_front >= assigned {
                        break;
                    }
                    let propagated = unsafe { (*master_ptr).assignment().trail[q_front as usize] };
                    q_front += 1;
                    ok = self.btig.for_each(propagated, |premise, first, second| {
                        if second == lit_false {
                            return unsafe {
                                (*master_ptr).assignment_mut().assign(
                                    first,
                                    0,
                                    Antecedent::from_literal(premise),
                                )
                            };
                        }
                        let antecedent = Antecedent::from_literal(premise);
                        let first_value = unsafe { (*master_ptr).assignment().value(first.var()) };
                        let second_value =
                            unsafe { (*master_ptr).assignment().value(second.var()) };
                        if second_value == true_value(second) || first_value == true_value(first) {
                            let first_reason =
                                unsafe { (*master_ptr).assignment().reason(first.var()).as_u64() };
                            let second_reason =
                                unsafe { (*master_ptr).assignment().reason(second.var()).as_u64() };
                            if first_reason == antecedent.as_u64()
                                || second_reason == antecedent.as_u64()
                            {
                                rewrites.push(!premise);
                                rewrites.push(first);
                                rewrites.push(second);
                            }
                            return true;
                        }
                        if second_value == first_value {
                            return second_value == value_free;
                        }
                        if first_value != value_free {
                            let first_reason =
                                unsafe { (*master_ptr).assignment().reason(first.var()).as_u64() };
                            if first_reason == antecedent.as_u64() {
                                let mut tagged = first;
                                tagged.flag();
                                rewrites.push(tagged);
                                rewrites.push(!premise);
                                rewrites.push(second);
                            }
                            return unsafe {
                                (*master_ptr).assignment_mut().assign(
                                    second,
                                    0,
                                    Antecedent::from_literals(premise, !first),
                                )
                            };
                        }
                        let second_reason =
                            unsafe { (*master_ptr).assignment().reason(second.var()).as_u64() };
                        if second_reason == antecedent.as_u64() {
                            let mut tagged = second;
                            tagged.flag();
                            rewrites.push(tagged);
                            rewrites.push(!premise);
                            rewrites.push(first);
                        }
                        unsafe {
                            (*master_ptr).assignment_mut().assign(
                                first,
                                0,
                                Antecedent::from_literals(premise, !second),
                            )
                        }
                    });
                }
                if ok {
                    for clause in rewrites.chunks_exact_mut(3) {
                        let sat = !clause[0].flagged();
                        let learnt = clause[1].flagged() || clause[2].flagged();
                        clause[0].unflag();
                        self.btig.remove(&clause[..3], learnt);
                        if !sat {
                            let _ = self.btig.add(&clause[1..3], learnt);
                        }
                    }
                }
                unsafe {
                    let assign = (*master_ptr).assignment_mut();
                    while assign.last() != literal {
                        let last = assign.last();
                        if !self.marked(last) {
                            self.mark(last);
                            seen.push(last);
                        }
                        assign.undo_last();
                    }
                    assign.undo_last();
                }
                if !ok {
                    let _ = unsafe { (*master_ptr).force(!literal, Antecedent::new()) }
                        && unsafe { (*master_ptr).propagate() };
                    break;
                }
            }
        }
        while let Some(literal) = seen.pop() {
            self.unmark_var(literal.var());
        }
        self.master().simplify()
    }

    pub fn warn(&mut self, what: &str) {
        if let Some(handler) = &mut self.progress {
            let event = LogEvent::new(
                handler.active(),
                Verbosity::VerbosityQuiet,
                LogEventType::Warning,
                None,
                what,
            );
            handler.dispatch(&event);
        }
    }

    pub fn report(&mut self, what: &str, solver: Option<&Solver>) {
        if let Some(handler) = &mut self.progress {
            let event = LogEvent::new(
                handler.active(),
                Verbosity::VerbosityHigh,
                LogEventType::Message,
                solver,
                what,
            );
            handler.dispatch(&event);
        }
    }

    pub fn report_model(&mut self, solver: &Solver, model: &Model) -> bool {
        self.progress
            .as_mut()
            .is_none_or(|handler| handler.on_model(solver, model))
    }

    pub fn report_conflict(&mut self, solver: &Solver, learnt: &[Literal], info: &ConstraintInfo) {
        if self.report_mode == ReportMode::Conflict {
            if let Some(handler) = &mut self.progress {
                let event = SharedConflictEvent::new(solver, learnt, *info);
                handler.dispatch(&event);
            }
        }
    }

    pub fn enter(&mut self, system: Subsystem) {
        if let Some(handler) = &mut self.progress {
            let _ = handler.set_active(system);
        }
    }

    pub fn pop_vars(&mut self, mut n_vars: u32) {
        assert!(!self.frozen, "Cannot pop vars from frozen program");
        assert!(n_vars <= self.num_vars(), "Too many variables to pop");
        let new_vars = self.num_vars() - n_vars;
        let committed_vars = self.master_ref().num_vars();
        if new_vars >= committed_vars {
            self.var_info.truncate(new_vars as usize + 1);
            self.stats.vars.num -= n_vars;
        } else {
            for var in (new_vars + 1..=self.num_vars()).rev() {
                self.stats.vars.eliminated -= u32::from(self.eliminated(var));
                self.stats.vars.frozen -= u32::from(self.var_info(var).frozen());
                self.stats.vars.num -= 1;
                self.var_info.pop();
                n_vars -= 1;
                if n_vars == 0 {
                    break;
                }
            }
            let current_vars = self.num_vars();
            self.btig.resize((current_vars + 1) << 1);
            self.master.update_vars(current_vars);
            for solver in &mut self.solvers {
                solver.update_vars(current_vars);
            }
            self.last_top_level = self
                .last_top_level
                .min(self.master_ref().num_assigned_vars());
        }
        if self.step_literal.var() > self.num_vars() {
            self.step_literal = lit_false;
        }
    }

    pub(crate) fn implication_graph(&self) -> &ShortImplicationsGraph {
        &self.btig
    }

    pub(crate) fn implication_graph_mut(&mut self) -> &mut ShortImplicationsGraph {
        &mut self.btig
    }

    pub(crate) fn add_post_for_solver(&mut self, solver_id: u32) -> bool {
        if !self.ok() {
            return false;
        }
        let solver_ptr = match self.solver(solver_id) {
            Some(solver) => solver as *mut Solver,
            None => return false,
        };
        unsafe { self.config.add_post(&mut *solver_ptr) }
    }

    fn init_stats_from(master_stats: &SolverStats, solver: &mut Solver) {
        let _ = solver.stats_mut().enable(master_stats);
        solver.stats_mut().reset();
    }

    fn refresh_solver_links(&mut self) {
        let this = self as *mut SharedContext;
        self.master.set_shared_context(this);
        for solver in &mut self.solvers {
            solver.set_shared_context(this);
        }
    }
}
