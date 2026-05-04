//! Rust port of `original_clasp/clasp/constraint.h` and `original_clasp/src/constraint.cpp`.

use core::cmp::Ordering;
use core::ffi::c_void;
use core::ptr::NonNull;

use crate::clasp::clause::{
    CLAUSE_EXPLICIT, CLAUSE_FORCE_SIMPLIFY, CLAUSE_NO_PREPARE, ClauseCreator, ClauseInfo,
    ClauseRep, SharedLiterals,
};
use crate::clasp::literal::{
    LitVec, Literal, ValT, lit_false, value_false, value_free, value_true,
};
use crate::clasp::pod_vector::{VectorLike, shrink_vec_to};
use crate::clasp::shared_context::SharedContext;
use crate::clasp::solver_strategies::{
    CCMinAntes, CCRepMode, HeuParams, ReduceAlgorithm, ReduceStrategy, SearchLimits,
    SearchStrategy, SolverParams, SolverStrategies, WatchInit,
};
use crate::clasp::solver_types::{
    Assignment, ClauseWatch, GenericWatch, ImpliedList, ImpliedLiteral, SolverStats, ValueSet,
    WatchList,
};
use crate::clasp::util::misc_types::{Rng, ratio};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClauseOwnerKind {
    #[default]
    Unknown,
    Explicit,
    Shared,
}

#[derive(Debug)]
pub struct ClauseHead {
    pub(crate) info: ConstraintInfo,
    pub(crate) head: [Literal; 3],
    pub(crate) constraint: *mut Constraint,
    pub(crate) owner: *mut c_void,
    pub(crate) owner_kind: ClauseOwnerKind,
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
            owner_kind: ClauseOwnerKind::Unknown,
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

    pub fn is_small(&self) -> bool {
        crate::clasp::clause::clause_head_is_small(self)
    }

    pub fn contracted(&self) -> bool {
        crate::clasp::clause::clause_head_contracted(self)
    }

    pub fn strengthened(&self) -> bool {
        crate::clasp::clause::clause_head_strengthened(self)
    }

