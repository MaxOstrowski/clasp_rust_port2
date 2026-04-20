//! Rust port of `original_clasp/clasp/constraint.h` and `original_clasp/src/constraint.cpp`.

use core::cmp::Ordering;
use core::ffi::c_void;
use core::ptr::NonNull;

use crate::clasp::literal::{LitVec, Literal, ValT, value_free, value_true};
use crate::clasp::shared_context::SharedContext;
use crate::clasp::solver_strategies::{SolverStrategies, WatchInit};
use crate::clasp::solver_types::{Assignment, ClauseWatch, GenericWatch, SolverStats, WatchList};
use crate::potassco::bits::{
    BitIndex, Bitset, nth_bit, right_most_bit, store_clear_bit, store_clear_mask, store_set_mask,
    store_toggle_bit, test_any, test_bit,
};
use crate::potassco::enums::EnumTag;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConstraintType {
    #[default]
    Static = 0,
    Conflict = 1,
    Loop = 2,
    Other = 3,
}

impl ConstraintType {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Static),
            1 => Some(Self::Conflict),
            2 => Some(Self::Loop),
            3 => Some(Self::Other),
            _ => None,
        }
    }
}

impl BitIndex for ConstraintType {
    fn bit_index(self) -> u32 {
        self as u32
    }
}

pub type TypeSet = Bitset<u32, ConstraintType>;

#[derive(Debug)]
pub struct ClauseHead {
    pub(crate) info: ConstraintInfo,
    pub(crate) head: [Literal; 3],
    pub(crate) constraint: *mut Constraint,
    pub(crate) owner: *mut c_void,
}

impl Default for ClauseHead {
    fn default() -> Self {
        Self::new(ConstraintInfo::default())
    }
}

impl ClauseHead {
    pub fn new(info: ConstraintInfo) -> Self {
        Self {
            info,
            head: [Literal::default(), Literal::default(), Literal::default()],
            constraint: core::ptr::null_mut(),
            owner: core::ptr::null_mut(),
        }
    }

    pub fn propagate(&mut self, solver: &mut Solver, literal: Literal) -> PropResult {
        crate::clasp::clause::clause_head_propagate(self, solver, literal)
    }

    pub fn attach(&mut self, solver: &mut Solver) {
        if self.head[0] == self.head[1] {
            return;
        }
        solver.add_clause_watch(!self.head[0], self as *mut Self);
        solver.add_clause_watch(!self.head[1], self as *mut Self);
    }

    pub fn detach(&mut self, solver: &mut Solver) {
        solver.remove_clause_watch(!self.head[0], self as *mut Self);
        solver.remove_clause_watch(!self.head[1], self as *mut Self);
    }

    pub fn locked(&self, solver: &Solver) -> bool {
        crate::clasp::clause::clause_head_locked(self, solver)
    }

    pub fn satisfied(&self, solver: &Solver) -> bool {
        crate::clasp::clause::clause_head_satisfied(self, solver)
    }

    pub fn reset_score(&mut self, score: ConstraintScore) {
        self.info.set_score(score);
    }

    pub fn tagged(&self) -> bool {
        self.info.tagged()
    }

    pub fn aux(&self) -> bool {
        self.info.aux()
    }

    pub fn learnt(&self) -> bool {
        self.info.learnt()
    }

    pub fn lbd(&self) -> u32 {
        self.info.lbd()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        self.info.constraint_type()
    }

    pub fn activity(&self) -> ConstraintScore {
        self.info.score()
    }

    pub fn decrease_activity(&mut self) {
        let mut score = self.info.score();
        score.reduce();
        self.info.set_score(score);
    }

    pub fn reset_activity(&mut self) {
        let mut score = self.info.score();
        score.reset(0, self.info.lbd());
        self.info.set_score(score);
    }

    pub fn size(&self) -> u32 {
        crate::clasp::clause::clause_head_size(self)
    }

    pub fn to_lits(&self) -> Vec<Literal> {
        crate::clasp::clause::clause_head_to_lits(self)
    }

    pub fn simplify(&mut self, solver: &mut Solver, reinit: bool) -> bool {
        crate::clasp::clause::clause_head_simplify(self, solver, reinit)
    }

    pub fn destroy(&mut self, solver: Option<&mut Solver>, detach: bool) {
        crate::clasp::clause::clause_head_destroy(self, solver, detach)
    }

    pub fn clone_attach(&self, other: &mut Solver) -> *mut ClauseHead {
        crate::clasp::clause::clause_head_clone_attach(self, other)
    }

    pub fn strengthen(
        &mut self,
        solver: &mut Solver,
        literal: Literal,
        allow_to_short: bool,
    ) -> ClauseStrengthenResult {
        crate::clasp::clause::clause_head_strengthen(self, solver, literal, allow_to_short)
    }

    pub fn constraint_ptr(&self) -> *mut Constraint {
        self.constraint
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClauseStrengthenResult {
    pub lit_removed: bool,
    pub remove_clause: bool,
}

#[derive(Debug, Default)]
pub struct CCMinRecursive;

pub trait DecisionHeuristic {
    fn new_constraint(&mut self, _solver: &Solver, _lits: &[Literal], _ty: ConstraintType) {}
}

#[derive(Debug, Default)]
pub struct SelectFirst;

impl DecisionHeuristic for SelectFirst {}

pub struct Solver {
    shared: Option<NonNull<SharedContext>>,
    id: u32,
    num_vars: u32,
    num_problem_vars: u32,
    has_conflict: bool,
    conflict_literal: Literal,
    conflict_reason: Antecedent,
    conflict_data: u32,
    decision_level: u32,
    root_level: u32,
    backtrack_level: u32,
    tag_literal: Literal,
    decisions: Vec<Literal>,
    level_starts: Vec<u32>,
    root_levels: Vec<u32>,
    assignment: Assignment,
    watches: Vec<WatchList>,
    undo_watches: Vec<Vec<*mut Constraint>>,
    constraints: Vec<*mut Constraint>,
    learnts: Vec<*mut Constraint>,
    heuristic: Box<dyn DecisionHeuristic>,
    strategies: SolverStrategies,
    stats: SolverStats,
    minimized: Vec<Literal>,
    cc_minimize_result: bool,
}

impl core::fmt::Debug for Solver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Solver")
            .field("id", &self.id)
            .field("num_vars", &self.num_vars)
            .field("num_problem_vars", &self.num_problem_vars)
            .field("has_conflict", &self.has_conflict)
            .field("decision_level", &self.decision_level)
            .field("root_level", &self.root_level)
            .finish_non_exhaustive()
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    pub fn new() -> Self {
        let mut assignment = Assignment::default();
        let sentinel = assignment.add_var();
        assignment.set_raw_assignment(sentinel, value_true, 0);
        assignment.set_seen_var(sentinel);
        Self {
            shared: None,
            id: 0,
            num_vars: 0,
            num_problem_vars: 0,
            has_conflict: false,
            conflict_literal: Literal::default(),
            conflict_reason: Antecedent::new(),
            conflict_data: u32::MAX,
            decision_level: 0,
            root_level: 0,
            backtrack_level: 0,
            tag_literal: Literal::default(),
            decisions: vec![Literal::default()],
            level_starts: vec![0],
            root_levels: vec![0],
            assignment,
            watches: vec![WatchList::default(), WatchList::default()],
            undo_watches: vec![Vec::new()],
            constraints: Vec::new(),
            learnts: Vec::new(),
            heuristic: Box::<SelectFirst>::default(),
            strategies: SolverStrategies::default(),
            stats: SolverStats::default(),
            minimized: Vec::new(),
            cc_minimize_result: true,
        }
    }

    pub(crate) fn set_shared_context(&mut self, shared: *mut SharedContext) {
        self.shared = NonNull::new(shared);
    }

    pub fn shared_context(&self) -> Option<&SharedContext> {
        self.shared.map(|ptr| unsafe { ptr.as_ref() })
    }

    fn shared_context_mut(&mut self) -> Option<&mut SharedContext> {
        self.shared.map(|mut ptr| unsafe { ptr.as_mut() })
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn set_id(&mut self, id: u32) {
        self.id = id;
    }

    pub fn num_vars(&self) -> u32 {
        self.num_vars
    }

    pub fn set_num_vars(&mut self, num_vars: u32) {
        self.num_vars = num_vars;
        self.assignment.ensure_var(num_vars);
        self.reserve_watch_capacity(((num_vars + 1) << 1) as usize);
    }

    pub fn num_problem_vars(&self) -> u32 {
        self.num_problem_vars
    }

    pub fn set_num_problem_vars(&mut self, num_problem_vars: u32) {
        self.num_problem_vars = num_problem_vars;
        self.assignment.ensure_var(num_problem_vars);
    }

    pub fn valid_var(&self, var: u32) -> bool {
        var != 0 && var <= self.num_vars.max(self.num_problem_vars)
    }

    pub fn value(&self, var: u32) -> ValT {
        if self.valid_var(var) || var == 0 {
            self.assignment.value(var)
        } else {
            value_free
        }
    }

    pub fn set_value(&mut self, var: u32, value: ValT, level: u32) {
        self.assignment.ensure_var(var);
        if value == value_free {
            self.assignment.clear(var);
            self.assignment.set_reason(var, Antecedent::new());
            self.assignment.set_data(var, u32::MAX);
        } else {
            self.assignment.set_raw_assignment(var, value, level);
        }
    }

    pub fn level(&self, var: u32) -> u32 {
        if self.valid_var(var) || var == 0 {
            self.assignment.level(var)
        } else {
            u32::MAX
        }
    }

    pub fn seen_var(&self, var: u32) -> bool {
        (self.valid_var(var) || var == 0) && self.assignment.seen_var(var)
    }

    pub fn seen_literal(&self, literal: Literal) -> bool {
        (self.valid_var(literal.var()) || literal.var() == 0)
            && self.assignment.seen_literal(literal)
    }

    pub fn mark_seen_var(&mut self, var: u32) {
        self.assignment.ensure_var(var);
        self.assignment.set_seen_var(var);
    }

    pub fn mark_seen_literal(&mut self, literal: Literal) {
        self.assignment.ensure_var(literal.var());
        self.assignment.set_seen_literal(literal);
    }

    pub fn clear_seen_var(&mut self, var: u32) {
        self.assignment.ensure_var(var);
        self.assignment.clear_seen(var);
    }

    pub fn has_conflict(&self) -> bool {
        self.has_conflict
    }

    pub fn set_has_conflict(&mut self, has_conflict: bool) {
        self.has_conflict = has_conflict;
        if !has_conflict {
            self.conflict_literal = Literal::default();
            self.conflict_reason = Antecedent::new();
            self.conflict_data = u32::MAX;
        }
    }

    pub fn conflict_literal(&self) -> Literal {
        self.conflict_literal
    }

    pub fn conflict_reason(&self) -> Antecedent {
        self.conflict_reason
    }

    pub fn conflict_data(&self) -> u32 {
        self.conflict_data
    }

    pub fn clear_conflict(&mut self) {
        self.set_has_conflict(false);
    }

    pub fn decision_level(&self) -> u32 {
        self.decision_level
    }

    pub fn set_decision_level(&mut self, decision_level: u32) {
        self.decision_level = decision_level;
        if self.decisions.len() <= decision_level as usize {
            self.decisions
                .resize(decision_level as usize + 1, Literal::default());
        }
        if self.level_starts.len() <= decision_level as usize {
            self.level_starts.resize(decision_level as usize + 1, 0);
        }
        if self.root_levels.len() <= decision_level as usize {
            self.root_levels.resize(decision_level as usize + 1, 0);
        }
        if self.undo_watches.len() <= decision_level as usize {
            self.undo_watches
                .resize_with(decision_level as usize + 1, Vec::new);
        }
    }

    pub fn root_level(&self) -> u32 {
        self.root_level
    }

    pub fn set_root_level(&mut self, root_level: u32) {
        self.root_level = root_level;
    }

    pub fn push_root_level(&mut self) {
        self.root_levels.push(self.root_level);
        self.root_level = self.decision_level;
    }

    pub fn pop_root_level(&mut self) {
        self.root_level = self.root_levels.pop().unwrap_or(0);
    }

    pub fn backtrack_level(&self) -> u32 {
        self.backtrack_level
    }

    pub fn set_backtrack_level(&mut self, backtrack_level: u32) {
        self.backtrack_level = backtrack_level.min(self.decision_level);
    }

    pub fn tag_literal(&self) -> Literal {
        self.tag_literal
    }

    pub fn set_tag_literal(&mut self, literal: Literal) {
        self.tag_literal = literal;
    }

    pub fn aux_var(&self, var: u32) -> bool {
        var > self.num_problem_vars
    }

    pub fn acquire_problem_var(&mut self, var: u32) {
        if var == 0 {
            return;
        }
        if self.num_vars < var {
            self.set_num_vars(var);
        }
        if self.num_problem_vars < var {
            self.set_num_problem_vars(var);
        }
    }

    pub fn decision(&self, level: u32) -> Literal {
        self.decisions[level as usize]
    }

    pub fn set_decision(&mut self, level: u32, literal: Literal) {
        if self.decisions.len() <= level as usize {
            self.decisions
                .resize(level as usize + 1, Literal::default());
        }
        self.decisions[level as usize] = literal;
    }

    pub fn num_assigned_vars(&self) -> u32 {
        self.assignment.assigned()
    }

    pub fn push_trail_literal(&mut self, literal: Literal) {
        self.assignment.push_trail_literal(literal);
    }

    pub fn clear_trail(&mut self) {
        self.assignment.clear_trail();
    }

    pub fn trail_lit(&self, index: u32) -> Literal {
        self.assignment.trail[index as usize]
    }

    pub fn level_start(&self, level: u32) -> u32 {
        self.level_starts[level as usize]
    }

    pub fn set_level_start(&mut self, level: u32, start: u32) {
        if self.level_starts.len() <= level as usize {
            self.level_starts.resize(level as usize + 1, 0);
        }
        self.level_starts[level as usize] = start;
    }

    pub fn queue_size(&self) -> u32 {
        self.assignment.q_size()
    }

    pub fn q_empty(&self) -> bool {
        self.assignment.q_empty()
    }

    pub fn stats(&self) -> &SolverStats {
        &self.stats
    }

    pub fn stats_mut(&mut self) -> &mut SolverStats {
        &mut self.stats
    }

    pub fn strategies(&self) -> &SolverStrategies {
        &self.strategies
    }

    pub fn strategies_mut(&mut self) -> &mut SolverStrategies {
        &mut self.strategies
    }

    pub fn watch_init_mode(&self) -> WatchInit {
        WatchInit::from_underlying(self.strategies.init_watches as u8)
            .unwrap_or(WatchInit::WatchRand)
    }

    pub fn heuristic(&self) -> &dyn DecisionHeuristic {
        &*self.heuristic
    }

    pub fn heuristic_mut(&mut self) -> &mut dyn DecisionHeuristic {
        &mut *self.heuristic
    }

    pub fn set_heuristic<H>(&mut self, heuristic: H)
    where
        H: DecisionHeuristic + 'static,
    {
        self.heuristic = Box::new(heuristic);
    }

    pub fn reason(&self, var: u32) -> &Antecedent {
        self.assignment.reason(var)
    }

    pub fn reason_data(&self, var: u32) -> u32 {
        self.assignment.data(var)
    }

    pub fn set_reason(&mut self, var: u32, antecedent: Antecedent) {
        self.assignment.ensure_var(var);
        self.assignment.set_reason(var, antecedent);
    }

    pub fn set_reason_data(&mut self, var: u32, data: u32) {
        self.assignment.ensure_var(var);
        self.assignment.set_data(var, data);
    }

    pub fn reserve_watch_capacity(&mut self, size: usize) {
        if self.watches.len() < size {
            self.watches.resize_with(size, WatchList::default);
        }
    }

    pub fn add_watch(&mut self, literal: Literal, constraint: *mut Constraint, data: u32) {
        self.reserve_watch_capacity(literal.id() as usize + 1);
        self.watches[literal.id() as usize].push_right(GenericWatch::new(constraint, data));
    }

    pub fn remove_watch(&mut self, literal: Literal, constraint: *mut Constraint) -> bool {
        let Some(list) = self.watches.get_mut(literal.id() as usize) else {
            return false;
        };
        for index in 0..list.right_size() {
            if list.right(index).con == constraint {
                if list.right_size() == 1 {
                    list.pop_right();
                } else {
                    list.erase_right_unordered(index);
                }
                return true;
            }
        }
        false
    }

    pub fn has_watch(&self, literal: Literal, constraint: *mut Constraint) -> bool {
        self.watches.get(literal.id() as usize).is_some_and(|list| {
            (0..list.right_size()).any(|index| list.right(index).con == constraint)
        })
    }

    pub fn num_watches(&self, literal: Literal) -> u32 {
        self.watches
            .get(literal.id() as usize)
            .map(|list| list.right_size() as u32)
            .unwrap_or(0)
    }

    pub fn get_watch(&self, literal: Literal, index: usize) -> Option<GenericWatch> {
        self.watches
            .get(literal.id() as usize)
            .and_then(|list| (index < list.right_size()).then(|| *list.right(index)))
    }

    pub fn add_clause_watch(&mut self, literal: Literal, head: *mut ClauseHead) {
        self.reserve_watch_capacity(literal.id() as usize + 1);
        self.watches[literal.id() as usize].push_left(ClauseWatch::new(head));
    }

    pub fn remove_clause_watch(&mut self, literal: Literal, head: *mut ClauseHead) -> bool {
        let Some(list) = self.watches.get_mut(literal.id() as usize) else {
            return false;
        };
        for index in 0..list.left_size() {
            if list.left(index).head == head {
                if list.left_size() == 1 {
                    list.pop_left();
                } else {
                    list.erase_left_unordered(index);
                }
                return true;
            }
        }
        false
    }

    pub fn has_clause_watch(&self, literal: Literal, head: *mut ClauseHead) -> bool {
        self.watches
            .get(literal.id() as usize)
            .is_some_and(|list| (0..list.left_size()).any(|index| list.left(index).head == head))
    }

    pub fn num_clause_watches(&self, literal: Literal) -> u32 {
        self.watches
            .get(literal.id() as usize)
            .map(|list| list.left_size() as u32)
            .unwrap_or(0)
    }

    pub fn get_clause_watch(&self, literal: Literal, index: usize) -> Option<ClauseWatch> {
        self.watches
            .get(literal.id() as usize)
            .and_then(|list| (index < list.left_size()).then(|| *list.left(index)))
    }

    pub fn add_undo_watch(&mut self, level: u32, constraint: *mut Constraint) {
        if self.undo_watches.len() <= level as usize {
            self.undo_watches.resize_with(level as usize + 1, Vec::new);
        }
        self.undo_watches[level as usize].push(constraint);
    }

    pub fn remove_undo_watch(&mut self, level: u32, constraint: *mut Constraint) -> bool {
        let Some(list) = self.undo_watches.get_mut(level as usize) else {
            return false;
        };
        if let Some(index) = list.iter().position(|&candidate| candidate == constraint) {
            list.swap_remove(index);
            return true;
        }
        false
    }

    pub fn force(&mut self, literal: Literal, antecedent: Antecedent) -> bool {
        self.force_with_data(literal, antecedent, u32::MAX)
    }

    pub fn force_with_data(&mut self, literal: Literal, antecedent: Antecedent, data: u32) -> bool {
        if self.has_conflict && self.is_true(literal) {
            return true;
        }
        self.assignment.ensure_var(literal.var());
        let assigned = if data == u32::MAX {
            self.assignment
                .assign(literal, self.decision_level, antecedent)
        } else {
            self.assignment
                .assign_with_data(literal, self.decision_level, antecedent, data)
        };
        if assigned {
            true
        } else {
            self.set_conflict(literal, antecedent, data);
            false
        }
    }

    pub fn assume(&mut self, literal: Literal) -> bool {
        if self.is_false(literal) {
            self.set_conflict(literal, Antecedent::new(), u32::MAX);
            return false;
        }
        if self.is_true(literal) {
            return true;
        }
        let next_level = self.decision_level + 1;
        self.set_decision_level(next_level);
        self.set_level_start(next_level, self.num_assigned_vars());
        self.set_decision(next_level, literal);
        self.force(literal, Antecedent::new())
    }

    pub fn is_true(&self, literal: Literal) -> bool {
        self.value(literal.var()) == crate::clasp::literal::true_value(literal)
    }

    pub fn is_false(&self, literal: Literal) -> bool {
        self.value(literal.var()) == crate::clasp::literal::false_value(literal)
    }

    pub fn propagate(&mut self) -> bool {
        while !self.assignment.q_empty() && !self.has_conflict {
            let literal = self.assignment.q_pop();
            if let Some(shared) = self.shared {
                let ok = unsafe { shared.as_ref().implication_graph().propagate(self, literal) };
                if !ok {
                    return false;
                }
            }
            if !self.propagate_watches(literal) {
                return false;
            }
        }
        !self.has_conflict
    }

    pub fn undo_until(&mut self, level: u32) {
        while self.decision_level > level {
            let current = self.decision_level;
            if current < self.undo_watches.len() as u32 {
                let mut undo = core::mem::take(&mut self.undo_watches[current as usize]);
                for constraint in undo.drain(..) {
                    if let Some(constraint) = unsafe { constraint.as_mut() } {
                        constraint.undo_level(self);
                    }
                }
            }
            let first = self.level_start(current) as usize;
            if self.num_assigned_vars() as usize > first {
                self.assignment.undo_trail(first, false);
            }
            self.decisions[current as usize] = Literal::default();
            self.decision_level -= 1;
        }
        self.assignment.q_reset();
        self.has_conflict = false;
    }

    pub fn backtrack(&mut self, level: u32) -> bool {
        let target = level.max(self.root_level);
        self.undo_until(target);
        self.backtrack_level = self.backtrack_level.min(self.decision_level);
        true
    }

    pub fn clear_assumptions(&mut self) -> bool {
        self.backtrack(0);
        self.root_level = 0;
        self.backtrack_level = 0;
        true
    }

    pub fn count_levels(&self, lits: &[Literal]) -> u32 {
        let mut levels = Vec::new();
        for &lit in lits {
            let level = self.level(lit.var());
            if level != u32::MAX && !levels.contains(&level) {
                levels.push(level);
            }
        }
        levels.len() as u32
    }

    pub fn add_constraint(&mut self, constraint: *mut Constraint) {
        self.constraints.push(constraint);
    }

    pub fn add_learnt_constraint(
        &mut self,
        constraint: *mut Constraint,
        size: u32,
        kind: ConstraintType,
    ) {
        self.learnts.push(constraint);
        self.stats.add_learnt(size, kind);
    }

    pub fn remove_constraint(&mut self, constraint: *mut Constraint) {
        if let Some(index) = self.constraints.iter().position(|&ptr| ptr == constraint) {
            self.constraints.swap_remove(index);
        }
        if let Some(index) = self.learnts.iter().position(|&ptr| ptr == constraint) {
            self.learnts.swap_remove(index);
        }
    }

    pub fn add(&mut self, clause: &crate::clasp::clause::ClauseRep, is_new: bool) -> bool {
        if clause.size > 1 {
            if !self.allow_implicit(clause) {
                return false;
            }
            let added = if let Some(shared) = self.shared_context_mut() {
                shared.add_imp(clause.literals(), clause.info.constraint_type())
            } else {
                -1
            };
            if added <= 0 {
                return added == 0;
            }
            if is_new && clause.info.learnt() {
                self.learnts.push(core::ptr::null_mut());
            }
            return true;
        }
        let unit = clause
            .literals()
            .first()
            .copied()
            .unwrap_or(crate::clasp::literal::lit_false);
        self.force(
            unit,
            Antecedent::from_literal(crate::clasp::literal::lit_true),
        )
    }

    pub fn allow_implicit(&self, clause: &crate::clasp::clause::ClauseRep) -> bool {
        if !clause.is_imp() {
            return clause.size <= 1;
        }
        let Some(shared) = self.shared_context() else {
            return false;
        };
        if !shared.allow_implicit(clause.info.constraint_type()) || clause.info.aux() {
            return false;
        }
        clause.prep
            || clause
                .literals()
                .iter()
                .take(clause.size.min(3) as usize)
                .all(|literal| !self.aux_var(literal.var()))
    }

    pub fn num_constraints(&self) -> u32 {
        self.constraints.len() as u32
    }

    pub fn num_learnt_constraints(&self) -> u32 {
        self.learnts.len() as u32
    }

    pub fn cc_minimize(&mut self, lit: Literal, _rec: *mut CCMinRecursive) -> bool {
        self.minimized.push(lit);
        self.cc_minimize_result
    }

    pub fn minimized_literals(&self) -> &[Literal] {
        &self.minimized
    }

    pub fn clear_minimized_literals(&mut self) {
        self.minimized.clear();
    }

    pub fn set_cc_minimize_result(&mut self, result: bool) {
        self.cc_minimize_result = result;
    }

    fn set_conflict(&mut self, literal: Literal, antecedent: Antecedent, data: u32) {
        self.has_conflict = true;
        self.conflict_literal = literal;
        self.conflict_reason = antecedent;
        self.conflict_data = data;
    }

    fn propagate_watches(&mut self, literal: Literal) -> bool {
        let left_len = self
            .watches
            .get(literal.id() as usize)
            .map(|list| list.left_size())
            .unwrap_or(0);
        let mut left_index = 0;
        while left_index < left_len.min(self.num_clause_watches(literal) as usize) {
            let watch = match self.get_clause_watch(literal, left_index) {
                Some(watch) => watch,
                None => break,
            };
            let result = unsafe {
                watch
                    .head
                    .as_mut()
                    .expect("clause watch requires a non-null head pointer")
                    .propagate(self, literal)
            };
            let current_len = self.num_clause_watches(literal) as usize;
            if left_index >= current_len {
                break;
            }
            if result.keep_watch {
                left_index += 1;
            } else if let Some(list) = self.watches.get_mut(literal.id() as usize) {
                if left_index < list.left_size() {
                    if list.left_size() == 1 {
                        list.pop_left();
                    } else {
                        list.erase_left_unordered(left_index);
                    }
                }
            }
            if !result.ok {
                return false;
            }
        }

        let list_len = self
            .watches
            .get(literal.id() as usize)
            .map(|list| list.right_size())
            .unwrap_or(0);
        let mut index = 0;
        while index < list_len.min(self.num_watches(literal) as usize) {
            let mut watch = match self.get_watch(literal, index) {
                Some(watch) => watch,
                None => break,
            };
            let result = watch.propagate(self, literal);
            let current_len = self.num_watches(literal) as usize;
            if index >= current_len {
                break;
            }
            if result.keep_watch {
                if let Some(list) = self.watches.get_mut(literal.id() as usize) {
                    if index < list.right_size() {
                        *list.right_mut(index) = watch;
                    }
                }
                index += 1;
            } else if let Some(list) = self.watches.get_mut(literal.id() as usize) {
                if index < list.right_size() {
                    if list.right_size() == 1 {
                        list.pop_right();
                    } else {
                        list.erase_right_unordered(index);
                    }
                }
            }
            if !result.ok {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropResult {
    pub ok: bool,
    pub keep_watch: bool,
}

impl PropResult {
    pub const fn new(ok: bool, keep_watch: bool) -> Self {
        Self { ok, keep_watch }
    }
}

impl Default for PropResult {
    fn default() -> Self {
        Self::new(true, true)
    }
}

pub trait ConstraintDyn {
    fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult;
    fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec);
    fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>>;

    fn undo_level(&mut self, _s: &mut Solver) {}

    fn simplify(&mut self, _s: &mut Solver, _reinit: bool) -> bool {
        false
    }

    fn destroy(&mut self, _s: Option<&mut Solver>, _detach: bool) {}

    fn valid(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn minimize(&mut self, s: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        let mut temp = LitVec::default();
        self.reason(s, p, &mut temp);
        for lit in temp.as_slice() {
            if !s.cc_minimize(*lit, rec) {
                return false;
            }
        }
        true
    }

    fn estimate_complexity(&self, _s: &Solver) -> u32 {
        1
    }

    fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        None
    }

    fn constraint_type(&self) -> ConstraintType {
        ConstraintType::Static
    }

    fn locked(&self, _s: &Solver) -> bool {
        true
    }

    fn activity(&self) -> ConstraintScore {
        ConstraintScore::default()
    }

    fn decrease_activity(&mut self) {}

    fn reset_activity(&mut self) {}

    fn is_open(&mut self, _s: &Solver, _types: &TypeSet, _free_lits: &mut LitVec) -> u32 {
        0
    }
}

pub struct Constraint {
    inner: Box<dyn ConstraintDyn>,
}

impl core::fmt::Debug for Constraint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Constraint").finish_non_exhaustive()
    }
}

impl Constraint {
    pub fn new<T>(inner: T) -> Self
    where
        T: ConstraintDyn + 'static,
    {
        Self {
            inner: Box::new(inner),
        }
    }

    pub fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult {
        self.inner.propagate(s, p, data)
    }

    pub fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec) {
        self.inner.reason(s, p, lits);
    }

    pub fn clone_attach(&self, other: &mut Solver) -> Option<Box<Constraint>> {
        self.inner.clone_attach(other)
    }

    pub fn undo_level(&mut self, s: &mut Solver) {
        self.inner.undo_level(s);
    }

    pub fn simplify(&mut self, s: &mut Solver, reinit: bool) -> bool {
        self.inner.simplify(s, reinit)
    }

    #[allow(clippy::boxed_local)]
    pub fn destroy(mut self: Box<Self>, s: Option<&mut Solver>, detach: bool) {
        self.inner.destroy(s, detach);
    }

    /// # Safety
    ///
    /// `ptr` must either be null or have been created by `Box::into_raw(Box<Constraint>)`
    /// and not have been previously passed to `destroy_raw` or reconstructed by `Box::from_raw`.
    pub unsafe fn destroy_raw(ptr: *mut Self, s: Option<&mut Solver>, detach: bool) {
        if !ptr.is_null() {
            unsafe { Box::from_raw(ptr) }.destroy(s, detach);
        }
    }

    pub fn valid(&mut self, s: &mut Solver) -> bool {
        self.inner.valid(s)
    }

    pub fn minimize(&mut self, s: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        self.inner.minimize(s, p, rec)
    }

    pub fn estimate_complexity(&self, s: &Solver) -> u32 {
        self.inner.estimate_complexity(s)
    }

    pub fn clause(&mut self) -> Option<NonNull<ClauseHead>> {
        self.inner.clause()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        self.inner.constraint_type()
    }

    pub fn locked(&self, s: &Solver) -> bool {
        self.inner.locked(s)
    }

    pub fn activity(&self) -> ConstraintScore {
        self.inner.activity()
    }

    pub fn decrease_activity(&mut self) {
        self.inner.decrease_activity();
    }

    pub fn reset_activity(&mut self) {
        self.inner.reset_activity();
    }

    pub fn is_open(&mut self, s: &Solver, types: &TypeSet, free_lits: &mut LitVec) -> u32 {
        self.inner.is_open(s, types, free_lits)
    }
}

#[allow(non_upper_case_globals)]
pub const priority_class_simple: u32 = 0;
#[allow(non_upper_case_globals)]
pub const priority_reserved_msg: u32 = 0;
#[allow(non_upper_case_globals)]
pub const priority_reserved_ufs: u32 = 10;
#[allow(non_upper_case_globals)]
pub const priority_reserved_look: u32 = 1023;
#[allow(non_upper_case_globals)]
pub const priority_class_general: u32 = 1024;

pub trait PostPropagatorDyn {
    fn priority(&self) -> u32;
    fn propagate_fixpoint(&mut self, s: &mut Solver, ctx: Option<NonNull<PostPropagator>>) -> bool;

    fn init(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn simplify(&mut self, _s: &mut Solver, _reinit: bool) -> bool {
        false
    }

    fn valid(&mut self, _s: &mut Solver) -> bool {
        true
    }

    fn reset(&mut self) {}

    fn is_model(&mut self, s: &mut Solver) -> bool {
        self.valid(s)
    }

    fn reason(&mut self, _s: &mut Solver, _p: Literal, _lits: &mut LitVec) {}

    fn propagate(&mut self, _s: &mut Solver, _p: Literal, _data: &mut u32) -> PropResult {
        PropResult::new(true, false)
    }

    fn destroy(&mut self, _s: Option<&mut Solver>, _detach: bool) {}
}

pub trait MessageHandlerDyn {
    fn handle_messages(&mut self) -> bool;
}

struct MessageHandlerAdapter<T> {
    inner: T,
}

impl<T> PostPropagatorDyn for MessageHandlerAdapter<T>
where
    T: MessageHandlerDyn,
{
    fn priority(&self) -> u32 {
        priority_reserved_msg
    }

    fn propagate_fixpoint(
        &mut self,
        _s: &mut Solver,
        _ctx: Option<NonNull<PostPropagator>>,
    ) -> bool {
        self.inner.handle_messages()
    }
}

pub struct PostPropagator {
    inner: Box<dyn PostPropagatorDyn>,
    pub next: Option<NonNull<PostPropagator>>,
}

impl core::fmt::Debug for PostPropagator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PostPropagator")
            .field("priority", &self.priority())
            .finish_non_exhaustive()
    }
}

impl PostPropagator {
    pub fn new<T>(inner: T) -> Self
    where
        T: PostPropagatorDyn + 'static,
    {
        Self {
            inner: Box::new(inner),
            next: None,
        }
    }