    pub fn compute_alloc_size(&self) -> u32 {
        crate::clasp::clause::clause_head_compute_alloc_size(self)
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

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CCMinState {
    Open = 0,
    Removable = 1,
    Poison = 2,
}

#[derive(Debug, Default)]
pub struct CCMinRecursive {
    todo: Vec<Literal>,
    open: u32,
}

impl CCMinRecursive {
    fn push(&mut self, literal: Literal) {
        self.todo.push(literal);
    }

    fn pop(&mut self) -> Literal {
        self.todo
            .pop()
            .expect("CCMinRecursive todo stack must not be empty")
    }

    fn clear(&mut self) {
        self.todo.clear();
    }

    fn encode_state(&self, state: CCMinState) -> u32 {
        self.open + state as u32
    }

    fn decode_state(&self, epoch: u32) -> CCMinState {
        if epoch <= self.open {
            CCMinState::Open
        } else {
            match epoch - self.open {
                1 => CCMinState::Removable,
                2 => CCMinState::Poison,
                _ => CCMinState::Open,
            }
        }
    }
}

pub trait DecisionHeuristic {
    fn new_constraint(&mut self, _solver: &Solver, _lits: &[Literal], _ty: ConstraintType) {}

    fn start_init(&mut self, _solver: &mut Solver) {}

    fn end_init(&mut self, _solver: &mut Solver) {}

    fn detach(&mut self, _solver: &mut Solver) {}

    fn set_config(&mut self, _params: HeuParams) {}

    fn update_var(&mut self, _solver: &mut Solver, _var: u32, _num: u32) {}

    fn select_literal(&self, _solver: &Solver, var: u32, _idx: u32) -> Literal {
        Literal::new(var, false)
    }

    fn select(&mut self, solver: &mut Solver) -> bool {
        for var in 1..=solver.num_vars() {
            if solver.value(var) == value_free {
                return solver.assume(self.select_literal(solver, var, 0));
            }
        }
        false
    }
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
    pub(crate) backtrack_mode: u32,
    tag_literal: Literal,
    decisions: Vec<Literal>,
    level_starts: Vec<u32>,
    assignment: Assignment,
    watches: Vec<WatchList>,
    undo_watches: Vec<Vec<*mut Constraint>>,
    constraints: Vec<*mut Constraint>,
    learnts: Vec<*mut Constraint>,
    learnt_bytes: u64,
    post_propagators: PropagatorList,
    post_init_ready: bool,
    posts_active: bool,
    heuristic: Box<dyn DecisionHeuristic>,
    strategies: SolverStrategies,
    stats: SolverStats,
    rng: Rng,
    epochs: Vec<u32>,
    level_marks: Vec<bool>,
    implied_lits: ImpliedList,
    conflict_lits: LitVec,
    cc_lits: LitVec,
    cc_info: ConstraintInfo,
    enum_constraint: Option<*mut Constraint>,
    last_simplify: u32,
    split_requested: bool,
    stop_conflict: Option<StopConflictState>,
    undo_target: Option<u32>,
    /// BundleA: freeze-level and conflict-helper cluster
    level_freeze: Vec<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StopConflictState {
    root_level: u32,
    backtrack_level: u32,
    queue_front: u32,
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
    /// Returns the frozen state of a decision level.
    pub fn frozen_level(&self, level: u32) -> bool {
        self.level_freeze
            .get(level as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Freezes a decision level.
    pub fn freeze_level(&mut self, level: u32) {
        if self.level_freeze.len() <= level as usize {
            self.level_freeze.resize(level as usize + 1, false);
        }
        self.level_freeze[level as usize] = true;
    }

    /// Unfreezes a decision level.
    pub fn unfreeze_level(&mut self, level: u32) {
        if let Some(f) = self.level_freeze.get_mut(level as usize) {
            *f = false;
        }
    }

    /// Returns the input variable for a literal (BundleA: synonym for var())
    pub fn input_var(&self, lit: Literal) -> u32 {
        lit.var()
    }

    /// Fills a vector with the current assignment values for all variables.
    pub fn values(&self, out: &mut Vec<ValT>) {
        out.clear();
        for v in 1..=self.num_vars {
            out.push(self.value(v));
        }
    }

    /// Updates seen/marked state for a reason clause (BundleA: updateOnReason)
    pub fn update_on_reason(&mut self, reason: &[Literal]) {
        for &lit in reason {
            let v = lit.var();
            if !self.seen_var(v) {
                self.mark_seen_var(v);
            }
        }
    }

    /// Updates seen/marked state for a minimize clause (BundleA: updateOnMinimize)
    pub fn update_on_minimize(&mut self, clause: &[Literal]) {
        for &lit in clause {
            let v = lit.var();
            if !self.seen_var(v) {
                self.mark_seen_var(v);
            }
        }
    }

    pub fn resolve_to_flagged(
        &mut self,
        input: &[Literal],
        flags: u8,
        out: &mut LitVec,
        out_lbd: &mut u32,
    ) -> bool {
        let trail = self.assignment.trail.as_slice().to_vec();
        let mut rhs = input.to_vec();
        let mut temp = LitVec::new();
        out.clear();
        let mut ok = true;
        let mut first = true;
        let mut trail_pos = trail.len();
        let mut resolve = 0u32;
        loop {
            for &lit in &rhs {
                let current = if first { !lit } else { lit };
                let var = current.var();
                if !self.seen_var(var) {
                    self.mark_seen_var(var);
                    if self.var_info(var).has_all(flags) {
                        self.mark_level(self.level(var));
                        out.push_back(!current);
                    } else if !self.reason(var).is_null() {
                        resolve += 1;
                    } else {
                        self.clear_seen_var(var);
                        ok = false;
                        break;
                    }
                }
            }
            first = false;
            if resolve == 0 {
                break;
            }
            resolve -= 1;
            loop {
                trail_pos -= 1;
                let lit = trail[trail_pos];
                if self.seen_var(lit.var()) && !self.var_info(lit.var()).has_all(flags) {
                    let antecedent = *self.reason(lit.var());
                    self.clear_seen_var(lit.var());
                    temp.clear();
                    let mut reason = antecedent;
                    reason.reason(self, lit, &mut temp);
                    rhs.clear();
                    rhs.extend_from_slice(temp.as_slice());
                    break;
                }
            }
        }
        let mut out_size = out.len();
        if ok && !first {
            let saved_keep = self.strategies.cc_min_keep_act;
            self.strategies.cc_min_keep_act = 1;
            let mut recursive = CCMinRecursive::default();
            let recursive_ptr = if self.strategies.cc_min_rec != 0 {
                self.prepare_cc_min_recursive(&mut recursive);
                &mut recursive as *mut CCMinRecursive
            } else {
                core::ptr::null_mut()
            };
            let mut index = 0usize;
            while index < out_size {
                if self.cc_removable(!out[index], CCMinAntes::AllAntes as u32, recursive_ptr) {
                    out_size -= 1;
                    out.as_mut_slice().swap(index, out_size);
                } else {
                    index += 1;
                }
            }
            self.strategies.cc_min_keep_act = saved_keep;
        }
        *out_lbd = 0;
        let mut on_root = 0u32;
        for index in 0..out_size {
            let lit = out[index];
            let var = lit.var();
            let level = self.level(var);
            self.clear_seen_var(var);
            if level != 0 && self.has_level(level) {
                self.unmark_level(level);
                *out_lbd += if level > self.root_level() {
                    1
                } else {
                    on_root += 1;
                    u32::from(on_root == 1)
                };
            }
        }
        shrink_vec_to(out, out_size);
        ok
    }

    pub fn resolve_to_core(&mut self, out: &mut LitVec) {
        assert!(self.has_conflict() && !self.has_stop_conflict());
        out.clear();
        let original_conflict = core::mem::take(&mut self.conflict_lits);
        self.cc_lits.clear();
        self.cc_lits.assign_from_slice(original_conflict.as_slice());
        if self.search_mode() == SearchStrategy::NoLearning {
            for level in 1..=self.decision_level() {
                self.cc_lits.push_back(self.decision(level));
            }
        }
        let trail = self.assignment.trail.as_slice().to_vec();
        let mut rhs = self.cc_lits.as_slice().to_vec();
        let mut marked = 0u32;
        let mut trail_pos = trail.len();
        loop {
            for &lit in &rhs {
                if !self.seen_var(lit.var()) {
                    self.mark_seen_var(lit.var());
                    marked += 1;
                }
            }
            if marked == 0 {
                break;
            }
            marked -= 1;
            while !self.seen_var(trail[trail_pos - 1].var()) {
                trail_pos -= 1;
            }
            trail_pos -= 1;
            let lit = trail[trail_pos];
            let level = self.level(lit.var());
            self.clear_seen_var(lit.var());
            rhs.clear();
            let antecedent = *self.reason(lit.var());
            if !antecedent.is_null() {
                let mut reason = antecedent;
                let mut temp = LitVec::new();
                reason.reason(self, lit, &mut temp);
                rhs.extend_from_slice(temp.as_slice());
            } else if lit == self.decision(level) {
                out.push_back(lit);
            }
        }
        self.conflict_lits = original_conflict;
    }
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
            backtrack_mode: 1,
            tag_literal: Literal::default(),
            decisions: vec![Literal::default()],
            level_starts: vec![0],
            assignment,
            watches: vec![WatchList::default(), WatchList::default()],
            undo_watches: vec![Vec::new()],
            constraints: Vec::new(),
            learnts: Vec::new(),
            learnt_bytes: 0,
            post_propagators: PropagatorList::default(),
            post_init_ready: false,
            posts_active: false,
            heuristic: Box::<SelectFirst>::default(),
            strategies: SolverStrategies::default(),
            stats: SolverStats::default(),
            rng: Rng::default(),
            epochs: vec![0],
            level_marks: vec![false],
            implied_lits: ImpliedList::default(),
            conflict_lits: LitVec::new(),
            cc_lits: LitVec::new(),
            cc_info: ConstraintInfo::new(ConstraintType::Conflict),
            enum_constraint: None,
            last_simplify: 0,
            split_requested: false,
            stop_conflict: None,
            undo_target: None,
            level_freeze: vec![false],
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

    pub fn eliminate_var(&mut self, var: u32) {
        self.assignment.ensure_var(var);
        self.assignment.eliminate(var);
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
        if self.undo_watches.len() <= decision_level as usize {
            self.undo_watches
                .resize_with(decision_level as usize + 1, Vec::new);
        }
        if self.level_marks.len() <= decision_level as usize {
            self.level_marks.resize(decision_level as usize + 1, false);
        }
    }

    pub fn root_level(&self) -> u32 {
        self.root_level
    }

    pub fn set_root_level(&mut self, root_level: u32) {
        self.root_level = root_level;
    }

    pub fn push_root_level(&mut self, levels: u32) {
        self.root_level = self
            .decision_level
            .min(self.root_level.saturating_add(levels));
        self.backtrack_level = self.backtrack_level.max(self.root_level);
    }

    pub fn pop_root_level(&mut self, levels: u32) -> bool {
        self.clear_stop_conflict();
        let new_root = self.root_level.saturating_sub(levels.min(self.root_level));
        self.root_level = new_root;
        self.backtrack_level = new_root;
        self.implied_lits.front = 0;
        self.undo_until(new_root);
        !self.has_conflict
    }

    pub fn pop_root_level_with(&mut self, levels: u32, popped: &mut LitVec, aux: bool) -> bool {
        self.clear_stop_conflict();
        let old_root = self.root_level;
        let new_root = old_root.saturating_sub(levels.min(old_root));
        if new_root < old_root {
            for level in new_root + 1..=old_root {
                let decision = self.decision(level);
                if decision != Literal::default() && (aux || !self.aux_var(decision.var())) {
                    popped.push_back(decision);
                }
            }
        }
        self.root_level = new_root;
        self.backtrack_level = new_root;
        self.implied_lits.front = 0;
        self.undo_until(new_root);
        !self.has_conflict
    }

    pub fn backtrack_level(&self) -> u32 {
        self.backtrack_level
    }

    pub fn set_backtrack_level(&mut self, backtrack_level: u32) {
        self.backtrack_level = backtrack_level
            .min(self.decision_level)
            .max(self.root_level);
    }

    pub fn has_level(&self, level: u32) -> bool {
        level != 0
            && level <= self.decision_level
            && self
                .level_marks
                .get(level as usize)
                .copied()
                .unwrap_or(false)
    }

    pub fn mark_level(&mut self, level: u32) {
        assert!(level != 0 && level <= self.decision_level);
        if self.level_marks.len() <= level as usize {
            self.level_marks.resize(level as usize + 1, false);
        }
        self.level_marks[level as usize] = true;
    }

    pub fn unmark_level(&mut self, level: u32) {
        assert!(level != 0 && level <= self.decision_level);
        if let Some(mark) = self.level_marks.get_mut(level as usize) {
            *mark = false;
        }
    }

    pub fn tag_literal(&self) -> Literal {
        self.tag_literal
    }

    pub fn set_tag_literal(&mut self, literal: Literal) {
        self.tag_literal = literal;
    }

    pub fn push_aux_var(&mut self) -> u32 {
        let aux = self.assignment.add_var();
        self.num_vars = self.num_vars.max(aux);
        self.reserve_watch_capacity(((aux + 1) << 1) as usize);
        self.assignment.request_prefs();
        self.assignment
            .set_pref(aux, ValueSet::def_value, value_false);
        let heuristic = self.heuristic.as_mut() as *mut dyn DecisionHeuristic;
        unsafe {
            (*heuristic).update_var(self, aux, 1);
        }
        aux
    }

    pub fn push_tag_var(&mut self, push_to_root: bool) -> u32 {
        if self.tag_literal.var() == 0 {
            self.tag_literal = Literal::new(self.push_aux_var(), false);
        }
        if push_to_root {
            let _ = self.push_root(self.tag_literal);
        }
        self.tag_literal.var()
    }

    pub fn remove_conditional(&mut self) {
        if self.tag_literal.var() == 0 {
            return;
        }
        let old_learnts = core::mem::take(&mut self.learnts);
        let mut kept = Vec::with_capacity(old_learnts.len());
        for constraint_ptr in old_learnts {
            let remove = unsafe {
                constraint_ptr
                    .as_mut()
                    .and_then(|constraint| constraint.clause())
                    .is_some_and(|head| head.as_ref().tagged())
            };
            if remove {
                destroy_constraint_ptr(constraint_ptr, self);
            } else {
                kept.push(constraint_ptr);
            }
        }
        kept.extend(core::mem::take(&mut self.learnts));
        self.learnts = kept;
    }

    pub fn strengthen_conditional(&mut self) {
        if self.tag_literal.var() == 0 {
            return;
        }
        let tag_literal = !self.tag_literal;
        let old_learnts = core::mem::take(&mut self.learnts);
        let mut kept = Vec::with_capacity(old_learnts.len());
        for constraint_ptr in old_learnts {
            let mut remove_clause = false;
            let mut unit_literal = Literal::default();
            unsafe {
                if let Some(constraint) = constraint_ptr.as_mut() {
                    if let Some(mut head) = constraint.clause() {
                        if head.as_ref().tagged() {
                            let remaining: Vec<Literal> = head
                                .as_ref()
                                .to_lits()
                                .into_iter()
                                .filter(|lit| *lit != tag_literal)
                                .collect();
                            remove_clause = head
                                .as_mut()
                                .strengthen(self, tag_literal, true)
                                .remove_clause;
                            if remove_clause && remaining.len() == 1 {
                                unit_literal = remaining[0];
                            }
                            if remove_clause {
                                debug_assert!(
                                    self.decision_level == self.root_level
                                        || !constraint.locked(self),
                                    "Solver::strengthen_conditional(): must not remove locked constraint!"
                                );
                            }
                        }
                    }
                }
            }
            if remove_clause {
                if unit_literal.var() != 0 {
                    let _ = self.force_at_level(
                        unit_literal,
                        0,
                        Antecedent::from_literal(crate::clasp::literal::lit_true),
                    );
                }
                destroy_constraint_ptr(constraint_ptr, self);
            } else {
                kept.push(constraint_ptr);
            }
        }
        kept.extend(core::mem::take(&mut self.learnts));
        self.learnts = kept;
    }

    pub fn begin_init(&mut self) {
        self.post_init_ready = false;
        self.posts_active = false;
    }

    pub fn start_init(&mut self, num_cons_guess: u32, mut params: SolverParams) {
        debug_assert_eq!(self.decision_level, 0);
        let shared_num_vars = self
            .shared_context()
            .map_or(self.num_vars, |shared| shared.num_vars());
        self.begin_init();
        self.last_simplify = self.num_assigned_vars();
        self.assignment.trail.reserve(shared_num_vars as usize + 2);
        self.reserve_watch_capacity(((shared_num_vars + 2) << 1) as usize);
        self.assignment.ensure_var(shared_num_vars + 1);
        self.update_vars(shared_num_vars);
        self.constraints.reserve((num_cons_guess / 2) as usize);
        self.level_starts.reserve(25);
        if self.undo_watches.len() < 25 {
            self.undo_watches.resize_with(25, Vec::new);
        }
        if !self.pop_root_level(self.root_level) {
            return;
        }
        if self.strategies.has_config == 0 {
            let old_heu_id = self.strategies.heu_id;
            let solver_id = self.id;
            let _ = params.prepare();
            self.strategies = SolverStrategies {
                compress: params.compress,
                save_progress: params.save_progress,
                heu_id: params.heu_id,
                reverse_arcs: params.reverse_arcs,
                otfs: params.otfs,
                update_lbd: params.update_lbd,
                cc_min_antes: params.cc_min_antes,
                cc_rep_mode: params.cc_rep_mode,
                cc_min_rec: params.cc_min_rec,
                cc_min_keep_act: params.cc_min_keep_act,
                init_watches: params.init_watches,
                up_mode: params.up_mode,
                bump_var_act: params.bump_var_act,
                search: params.search,
                restart_on_model: params.restart_on_model,
                reset_on_model: params.reset_on_model,
                sign_def: params.sign_def,
                sign_fix: params.sign_fix,
                has_config: 1,
                id: solver_id,
            };
            let seed = if let Some(shared) = self.shared_context() {
                if solver_id == params.id || !shared.seed_solvers() {
                    params.seed
                } else {
                    let mut seeded = Rng::new(14_182_940);
                    for _ in 0..solver_id {
                        let _ = seeded.rand();
                    }
                    seeded.seed()
                }
            } else {
                params.seed
            };
            self.rng.srand(seed);
            if old_heu_id != params.heu_id {
                self.set_default_heuristic();
            }
            let heuristic = self.heuristic.as_mut() as *mut dyn DecisionHeuristic;
            unsafe {
                (*heuristic).set_config(params.heuristic);
            }
        }
        let heuristic = self.heuristic.as_mut() as *mut dyn DecisionHeuristic;
        unsafe {
            (*heuristic).start_init(self);
        }
    }

    pub fn end_init(&mut self) -> bool {
        let heuristic = self.heuristic.as_mut() as *mut dyn DecisionHeuristic;
        unsafe {
            (*heuristic).end_init(self);
        }
        if self.strategies.sign_fix != 0 {
            self.assignment.request_prefs();
            for var in 1..=self.num_vars() {
                let literal = self.heuristic.select_literal(self, var, 0);
                self.assignment.set_pref(
                    var,
                    ValueSet::user_value,
                    if literal.sign() {
                        value_false
                    } else {
                        value_true
                    },
                );
            }
        }
        if !self.post_init_ready {
            let post_list = &mut self.post_propagators as *mut PropagatorList;
            if !unsafe { (*post_list).init(self) } {
                return false;
            }
            self.post_init_ready = true;
        }
        self.posts_active = true;
        self.propagate() && self.simplify()
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

    pub fn update_vars(&mut self, new_num_vars: u32) {
        self.num_vars = self.num_vars.min(new_num_vars);
        self.num_problem_vars = self.num_problem_vars.min(new_num_vars);
        self.assignment
            .truncate_vars(self.num_vars.max(self.num_problem_vars));
        self.watches
            .truncate((((self.num_vars + 1) << 1) as usize).max(2));
        if self.tag_literal.var() > self.num_vars {
            self.tag_literal = Literal::default();
        }
        for decision in &mut self.decisions {
            if decision.var() > self.num_vars {
                *decision = Literal::default();
            }
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

    pub fn assignment(&self) -> &Assignment {
        &self.assignment
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

    pub fn learnt_bytes(&self) -> u64 {
        self.learnt_bytes
    }

    pub fn add_learnt_bytes(&mut self, bytes: u32) {
        self.learnt_bytes = self.learnt_bytes.saturating_add(u64::from(bytes));
    }

    pub fn free_learnt_bytes(&mut self, bytes: u64) {
        self.learnt_bytes = self
            .learnt_bytes
            .saturating_sub(bytes.min(self.learnt_bytes));
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
        let mut old = core::mem::replace(&mut self.heuristic, Box::new(heuristic));
        old.detach(self);
    }

    pub fn set_default_heuristic(&mut self) {
        let mut old = core::mem::replace(&mut self.heuristic, Box::new(SelectFirst));
        old.detach(self);
    }

    pub fn reset_config(&mut self) {
        if self.strategies.has_config != 0 {
            if let Some(look) = self.get_post(priority_reserved_look) {
                if let Some(removed) = self.remove_post(look) {
                    removed.destroy(Some(self), true);
                }
            }
        }
        self.strategies.has_config = 0;
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

    fn set_reason_for_literal(
        &mut self,
        literal: Literal,
        antecedent: Antecedent,
        data: u32,
    ) -> bool {
        self.set_reason(literal.var(), antecedent);
        if data != u32::MAX {
            self.set_reason_data(literal.var(), data);
        }
        true
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
        let mut total = self
            .watches
            .get(literal.id() as usize)
            .map(|list| list.right_size() as u32)
            .unwrap_or(0);
        if !self.aux_var(literal.var()) {
            if let Some(shared) = self.shared_context() {
                total += shared.implication_graph().num_edges(literal);
            }
        }
        total
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

    pub fn force_at_level(&mut self, literal: Literal, level: u32, antecedent: Antecedent) -> bool {
        self.force_at_level_with_data(literal, level, antecedent, u32::MAX)
    }

    pub fn force_at_level_with_data(
        &mut self,
        literal: Literal,
        level: u32,
        antecedent: Antecedent,
        data: u32,
    ) -> bool {
        if level == self.decision_level {
            self.force_with_data(literal, antecedent, data)
        } else {
            self.force_implied(ImpliedLiteral::with_data(literal, level, antecedent, data))
        }
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

    fn force_implied(&mut self, implied: ImpliedLiteral) -> bool {
        if self.is_true(implied.lit) {
            if self.level(implied.lit.var()) <= implied.level {
                return true;
            }
            let mut update_reason = false;
            if let Some(existing) = self.implied_lits.find(implied.lit) {
                if existing.level > implied.level {
                    *existing = implied;
                    update_reason = true;
                }
                if update_reason {
                    self.set_reason_for_literal(
                        implied.lit,
                        implied.ante.ante(),
                        implied.ante.data(),
                    );
                }
                return true;
            }
        }
        if self.undo_until(implied.level) != implied.level {
            self.implied_lits.add(self.decision_level, implied);
        }
        if self.is_true(implied.lit) {
            self.set_reason_for_literal(implied.lit, implied.ante.ante(), implied.ante.data())
        } else {
            self.force_with_data(implied.lit, implied.ante.ante(), implied.ante.data())
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
        if self.unit_propagate() && self.post_propagate(None, None, None) {
            return true;
        }
        self.cancel_propagation();
        false
    }

    pub fn propagate_until(
        &mut self,
        stop: Option<NonNull<PostPropagator>>,
        max_prio: Option<u32>,
    ) -> ValT {
        let mut max_prio = max_prio;
        if self.unit_propagate() && self.post_propagate(None, stop, max_prio.as_mut()) {
            return if max_prio.is_some_and(|prio| prio == u32::MAX) {
                value_free
            } else {
                value_true
            };
        }
        self.cancel_propagation();
        crate::clasp::literal::value_false
    }

    pub fn propagate_from(&mut self, start: NonNull<PostPropagator>) -> bool {
        debug_assert!(
            self.posts_active,
            "post propagators are not active during init"
        );
        debug_assert!(
            self.assignment.q_empty(),
            "propagate_from requires a drained unit queue"
        );
        if self.post_propagate(Some(start), None, None) {
            return true;
        }
        self.cancel_propagation();
        false
    }

    pub fn add_post(&mut self, propagator: Box<PostPropagator>) -> bool {
        let mut post = self.post_propagators.add(propagator);
        if self.posts_active || self.post_init_ready {
            unsafe {
                return post.as_mut().init(self);
            }
        }
        true
    }

    pub fn remove_post(
        &mut self,
        propagator: NonNull<PostPropagator>,
    ) -> Option<Box<PostPropagator>> {
        self.post_propagators.remove(propagator)
    }

    pub fn get_post(&self, priority: u32) -> Option<NonNull<PostPropagator>> {
        self.post_propagators.find(priority)
    }

    pub fn get_post_by<P>(&self, pred: P, max_prio: Option<u32>) -> Option<NonNull<PostPropagator>>
    where
        P: FnMut(&PostPropagator) -> bool,
    {
        self.post_propagators.find_by(pred, max_prio)
    }

    pub fn has_stop_conflict(&self) -> bool {
        self.stop_conflict.is_some()
    }

    pub fn set_stop_conflict(&mut self) {
        if !self.has_conflict {
            self.stop_conflict = Some(StopConflictState {
                root_level: self.root_level,
                backtrack_level: self.backtrack_level,
                queue_front: self.assignment.front,
            });
            self.set_conflict(lit_false, Antecedent::new(), u32::MAX);
        }
        self.push_root_level(self.decision_level);
    }

    pub fn clear_stop_conflict(&mut self) {
        if let Some(saved) = self.stop_conflict.take() {
            self.root_level = saved.root_level;
            self.backtrack_level = saved.backtrack_level;
            self.assignment.front = saved.queue_front;
            self.clear_conflict();
        }
    }

    pub fn push_root(&mut self, literal: Literal) -> bool {
        if self.has_conflict() {
            return false;
        }
        if self.decision_level() != self.root_level() && !self.pop_root_level(0) {
            return false;
        }
        if self.queue_size() != 0 && !self.propagate() {
            return false;
        }
        if self.value(literal.var()) != value_free {
            return self.is_true(literal);
        }
        if !self.assume(literal) {
            return false;
        }
        self.push_root_level(1);
        self.propagate()
    }

    pub fn push_root_path(&mut self, path: &[Literal]) -> bool {
        self.push_root_path_with_step(path, false)
    }

    pub fn push_root_path_with_step(&mut self, path: &[Literal], push_step: bool) -> bool {
        if !self.pop_root_level(0) || !self.simplify() || !self.propagate() {
            return false;
        }
        if push_step {
            let Some(step_literal) = self.shared_context().map(|shared| shared.step_literal())
            else {
                return false;
            };
            if !self.push_root(step_literal) {
                return false;
            }
        }
        self.stats.add_path(path.len());
        for &literal in path {
            if !self.push_root(literal) {
                return false;
            }
        }
        self.cc_info.set_activity(1);
        true
    }

    pub fn copy_guiding_path(&self, out: &mut LitVec) {
        let mut first_aux = self.root_level.saturating_add(1);
        out.clear();

        for level in 1..=self.root_level {
            let decision = self.decision(level);
            if !self.aux_var(decision.var()) {
                out.push_back(decision);
            } else if level < first_aux {
                first_aux = level;
            }
        }

        for implied in &self.implied_lits {
            if implied.level <= self.root_level
                && (implied.ante.ante().is_null() || implied.level < first_aux)
                && !self.aux_var(implied.lit.var())
            {
                out.push_back(implied.lit);
            }
        }
    }

    pub fn splittable(&self) -> bool {
        if self.decision_level() == self.root_level() || self.frozen_level(self.root_level() + 1) {
            return false;
        }
        if self.num_aux_vars() != 0 {
            let min_aux = self.root_level().saturating_add(2);
            for level in 1..min_aux {
                let decision = self.decision(level);
                if self.aux_var(decision.var()) && decision != self.tag_literal() {
                    return false;
                }
            }
            for implied in &self.implied_lits {
                if implied.ante.ante().is_null()
                    && implied.level < min_aux
                    && self.aux_var(implied.lit.var())
                    && implied.lit != self.tag_literal()
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn split(&mut self, out: &mut LitVec) -> bool {
        if !self.splittable() {
            return false;
        }
        self.copy_guiding_path(out);
        self.push_root_level(1);
        out.push_back(!self.decision(self.root_level()));
        self.split_requested = false;
        self.stats.add_split(1);
        true
    }

    pub fn request_split(&mut self) -> bool {
        self.split_requested = true;
        let result = self.splittable();
        if !result
            && self.decision_level() > self.root_level()
            && !self.frozen_level(self.root_level() + 1)
        {
            self.split_requested = false;
        }
        result
    }

    pub fn clear_split_request(&mut self) -> bool {
        core::mem::replace(&mut self.split_requested, false)
    }

    fn unit_propagate(&mut self) -> bool {
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

    fn cancel_propagation(&mut self) {
        self.assignment.q_reset();
        if self.posts_active {
            let mut current = self.post_propagators.head();
            while let Some(mut ptr) = current {
                unsafe {
                    ptr.as_mut().reset();
                    current = ptr.as_ref().next;
                }
            }
        }
    }

    pub fn simplify(&mut self) -> bool {
        if self.queue_size() > 0 && !self.propagate() {
            return false;
        }
        let root_literals: Vec<Literal> = self
            .assignment
            .trail
            .as_slice()
            .iter()
            .copied()
            .filter(|lit| self.level(lit.var()) == 0 && self.is_true(*lit))
            .collect();
        let solver_ptr: *const Solver = self;
        if let Some(mut shared) = self.shared {
            unsafe {
                let ctx = shared.as_mut();
                for literal in root_literals {
                    ctx.implication_graph_mut()
                        .remove_true(&*solver_ptr, literal);
                }
            }
        }
        self.last_simplify = self.num_assigned_vars();
        true
    }

    pub fn estimate_bcp(&mut self, literal: Literal, mut max_recursion_depth: i32) -> u32 {
        if self.value(literal.var()) != value_free {
            return 0;
        }
        self.assignment.ensure_var(literal.var());
        let first = self.assignment.assigned();
        self.assignment.set_raw_assignment(
            literal.var(),
            crate::clasp::literal::true_value(literal),
            0,
        );
        self.assignment.push_trail_literal(literal);
        let mut index = first as usize;
        if let Some(shared) = self.shared {
            let graph = unsafe { shared.as_ref().implication_graph() };
            let max_idx = graph.size();
            while index < self.assignment.assigned() as usize {
                let next = self.assignment.trail[index];
                index += 1;
                if next.id() < max_idx && !graph.propagate_bin(&mut self.assignment, next, 0) {
                    break;
                }
                if max_recursion_depth == 0 {
                    break;
                }
                max_recursion_depth -= 1;
            }
        }
        let result = self.assignment.assigned() - first;
        while self.assignment.assigned() != first {
            self.assignment.undo_last();
        }
        result
    }

    pub fn test(&mut self, literal: Literal, propagator: Option<NonNull<PostPropagator>>) -> bool {
        if self.value(literal.var()) != value_free || self.has_conflict() {
            return false;
        }
        if !self.assume(literal) {
            return false;
        }
        let probe_level = self.decision_level();
        self.freeze_level(probe_level);
        if self.propagate_until(propagator, None) != value_false {
            if let Some(mut propagator) = propagator {
                unsafe {
                    propagator.as_mut().undo_level(self);
                }
            }
            self.unfreeze_level(probe_level);
            self.undo_until(probe_level.saturating_sub(1));
            return true;
        }
        self.unfreeze_level(probe_level);
        self.cancel_propagation();
        false
    }

    pub fn reverse_arc(&self, literal: Literal, max_level: u32) -> Option<Antecedent> {
        self.shared_context()
            .and_then(|shared| shared.reverse_arc(self, literal, max_level))
    }

    pub fn undo_until(&mut self, level: u32) -> u32 {
        let target = level.max(self.backtrack_level);
        let saved_undo_target = self.undo_target.replace(target);
        while self.decision_level > target {
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
        self.conflict_lits.clear();
        let actual = self.decision_level;
        if self.implied_lits.active(actual) {
            let mut implied_lits = core::mem::take(&mut self.implied_lits);
            let _ = implied_lits.assign(self);
            self.implied_lits = implied_lits;
        }
        self.undo_target = saved_undo_target;
        actual
    }

    pub fn backtrack(&mut self, level: u32) -> bool {
        let target = level.max(self.root_level);
        self.undo_until(target);
        self.backtrack_level = self.backtrack_level.min(self.decision_level);
        true
    }

    pub(crate) fn current_undo_target(&self) -> Option<u32> {
        self.undo_target
    }

    pub fn is_undo_level(&self) -> bool {
        self.decision_level() > self.backtrack_level()
    }

    pub fn backtrack_step(&mut self) -> bool {
        let mut last_choice_inverted;
        loop {
            if self.decision_level == self.root_level {
                self.set_stop_conflict();
                return false;
            }
            last_choice_inverted = !self.decision(self.decision_level);
            let target = self.decision_level.saturating_sub(1).max(self.root_level);
            self.backtrack_level = target;
            self.undo_until(target);
            self.set_backtrack_level(self.decision_level);
            if !self.has_conflict() && self.force(last_choice_inverted, Antecedent::new()) {
                break;
            }
        }
        self.implied_lits.add(
            self.decision_level,
            ImpliedLiteral::new(last_choice_inverted, self.decision_level, Antecedent::new()),
        );
        true
    }

    pub fn clear_assumptions(&mut self) -> bool {
        self.pop_root_level(self.root_level) && self.simplify()
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
        if !clause.prep {
            return ClauseCreator::create_from_rep(self, clause, CLAUSE_FORCE_SIMPLIFY).ok();
        }
        if clause.size > 1 {
            if !self.allow_implicit(clause) {
                return ClauseCreator::create_from_rep(self, clause, CLAUSE_EXPLICIT).ok();
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
                self.stats
                    .add_learnt(clause.size, clause.info.constraint_type());
            }
            return true;
        }
        let unit = clause
            .literals()
            .first()
            .copied()
            .unwrap_or(crate::clasp::literal::lit_false);
        let ok = self.force(
            unit,
            Antecedent::from_literal(crate::clasp::literal::lit_true),
        );
        if ok && is_new && clause.info.learnt() {
            self.stats
                .add_learnt(clause.size, clause.info.constraint_type());
        }
        ok
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

    pub fn prepare_post(&mut self) -> bool {
        if self.has_conflict() {
            return false;
        }
        if let Some(shared) = self.shared_context() {
            self.acquire_problem_var(shared.num_vars());
        }
        if !self.post_init_ready {
            let post_list = &mut self.post_propagators as *mut PropagatorList;
            if !unsafe { (*post_list).init(self) } {
                return false;
            }
            self.post_init_ready = true;
        }
        let solver_id = self.id;
        self.shared_context_mut()
            .is_none_or(|shared| shared.add_post_for_solver(solver_id))
    }

    pub fn set_enumeration_constraint(&mut self, constraint: Option<*mut Constraint>) {
        let next = constraint.filter(|ptr| !ptr.is_null());
        if let Some(prev) = self
            .enum_constraint
            .replace(next.unwrap_or(core::ptr::null_mut()))
        {
            if Some(prev) != next {
                unsafe { Constraint::destroy_raw(prev, Some(self), true) };
            }
        }
        if next.is_none() {
            self.enum_constraint = None;
        }
    }

    pub fn enumeration_constraint(&self) -> Option<*mut Constraint> {
        self.enum_constraint.filter(|ptr| !ptr.is_null())
    }

    pub fn reset_prefs(&mut self) {
        self.assignment.reset_prefs();
    }

    pub fn reset_learnt_activities(&mut self) {
        for &constraint_ptr in &self.learnts {
            unsafe {
                if let Some(constraint) = constraint_ptr.as_mut() {
                    constraint.reset_activity();
                }
            }
        }
    }

    pub fn end_step(&mut self, top: u32, params: SolverParams) -> bool {
        self.post_init_ready = false;
        if !self.pop_root_level(self.root_level) {
            return false;
        }
        let aux_vars = self.num_aux_vars();
        if aux_vars != 0 {
            self.pop_aux_var(aux_vars);
        }
        let step_literal = self
            .shared_context()
            .map(|shared| shared.step_literal())
            .unwrap_or(lit_false);
        let top = top.min(self.last_simplify);
        if let Some(look) = self.get_post(priority_reserved_look) {
            if let Some(removed) = self.remove_post(look) {
                removed.destroy(Some(self), true);
            }
        }
        let okay = (step_literal.var() == 0
            || self.value(step_literal.var()) != value_free
            || self.force(!step_literal, Antecedent::new()))
            && self.simplify();
        if okay && !self.is_master() && self.shared_context().is_some_and(|shared| shared.ok()) {
            let forward: Vec<Literal> = self
                .assignment
                .trail
                .iter()
                .copied()
                .skip(top as usize)
                .filter(|lit| lit.var() != step_literal.var())
                .collect();
            if let Some(mut shared) = self.shared {
                unsafe {
                    let master = shared.as_mut().master();
                    for lit in forward {
                        if !master.force(lit, Antecedent::new()) {
                            break;
                        }
                    }
                }
            }
        }
        if params.forget_learnts() {
            if let Some(search) = self.search_config() {
                let _ = self.reduce_learnts(1.0, &search.reduce.strategy);
            }
        }
        if params.forget_heuristic() {
            self.set_default_heuristic();
        }
        if params.forget_signs() {
            self.reset_prefs();
        }
        if params.forget_activities() {
            self.reset_learnt_activities();
        }
        true
    }

    pub fn receive(&self, _out: &mut [*mut SharedLiterals]) -> u32 {
        0
    }

    pub fn distribute(
        &mut self,
        _lits: &[Literal],
        _extra: ConstraintInfo,
    ) -> Option<*mut SharedLiterals> {
        None
    }

    pub fn num_learnt_constraints(&self) -> u32 {
        self.learnts.len() as u32
    }

    pub fn restart(&mut self) {
        self.undo_until(0);
        self.stats.core.restarts += 1;
        let mut score = self.cc_info.score();
        score.bump_activity();
        self.cc_info.set_score(score);
    }

    pub fn pop_aux_var(&mut self, num: u32) {
        let removable = self.shared_context().map_or(num, |shared| {
            self.num_vars().saturating_sub(shared.num_vars()).min(num)
        });
        if removable != 0 {
            let _ = self.pop_vars(removable, true);
        }
    }

    pub fn pop_vars(&mut self, num: u32, pop_learnt: bool) -> Literal {
        let pop = Literal::new(self.assignment.num_vars().saturating_sub(num), false);
        let mut min_level = self.decision_level.saturating_add(1);
        for implied in &self.implied_lits {
            if implied.lit >= pop {
                min_level = min_level.min(implied.level);
            }
        }
        for var in pop.var()..pop.var().saturating_add(num) {
            if self.value(var) != value_free {
                min_level = min_level.min(self.level(var));
            }
        }
        if min_level > self.root_level {
            self.undo_until(min_level.saturating_sub(1));
        } else {
            let _ =
                self.pop_root_level(self.root_level.saturating_sub(min_level).saturating_add(1));
            if min_level == 0 {
                let mut write = 0usize;
                let mut units = self.assignment.units();
                let mut front = self.assignment.front;
                let mut last_simplify = self.last_simplify;
                let old_len = self.assignment.trail.len();
                for read in 0..old_len {
                    let lit = self.assignment.trail[read];
                    if lit < pop {
                        self.assignment.trail[write] = lit;
                        write += 1;
                    } else {
                        units = units
                            .saturating_sub(u32::from(read < self.assignment.units() as usize));
                        front =
                            front.saturating_sub(u32::from(read < self.assignment.front as usize));
                        last_simplify = last_simplify
                            .saturating_sub(u32::from(read < self.last_simplify as usize));
                    }
                }
                self.assignment.trail.truncate(write);
                self.assignment.front = front.min(self.assignment.trail.len() as u32);
                self.assignment
                    .set_units(units.min(self.assignment.trail.len() as u32));
                self.last_simplify = last_simplify.min(self.assignment.trail.len() as u32);
            }
        }
        for _ in 0..num {
            let _ = self.watches.pop();
            let _ = self.watches.pop();
        }
        if pop_learnt {
            let old_learnts = core::mem::take(&mut self.learnts);
            let mut kept = Vec::with_capacity(old_learnts.len());
            for constraint_ptr in old_learnts {
                let remove = unsafe {
                    constraint_ptr
                        .as_mut()
                        .and_then(|constraint| constraint.clause())
                        .is_some_and(|head| {
                            head.as_ref().aux()
                                && head.as_ref().to_lits().into_iter().any(|lit| lit >= pop)
                        })
                };
                if remove {
                    destroy_constraint_ptr(constraint_ptr, self);
                } else {
                    kept.push(constraint_ptr);
                }
            }
            self.learnts = kept;
        }
        let new_max_var = pop.var().saturating_sub(1);
        self.assignment.truncate_vars(new_max_var);
        self.num_vars = self.num_vars.min(new_max_var);
        self.num_problem_vars = self.num_problem_vars.min(new_max_var);
        if self.tag_literal.var() > new_max_var {
            self.tag_literal = Literal::default();
        }
        let heuristic = self.heuristic.as_mut() as *mut dyn DecisionHeuristic;
        unsafe {
            (*heuristic).update_var(self, pop.var(), num);
        }
        pop
    }

    pub fn reduce_learnts(&mut self, rem_frac: f64, strategy: &ReduceStrategy) -> DBInfo {
        let old_size = self.num_learnt_constraints();
        let max_remove = (f64::from(old_size) * rem_frac.clamp(0.0, 1.0)) as u32;
        let cmp = CmpScore::new(*strategy);
        let reduced = match ReduceAlgorithm::from_underlying(strategy.algo as u8) {
            Some(ReduceAlgorithm::ReduceLinear) => self.reduce_linear(max_remove, cmp),
            Some(ReduceAlgorithm::ReduceStable)
            | Some(ReduceAlgorithm::ReduceSort)
            | Some(ReduceAlgorithm::ReduceHeap)
            | None => self.reduce_sort_in_place(max_remove, cmp),
        };
        self.stats
            .add_deleted(old_size.saturating_sub(reduced.size));
        reduced
    }

    pub fn search_with_limits(&mut self, limit: &mut SearchLimits<'_>, randf: f64) -> ValT {
        assert!(!self.is_false(self.tag_literal()));
        let randf = randf.clamp(0.0, 1.0);
        let mut local_used = 0_u64;
        loop {
            let mut conflict = self.has_conflict() || !self.propagate() || !self.simplify();
            while conflict {
                let mut resolved = 0_u64;
                loop {
                    resolved += 1;
                    if !self.resolve_conflict() {
                        break;
                    }
                    if self.propagate() {
                        break;
                    }
                }
                limit.used = limit.used.saturating_add(resolved);
                local_used = local_used.saturating_add(resolved);
                if self.has_conflict() || (self.decision_level() == 0 && !self.simplify()) {
                    return crate::clasp::literal::value_false;
                }
                if self.num_free_vars() != 0
                    && (limit.used >= limit.conflicts
                        || self.restart_reached(limit, local_used)
                        || self.reduce_reached(limit))
                {
                    return value_free;
                }
                conflict = false;
            }
            if self.decide_next_branch(randf) {
                conflict = !self.propagate();
                if conflict {
                    continue;
                }
            } else if self.is_model() {
                return value_true;
            }
        }
    }

    pub fn search(
        &mut self,
        max_conflicts: u64,
        max_learnts: u32,
        local: bool,
        rand_prob: f64,
    ) -> ValT {
        let mut limit = SearchLimits {
            conflicts: max_conflicts,
            learnts: max_learnts,
            local,
            ..SearchLimits::default()
        };
        self.search_with_limits(&mut limit, rand_prob)
    }

    pub(crate) fn constraint_db(&self) -> &[*mut Constraint] {
        &self.constraints
    }

    pub(crate) fn clone_db(&mut self, db: &[*mut Constraint]) -> bool {
        for &constraint_ptr in db {
            if self.has_conflict {
                break;
            }
            let Some(constraint) = (unsafe { constraint_ptr.as_ref() }) else {
                continue;
            };
            if let Some(clone) = constraint.clone_attach(self) {
                self.constraints.push(Box::into_raw(clone));
            }
        }
        !self.has_conflict
    }

    pub(crate) fn detach_local_runtime(&mut self) {
        let shared = self.shared;
        let solver_id = self.id;
        let mut owned = core::mem::take(&mut self.constraints);
        owned.extend(core::mem::take(&mut self.learnts));
        owned.sort_unstable_by_key(|ptr| *ptr as usize);
        owned.dedup();
        for constraint_ptr in owned {
            if constraint_ptr.is_null() {
                continue;
            }
            unsafe {
                if let Some(constraint) = constraint_ptr.as_mut() {
                    if let Some(mut head) = constraint.clause() {
                        head.as_mut().destroy(Some(self), true);
                    } else {
                        Constraint::destroy_raw(constraint_ptr, Some(self), true);
                    }
                }
            }
        }
        *self = Solver::new();
        self.shared = shared;
        self.id = solver_id;
    }

    pub fn prepare_cc_min_recursive(&mut self, rec: &mut CCMinRecursive) {
        rec.clear();
        rec.open = self.inc_epoch(self.num_vars.max(self.num_problem_vars) as usize + 1, 2) - 2;
    }

    pub fn resolve_conflict(&mut self) -> bool {
        if !self.has_conflict {
            return false;
        }
        if self.decision_level <= self.root_level {
            return false;
        }
        if self.decision_level != self.backtrack_level
            && self.strategies.search != SearchStrategy::NoLearning as u32
        {
            let uip_level = self.analyze_conflict();
            let decision_level = self.decision_level;
            self.stats.add_conflict(
                decision_level,
                uip_level,
                self.backtrack_level,
                self.cc_info.lbd(),
            );
            self.undo_until(uip_level);
            let rep = ClauseRep::prepared(self.cc_lits.as_slice(), self.cc_info);
            return ClauseCreator::create_from_rep(self, &rep, CLAUSE_NO_PREPARE).ok();
        }
        self.backtrack(self.decision_level.saturating_sub(1))
    }

    pub fn cc_minimize(&mut self, lit: Literal, rec: *mut CCMinRecursive) -> bool {
        self.cc_minimize_with_limit(lit, self.strategies.cc_min_antes, rec)
    }

    fn set_conflict(&mut self, literal: Literal, antecedent: Antecedent, data: u32) {
        self.stats.core.conflicts += 1;
        self.has_conflict = true;
        self.conflict_literal = literal;
        self.conflict_reason = antecedent;
        self.conflict_data = data;
        let mut lits = LitVec::new();
        lits.push_back(!literal);
        if self.strategies.search != SearchStrategy::NoLearning as u32 && !antecedent.is_null() {
            if data == u32::MAX {
                let mut reason = antecedent;
                reason.reason(self, literal, &mut lits);
            } else {
                let saved = self.reason_data(literal.var());
                self.set_reason_data(literal.var(), data);
                let mut reason = antecedent;
                reason.reason(self, literal, &mut lits);
                self.set_reason_data(literal.var(), saved);
            }
        }
        self.conflict_lits = lits;
    }

    fn analyze_conflict(&mut self) -> u32 {
        let mut on_level = 0u32;
        let mut res_size = 0u32;
        let mut pivot = Literal::default();
        let mut last_reason = Antecedent::default();
        self.cc_info = ClauseInfo::new(ConstraintType::Conflict);
        self.cc_lits.clear();
        self.cc_lits.push_back(pivot);
        loop {
            let current_reason = self.conflict_lits.as_slice().to_vec();
            for literal in current_reason {
                let clause_level = self.level(literal.var());
                if !self.seen_var(literal.var()) {
                    res_size += 1;
                    debug_assert!(self.is_true(literal), "invalid literal in conflict reason");
                    debug_assert!(clause_level > 0, "top-level implication not marked");
                    self.mark_seen_var(literal.var());
                    if clause_level == self.decision_level {
                        on_level += 1;
                    } else {
                        self.cc_lits.push_back(!literal);
                        self.mark_level(clause_level);
                    }
                }
            }
            debug_assert!(on_level > 0, "conflict must touch the conflict level");
            while !self.seen_var(self.assignment.last().var()) {
                self.assignment.undo_last();
            }
            pivot = self.assignment.last();
            let reason = *self.reason(pivot.var());
            self.clear_seen_var(pivot.var());
            if on_level == 1 {
                break;
            }
            on_level -= 1;
            res_size = res_size.saturating_sub(1);
            last_reason = reason;
            let mut next_conflict = LitVec::new();
            let mut antecedent = reason;
            antecedent.reason(self, pivot, &mut next_conflict);
            self.conflict_lits = next_conflict;
        }
        self.cc_lits[0] = !pivot;
        let last_res = if !last_reason.is_null()
            && last_reason.type_() == Antecedent::GENERIC
            && self.cc_lits.len() <= self.conflict_lits.len() + 1
        {
            last_reason.constraint_mut().clause()
        } else {
            None
        };
        self.simplify_conflict_clause(last_res)
    }

    fn simplify_conflict_clause(&mut self, mut rhs: Option<NonNull<ClauseHead>>) -> u32 {
        let mut removed = LitVec::new();
        let mut recursive = CCMinRecursive::default();
        let recursive_ptr = if self.strategies.cc_min_rec != 0 {
            self.prepare_cc_min_recursive(&mut recursive);
            &mut recursive as *mut CCMinRecursive
        } else {
            core::ptr::null_mut()
        };
        let on_assert =
            self.cc_minimize_clause(&mut removed, self.strategies.cc_min_antes, recursive_ptr);
        let assert_level = if self.cc_lits.len() > 1 {
            self.level(self.cc_lits[1].var())
        } else {
            0
        };
        for literal in removed.as_slice().iter().copied() {
            self.clear_seen_var(literal.var());
        }
        if on_assert == 1 && self.strategies.reverse_arcs > 0 && self.cc_lits.len() > 1 {
            self.mark_seen_var(self.cc_lits[0].var());
            if let Some(antecedent) = self.reverse_arc(self.cc_lits[1], assert_level) {
                let mut reason = LitVec::new();
                let mut antecedent = antecedent;
                antecedent.reason(self, !self.cc_lits[1], &mut reason);
                for literal in reason.as_slice().iter().copied() {
                    if !self.seen_var(literal.var()) {
                        self.mark_level(self.level(literal.var()));
                        self.cc_lits.push_back(!literal);
                    }
                }
                let resolved = self.cc_lits[1];
                self.clear_seen_var(resolved.var());
                self.unmark_level(self.level(resolved.var()));
                let last = self.cc_lits.len() - 1;
                self.cc_lits.as_mut_slice().swap(1, last);
                self.cc_lits.pop_back();
            }
            self.clear_seen_var(self.cc_lits[0].var());
        }
        if let Some(mut rhs_head) = rhs.take() {
            let mut strengthen = LitVec::new();
            self.mark_seen_var(self.cc_lits[0].var());
            let rhs_lits = unsafe { rhs_head.as_ref() }.to_lits();
            let mut marked = self.cc_lits.len() as isize;
            let mut max_missing = rhs_lits.len() as isize - marked;
            for literal in rhs_lits {
                if !self.seen_var(literal.var()) || self.level(literal.var()) == 0 {
                    max_missing -= 1;
                    if max_missing < 0 {
                        break;
                    }
                    strengthen.push_back(literal);
                } else {
                    marked -= 1;
                    if marked == 0 {
                        break;
                    }
                }
            }
            if marked <= 0 {
                for literal in strengthen.as_slice().iter().copied() {
                    let result = unsafe { rhs_head.as_mut() }.strengthen(self, literal, false);
                    if !result.lit_removed {
                        break;
                    }
                }
            }
            self.clear_seen_var(self.cc_lits[0].var());
        }
        self.finalize_conflict_clause()
    }

    fn cc_minimize_clause(
        &mut self,
        removed: &mut LitVec,
        antes: u32,
        recursive: *mut CCMinRecursive,
    ) -> u32 {
        let mut assert_level = 0u32;
        let mut assert_pos = 1usize;
        let mut on_assert = 0u32;
        let mut write = 1usize;
        let snapshot = self.cc_lits.as_slice()[1..].to_vec();
        for literal in snapshot {
            if antes == CCMinAntes::NoAntes as u32 || !self.cc_removable(!literal, antes, recursive)
            {
                let level = self.level(literal.var());
                if level > assert_level {
                    assert_level = level;
                    assert_pos = write;
                    on_assert = 0;
                }
                if level == assert_level {
                    on_assert += 1;
                }
                self.cc_lits[write] = literal;
                write += 1;
            } else {
                removed.push_back(literal);
            }
        }
        shrink_vec_to(&mut self.cc_lits, write);
        if assert_pos != 1 && self.cc_lits.len() > 1 {
            self.cc_lits.as_mut_slice().swap(1, assert_pos);
        }
        on_assert
    }

    fn cc_removable(
        &mut self,
        literal: Literal,
        antes: u32,
        recursive: *mut CCMinRecursive,
    ) -> bool {
        let mut antecedent = *self.reason(literal.var());
        if antecedent.is_null() || antes > antecedent.type_() as u32 {
            return false;
        }
        if recursive.is_null() {
            return antecedent.minimize(self, literal, core::ptr::null_mut());
        }
        unsafe { self.cc_min_recurse(&mut *recursive, antes, literal) }
    }

    fn finalize_conflict_clause(&mut self) -> u32 {
        let mut lbd = 1u32;
        let mut on_root = 0u32;
        let mut assert_level = 0u32;
        let mut assert_pos = 1usize;
        let mut max_var = self.cc_lits[0].var();
        let tag_lit = !self.tag_literal();
        let mut tagged = false;
        for index in 1..self.cc_lits.len() {
            let literal = self.cc_lits[index];
            let var = literal.var();
            self.clear_seen_var(var);
            if literal == tag_lit {
                tagged = true;
            }
            if var > max_var {
                max_var = var;
            }
            let level = self.level(var);
            if level > assert_level {
                assert_level = level;
                assert_pos = index;
            }
            if self.has_level(level) {
                self.unmark_level(level);
                if level > self.root_level {
                    lbd += 1;
                } else {
                    on_root += 1;
                    if on_root == 1 {
                        lbd += 1;
                    }
                }
            }
        }
        if assert_pos != 1 && self.cc_lits.len() > 1 {
            self.cc_lits.as_mut_slice().swap(1, assert_pos);
        }
        let mut rep_mode = if self.cc_lits.len()
            < core::cmp::max(
                self.strategies.compress as usize,
                self.decision_level as usize + 1,
            ) {
            CCRepMode::CcNoReplace as u32
        } else {
            self.strategies.cc_rep_mode
        };
        if rep_mode == CCRepMode::CcRepDynamic as u32 {
            rep_mode = if ratio(u64::from(lbd), u64::from(self.decision_level())) > 0.66 {
                CCRepMode::CcRepDecision as u32
            } else {
                CCRepMode::CcRepUip as u32
            };
        }
        if rep_mode != CCRepMode::CcNoReplace as u32 {
            max_var = self.cc_lits[0].var();
            tagged = false;
            lbd = 1;
            if rep_mode == CCRepMode::CcRepDecision as u32 {
                self.cc_lits.truncate(1);
                for level in (1..=assert_level).rev() {
                    let decision = !self.decision(level);
                    self.cc_lits.push_back(decision);
                    lbd += 1;
                    if decision == tag_lit {
                        tagged = true;
                    }
                    if decision.var() > max_var {
                        max_var = decision.var();
                    }
                }
            } else {
                let mut marked = self.cc_lits.len() as u32 - 1;
                while self.cc_lits.len() > 1 {
                    let lit = self.cc_lits[self.cc_lits.len() - 1];
                    self.mark_seen_literal(!lit);
                    self.cc_lits.pop_back();
                }
                let trail = self.assignment.trail.as_slice().to_vec();
                let mut trail_pos = trail.len();
                while marked != 0 {
                    loop {
                        trail_pos -= 1;
                        if self.seen_literal(trail[trail_pos]) {
                            break;
                        }
                    }
                    let trail_lit = trail[trail_pos];
                    let resolve_next = marked != 1 && !self.reason(trail_lit.var()).is_null();
                    let mut next = trail_pos;
                    if resolve_next {
                        let stop = self.level_start(self.level(trail_lit.var())) as usize;
                        while next > stop {
                            next -= 1;
                            if self.seen_literal(trail[next]) {
                                break;
                            }
                        }
                    }
                    marked -= 1;
                    self.clear_seen_var(trail_lit.var());
                    if !resolve_next || self.level(trail[next].var()) != self.level(trail_lit.var())
                    {
                        self.cc_lits.push_back(!trail_lit);
                        if trail_lit.var() == tag_lit.var() {
                            tagged = true;
                        }
                        if trail_lit.var() > max_var {
                            max_var = trail_lit.var();
                        }
                    } else {
                        let mut antecedent = *self.reason(trail_lit.var());
                        let mut reason_snapshot = LitVec::new();
                        antecedent.reason(self, trail_lit, &mut reason_snapshot);
                        let reason_snapshot = reason_snapshot.as_slice().to_vec();
                        for lit in reason_snapshot {
                            if !self.seen_literal(lit) {
                                marked += 1;
                                self.mark_seen_literal(lit);
                            }
                        }
                    }
                }
                lbd = self.cc_lits.len() as u32;
            }
        }
        self.cc_info
            .set_score(ConstraintScore::new(self.cc_info.activity(), lbd));
        self.cc_info.set_tagged(tagged);
        self.cc_info.set_aux(self.aux_var(max_var));
        assert_level
    }

    fn cc_minimize_with_limit(
        &mut self,
        lit: Literal,
        antes: u32,
        rec: *mut CCMinRecursive,
    ) -> bool {
        if self.seen_var(lit.var()) {
            return true;
        }
        if rec.is_null() {
            return false;
        }
        let level = self.level(lit.var());
        level != 0
            && level != u32::MAX
            && self.has_level(level)
            && unsafe { self.cc_min_recurse(&mut *rec, antes, lit) }
    }

    fn cc_min_recurse(&mut self, rec: &mut CCMinRecursive, antes: u32, lit: Literal) -> bool {
        let state = rec.decode_state(self.epoch(lit.var()));
        match state {
            CCMinState::Poison => return false,
            CCMinState::Open => {
                let mut start = lit;
                start.unflag();
                rec.push(start);
            }
            CCMinState::Removable => return true,
        }

        let mut dfs_state = CCMinState::Removable;
        let rec_ptr = rec as *mut CCMinRecursive;
        loop {
            let mut current = rec.pop();
            if current.flagged() {
                if current == lit {
                    return dfs_state == CCMinState::Removable;
                }
                self.set_epoch(current.var(), rec.encode_state(dfs_state));
            } else if dfs_state != CCMinState::Poison {
                let state = rec.decode_state(self.epoch(current.var()));
                if state == CCMinState::Open {
                    current.flag();
                    rec.push(current);
                    let mut next = *self.reason(current.var());
                    if next.is_null()
                        || antes > next.type_() as u32
                        || !next.minimize(self, current, rec_ptr)
                    {
                        dfs_state = CCMinState::Poison;
                    }
                } else if state == CCMinState::Poison {
                    dfs_state = CCMinState::Poison;
                }
            }
        }
    }

    fn epoch(&mut self, index: u32) -> u32 {
        let index = index as usize;
        if self.epochs.len() <= index {
            self.epochs.resize(index + 1, 0);
        }
        self.epochs[index]
    }

    fn set_epoch(&mut self, index: u32, value: u32) {
        let index = index as usize;
        if self.epochs.len() <= index {
            self.epochs.resize(index + 1, 0);
        }
        self.epochs[index] = value;
    }

    fn inc_epoch(&mut self, size: usize, step: u32) -> u32 {
        if self.epochs.len() < size {
            self.epochs.resize(size, 0);
        }
        if u32::MAX - self.epochs[0] < step {
            self.epochs.fill(0);
        }
        self.epochs[0] = self.epochs[0].saturating_add(step);
        self.epochs[0]
    }

    fn find_post_link(
        &mut self,
        target: Option<NonNull<PostPropagator>>,
    ) -> Option<*mut Option<NonNull<PostPropagator>>> {
        if target.is_none() {
            return Some(&mut self.post_propagators.head as *mut Option<NonNull<PostPropagator>>);
        }
        let mut link = &mut self.post_propagators.head as *mut Option<NonNull<PostPropagator>>;
        unsafe {
            loop {
                match (*link, target) {
                    (current, wanted) if current == wanted => return Some(link),
                    (Some(mut current), _) => {
                        link = &mut current.as_mut().next as *mut Option<NonNull<PostPropagator>>;
                    }
                    (None, _) => return None,
                }
            }
        }
    }

    fn post_propagate(
        &mut self,
        start: Option<NonNull<PostPropagator>>,
        stop: Option<NonNull<PostPropagator>>,
        mut max_prio: Option<&mut u32>,
    ) -> bool {
        if !self.posts_active {
            return true;
        }
        let Some(mut link) = self.find_post_link(start) else {
            return start.is_none();
        };
        unsafe {
            while (*link) != stop {
                let mut current = match *link {
                    Some(ptr) => ptr,
                    None => break,
                };
                if let Some(limit) = max_prio.as_deref_mut() {
                    if current.as_ref().priority() > *limit {
                        *limit = u32::MAX;
                        return true;
                    }
                }
                if !current.as_mut().propagate_fixpoint(self, stop) {
                    return false;
                }
                if *link == Some(current) {
                    link = &mut current.as_mut().next as *mut Option<NonNull<PostPropagator>>;
                }
            }
        }
        true
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

    fn decide_next_branch(&mut self, randf: f64) -> bool {
        if self.num_free_vars() == 0 {
            return false;
        }
        if randf <= 0.0 || self.rng.drand() >= randf {
            let solver_ptr = self as *mut Solver;
            let chose = unsafe { (*solver_ptr).heuristic.select(&mut *solver_ptr) };
            self.stats.core.choices += u64::from(chose);
            return chose;
        }
        let max_var = self.num_vars() + 1;
        let mut var = self.rng.irand(max_var).max(1);
        loop {
            if self.value(var) == value_free {
                let choice = self.heuristic.select_literal(self, var, 0);
                let chose = self.assume(choice);
                self.stats.core.choices += u64::from(chose);
                return chose;
            }
            var += 1;
            if var == max_var {
                var = 1;
            }
        }
    }

    fn reduce_reached(&self, limits: &SearchLimits<'_>) -> bool {
        self.num_learnt_constraints() > limits.learnts || self.learnt_bytes > limits.memory
    }

    fn restart_reached(&self, limits: &SearchLimits<'_>, local_used: u64) -> bool {
        let used = if limits.local {
            local_used
        } else {
            limits.used
        };
        used >= limits.restart_conflicts
            || limits
                .dynamic
                .is_some_and(crate::clasp::solver_strategies::DynamicLimit::reached)
    }

    fn is_model(&mut self) -> bool {
        if self.has_conflict() {
            return false;
        }
        let post_list = &mut self.post_propagators as *mut PropagatorList;
        unsafe { (*post_list).is_model(self) }
    }

    fn reduce_linear(&mut self, mut max_remove: u32, cmp: CmpScore) -> DBInfo {
        let old_learnts = core::mem::take(&mut self.learnts);
        let score_sum = old_learnts
            .iter()
            .filter_map(|&ptr| unsafe { ptr.as_ref() })
            .map(|constraint| u64::from(cmp.score(constraint.activity())))
            .sum::<u64>();
        let avg_activity = if old_learnts.is_empty() {
            0.0
        } else {
            score_sum as f64 / old_learnts.len() as f64
        };
        let score_max = f64::from(cmp.score(ConstraintScore::new(act_max, 1)));
        let mut score_thresh = avg_activity * 1.5;
        if score_thresh > score_max {
            score_thresh = (score_max + avg_activity) / 2.0;
        }
        let mut kept = Vec::with_capacity(old_learnts.len());
        let mut info = DBInfo::default();
        let solver_ptr = self as *mut Solver;
        for constraint_ptr in old_learnts {
            if constraint_ptr.is_null() {
                kept.push(constraint_ptr);
                continue;
            }
            let constraint = unsafe { &mut *constraint_ptr };
            let score = constraint.activity();
            let is_locked = unsafe { constraint.locked(&*solver_ptr) };
            let is_glue = f64::from(cmp.score(score)) > score_thresh || cmp.is_glue(score);
            if max_remove == 0 || is_locked || is_glue || cmp.is_frozen(score) {
                info.pinned += u32::from(is_glue);
                info.locked += u32::from(is_locked);
                constraint.decrease_activity();
                kept.push(constraint_ptr);
            } else {
                max_remove -= 1;
                destroy_constraint_ptr(constraint_ptr, self);
            }
        }
        info.size = kept.len() as u32;
        self.learnts = kept;
        info
    }

    fn reduce_sort_in_place(&mut self, mut max_remove: u32, cmp: CmpScore) -> DBInfo {
        let mut old_learnts = core::mem::take(&mut self.learnts);
        old_learnts.sort_by(|lhs, rhs| compare_learnts(*lhs, *rhs, cmp));
        let mut kept = Vec::with_capacity(old_learnts.len());
        let mut info = DBInfo::default();
        let solver_ptr = self as *mut Solver;
        for constraint_ptr in old_learnts {
            if constraint_ptr.is_null() {
                kept.push(constraint_ptr);
                continue;
            }
            let constraint = unsafe { &mut *constraint_ptr };
            let score = constraint.activity();
            let is_glue = cmp.is_glue(score);
            let is_locked = unsafe { constraint.locked(&*solver_ptr) };
            if max_remove == 0 || is_locked || is_glue || cmp.is_frozen(score) {
                info.pinned += u32::from(is_glue);
                info.locked += u32::from(is_locked);
                constraint.decrease_activity();
                kept.push(constraint_ptr);
            } else {
                max_remove -= 1;
                destroy_constraint_ptr(constraint_ptr, self);
            }
        }
        info.size = kept.len() as u32;
        self.learnts = kept;
        info
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DBInfo {
    pub size: u32,
    pub locked: u32,
    pub pinned: u32,
}

#[derive(Clone, Copy, Debug)]
struct CmpScore {
    strategy: ReduceStrategy,
}

impl CmpScore {
    const fn new(strategy: ReduceStrategy) -> Self {
        Self { strategy }
    }

    fn score(self, score: ConstraintScore) -> u32 {
        self.strategy.as_score(score)
    }

    fn is_frozen(self, score: ConstraintScore) -> bool {
        score.bumped() && score.lbd() <= self.strategy.protect
    }

    fn is_glue(self, score: ConstraintScore) -> bool {
        score.lbd() <= self.strategy.glue
    }
}

fn compare_learnts(lhs: *mut Constraint, rhs: *mut Constraint, cmp: CmpScore) -> Ordering {
    match (unsafe { lhs.as_ref() }, unsafe { rhs.as_ref() }) {
        (Some(lhs), Some(rhs)) => {
            let result = cmp.strategy.compare(lhs.activity(), rhs.activity());
            result.cmp(&0)
        }
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn destroy_constraint_ptr(constraint_ptr: *mut Constraint, solver: &mut Solver) {
    unsafe {
        if let Some(constraint) = constraint_ptr.as_mut() {
            if let Some(mut head) = constraint.clause() {
                head.as_mut().destroy(Some(solver), true);
            } else {
                Constraint::destroy_raw(constraint_ptr, Some(solver), true);
            }
        }
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

    fn undo_level(&mut self, _s: &mut Solver) {}

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

    pub fn undo_level(&mut self, s: &mut Solver) {
        self.inner.undo_level(s);
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

    pub fn add(&mut self, propagator: Box<PostPropagator>) -> NonNull<PostPropagator> {
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
        new_ptr
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