    pub fn from_message_handler<T>(inner: T) -> Self
    where
        T: MessageHandlerDyn + 'static,
    {
        Self::new(MessageHandlerAdapter { inner })
    }

    pub fn priority(&self) -> u32 {
        self.inner.priority()
    }

    pub fn init(&mut self, s: &mut Solver) -> bool {
        self.inner.init(s)
    }

    pub fn propagate_fixpoint(
        &mut self,
        s: &mut Solver,
        ctx: Option<NonNull<PostPropagator>>,
    ) -> bool {
        self.inner.propagate_fixpoint(s, ctx)
    }

    pub fn simplify(&mut self, s: &mut Solver, reinit: bool) -> bool {
        self.inner.simplify(s, reinit)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn is_model(&mut self, s: &mut Solver) -> bool {
        self.inner.is_model(s)
    }

    pub fn reason(&mut self, s: &mut Solver, p: Literal, lits: &mut LitVec) {
        self.inner.reason(s, p, lits);
    }

    pub fn propagate(&mut self, s: &mut Solver, p: Literal, data: &mut u32) -> PropResult {
        self.inner.propagate(s, p, data)
    }

    pub fn cancel_propagation(&mut self) {
        let mut current = self.next;
        while let Some(mut ptr) = current {
            unsafe {
                ptr.as_mut().reset();
                current = ptr.as_ref().next;
            }
        }
    }

    #[allow(clippy::boxed_local)]
    pub fn destroy(mut self: Box<Self>, s: Option<&mut Solver>, detach: bool) {
        self.inner.destroy(s, detach);
    }

    /// # Safety
    ///
    /// `ptr` must either be null or have been created by `Box::into_raw(Box<PostPropagator>)`
    /// and not have been previously passed to `destroy_raw` or reconstructed by `Box::from_raw`.
    pub unsafe fn destroy_raw(ptr: *mut Self, s: Option<&mut Solver>, detach: bool) {
        if !ptr.is_null() {
            unsafe { Box::from_raw(ptr) }.destroy(s, detach);
        }
    }
}

#[derive(Debug, Default)]
pub struct PropagatorList {
    head: Option<NonNull<PostPropagator>>,
}

impl PropagatorList {
    pub fn new() -> Self {
        Self { head: None }
    }

    pub fn head(&self) -> Option<NonNull<PostPropagator>> {
        self.head
    }

    pub fn add(&mut self, propagator: Box<PostPropagator>) {
        assert!(propagator.next.is_none(), "Invalid post propagator");

        let priority = propagator.priority();
        let leaked = Box::leak(propagator);
        let new_ptr = NonNull::from(&mut *leaked);

        unsafe {
            let mut link = &mut self.head;
            while let Some(mut current) = *link {
                if priority < current.as_ref().priority() {
                    break;
                }
                link = &mut current.as_mut().next;
            }
            leaked.next = *link;
            *link = Some(new_ptr);
        }
    }

    pub fn remove(&mut self, target: NonNull<PostPropagator>) -> Option<Box<PostPropagator>> {
        unsafe {
            let mut link = &mut self.head;
            while let Some(mut current) = *link {
                if current == target {
                    *link = current.as_ref().next;
                    current.as_mut().next = None;
                    return Some(Box::from_raw(current.as_ptr()));
                }
                link = &mut current.as_mut().next;
            }
        }
        None
    }

    pub fn clear(&mut self) {
        let mut current = self.head.take();
        while let Some(ptr) = current {
            unsafe {
                current = ptr.as_ref().next;
                PostPropagator::destroy_raw(ptr.as_ptr(), None, false);
            }
        }
    }

    pub fn find_by<P>(&self, mut pred: P, prio: Option<u32>) -> Option<NonNull<PostPropagator>>
    where
        P: FnMut(&PostPropagator) -> bool,
    {
        let mut current = self.head;
        while let Some(ptr) = current {
            let propagator = unsafe { ptr.as_ref() };
            if let Some(target_prio) = prio {
                match propagator.priority().cmp(&target_prio) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        if pred(propagator) {
                            return Some(ptr);
                        }
                    }
                    Ordering::Greater => break,
                }
            } else if pred(propagator) {
                return Some(ptr);
            }
            current = propagator.next;
        }
        None
    }

    pub fn find(&self, prio: u32) -> Option<NonNull<PostPropagator>> {
        self.find_by(|_| true, Some(prio))
    }

    pub fn init(&mut self, solver: &mut Solver) -> bool {
        let mut current = self.head;
        while let Some(mut ptr) = current {
            unsafe {
                if !ptr.as_mut().init(solver) {
                    return false;
                }
                current = ptr.as_ref().next;
            }
        }
        true
    }

    pub fn simplify(&mut self, solver: &mut Solver, reinit: bool) -> bool {
        unsafe {
            let mut link = &mut self.head;
            while let Some(mut ptr) = *link {
                if ptr.as_mut().simplify(solver, reinit) {
                    *link = ptr.as_ref().next;
                    PostPropagator::destroy_raw(ptr.as_ptr(), Some(solver), false);
                } else {
                    link = &mut ptr.as_mut().next;
                }
            }
        }
        false
    }

    pub fn is_model(&mut self, solver: &mut Solver) -> bool {
        let mut current = self.head;
        while let Some(mut ptr) = current {
            unsafe {
                if !ptr.as_mut().is_model(solver) {
                    return false;
                }
                current = ptr.as_ref().next;
            }
        }
        true
    }
}

impl Drop for PropagatorList {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Antecedent {
    data: u64,
}

impl Antecedent {
    pub const GENERIC: u64 = 0;
    pub const TERNARY: u64 = 1;
    pub const BINARY: u64 = 2;

    pub const fn new() -> Self {
        Self { data: 0 }
    }

    pub const fn from_literal(p: Literal) -> Self {
        Self {
            data: ((p.id() as u64) << 33) + Self::BINARY,
        }
    }

    pub const fn from_literals(p: Literal, q: Literal) -> Self {
        Self {
            data: ((p.id() as u64) << 33) + ((q.id() as u64) << 2) + Self::TERNARY,
        }
    }

    pub fn from_constraint_ptr(con: *mut Constraint) -> Self {
        Self {
            data: con as usize as u64,
        }
    }

    pub const fn is_null(&self) -> bool {
        self.data == 0
    }

    pub const fn type_(&self) -> u64 {
        self.data & 3
    }

    pub fn learnt(&self) -> bool {
        right_most_bit(self.data) > Self::BINARY
            && self.constraint().constraint_type() != ConstraintType::Static
    }

    pub fn constraint(&self) -> &Constraint {
        assert_eq!(self.type_(), Self::GENERIC);
        unsafe { &*(self.data as usize as *const Constraint) }
    }

    pub fn constraint_mut(&mut self) -> &mut Constraint {
        assert_eq!(self.type_(), Self::GENERIC);
        unsafe { &mut *(self.data as usize as *mut Constraint) }
    }

    pub const fn first_literal(&self) -> Literal {
        assert!(self.type_() != Self::GENERIC);
        Literal::from_id((self.data >> 33) as u32)
    }

    pub const fn second_literal(&self) -> Literal {
        assert!(self.type_() == Self::TERNARY);
        Literal::from_id(((self.data >> 1) as u32) >> 1)
    }

    pub fn reason(&mut self, solver: &mut Solver, p: Literal, lits: &mut LitVec) {
        assert!(!self.is_null());
        match self.type_() {
            Self::GENERIC => self.constraint_mut().reason(solver, p, lits),
            Self::BINARY => lits.push_back(self.first_literal()),
            Self::TERNARY => {
                lits.push_back(self.first_literal());
                lits.push_back(self.second_literal());
            }
            _ => unreachable!(),
        }
    }

    pub fn minimize(&mut self, solver: &mut Solver, p: Literal, rec: *mut CCMinRecursive) -> bool {
        assert!(!self.is_null());
        match self.type_() {
            Self::GENERIC => self.constraint_mut().minimize(solver, p, rec),
            Self::BINARY => solver.cc_minimize(self.first_literal(), rec),
            Self::TERNARY => {
                solver.cc_minimize(self.first_literal(), rec)
                    && solver.cc_minimize(self.second_literal(), rec)
            }
            _ => unreachable!(),
        }
    }

    pub const fn as_u64(&self) -> u64 {
        self.data
    }

    pub fn as_u64_mut(&mut self) -> &mut u64 {
        &mut self.data
    }
}

impl From<Literal> for Antecedent {
    fn from(value: Literal) -> Self {
        Self::from_literal(value)
    }
}

impl From<(Literal, Literal)> for Antecedent {
    fn from(value: (Literal, Literal)) -> Self {
        Self::from_literals(value.0, value.1)
    }
}

impl PartialEq<*const Constraint> for Antecedent {
    fn eq(&self, other: &*const Constraint) -> bool {
        self.data as usize == *other as usize
    }
}

#[allow(non_upper_case_globals)]
pub const lbd_max: u32 = 127;
#[allow(non_upper_case_globals)]
pub const act_max: u32 = (1 << 20) - 1;

#[derive(Clone, Copy, Debug, Default)]
pub struct ConstraintScore {
    rep: u32,
}

#[allow(non_upper_case_globals)]
impl ConstraintScore {
    pub const bits_used: u32 = 28;
    pub const bumped_bit: u32 = 27;
    pub const lbd_shift: u32 = 20;
    pub const lbd_mask: u32 = lbd_max << Self::lbd_shift;
    pub const score_mask: u32 = (1u32 << Self::bits_used) - 1;

    pub const fn new(act: u32, lbd: u32) -> Self {
        Self {
            rep: (if lbd < lbd_max { lbd } else { lbd_max }) << Self::lbd_shift
                | if act < act_max { act } else { act_max },
        }
    }

    const fn from_rep(rep: u32) -> Self {
        Self { rep }
    }

    pub fn reset(&mut self, act: u32, lbd: u32) {
        self.assign(Self::new(act, lbd));
    }

    pub const fn activity(&self) -> u32 {
        self.rep & act_max
    }

    pub fn lbd(&self) -> u32 {
        if self.has_lbd() {
            (self.rep & Self::lbd_mask) >> Self::lbd_shift
        } else {
            lbd_max
        }
    }

    pub fn has_lbd(&self) -> bool {
        test_any(self.rep, Self::lbd_mask)
    }

    pub fn bumped(&self) -> bool {
        test_bit(self.rep, Self::bumped_bit)
    }

    pub fn bump_activity(&mut self) {
        self.rep += u32::from(self.activity() < act_max);
    }

    pub fn bump_lbd(&mut self, x: u32) {
        if x < self.lbd() {
            store_clear_mask(&mut self.rep, Self::lbd_mask);
            store_set_mask(
                &mut self.rep,
                (x << Self::lbd_shift) | nth_bit::<u32>(Self::bumped_bit),
            );
        }
    }

    pub fn clear_bumped(&mut self) {
        store_clear_bit(&mut self.rep, Self::bumped_bit);
    }

    pub fn reduce(&mut self) {
        self.clear_bumped();
        let activity = self.activity();
        if activity != 0 {
            store_clear_mask(&mut self.rep, act_max);
            store_set_mask(&mut self.rep, activity >> 1);
        }
    }

    pub fn assign(&mut self, other: Self) {
        store_clear_mask(&mut self.rep, Self::score_mask);
        store_set_mask(&mut self.rep, other.rep & Self::score_mask);
    }
}

impl PartialEq for ConstraintScore {
    fn eq(&self, other: &Self) -> bool {
        (self.rep & Self::score_mask) == (other.rep & Self::score_mask)
    }
}

impl Eq for ConstraintScore {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConstraintInfo {
    rep: u32,
}

#[allow(non_upper_case_globals)]
impl ConstraintInfo {
    const tag_bit: u32 = 31;
    const aux_bit: u32 = 30;
    const type_shift: u32 = 28;
    const type_mask: u32 = 3 << Self::type_shift;

    pub fn new(constraint_type: ConstraintType) -> Self {
        Self {
            rep: constraint_type.as_u32() << Self::type_shift,
        }
    }

    pub fn activity(&self) -> u32 {
        self.score().activity()
    }

    pub fn lbd(&self) -> u32 {
        self.score().lbd()
    }

    pub fn constraint_type(&self) -> ConstraintType {
        ConstraintType::from_u32((self.rep & Self::type_mask) >> Self::type_shift)
            .expect("invalid ConstraintInfo type bits")
    }

    pub fn tagged(&self) -> bool {
        test_bit(self.rep, Self::tag_bit)
    }

    pub fn aux(&self) -> bool {
        self.tagged() || test_bit(self.rep, Self::aux_bit)
    }

    pub fn learnt(&self) -> bool {
        self.constraint_type() != ConstraintType::Static
    }

    pub fn score(&self) -> ConstraintScore {
        ConstraintScore::from_rep(self.rep & ConstraintScore::score_mask)
    }

    pub fn set_type(&mut self, constraint_type: ConstraintType) -> &mut Self {
        store_clear_mask(&mut self.rep, Self::type_mask);
        store_set_mask(&mut self.rep, constraint_type.as_u32() << Self::type_shift);
        self
    }

    pub fn set_score(&mut self, score: ConstraintScore) -> &mut Self {
        store_clear_mask(&mut self.rep, ConstraintScore::score_mask);
        store_set_mask(&mut self.rep, score.rep & ConstraintScore::score_mask);
        self
    }

    pub fn set_activity(&mut self, activity: u32) -> &mut Self {
        self.set_score(ConstraintScore::new(activity, self.lbd()))
    }

    pub fn set_lbd(&mut self, lbd: u32) -> &mut Self {
        self.set_score(ConstraintScore::new(self.activity(), lbd))
    }

    pub fn set_tagged(&mut self, value: bool) -> &mut Self {
        self.set_bit::<{ Self::tag_bit }>(value)
    }

    pub fn set_aux(&mut self, value: bool) -> &mut Self {
        self.set_bit::<{ Self::aux_bit }>(value)
    }

    fn set_bit<const BIT: u32>(&mut self, value: bool) -> &mut Self {
        if test_bit(self.rep, BIT) != value {
            store_toggle_bit(&mut self.rep, BIT);
        }
        self
    }
}
