//! Partial Rust port of `original_clasp/clasp/shared_context.h` and
//! `original_clasp/src/shared_context.cpp`.
//!
//! This module ports the `ProblemStats` aggregate together with the minimal
//! Bundle A runtime seam needed before clause runtime: variable metadata,
//! a short-implication graph, and a concrete `SharedContext` owning the master
//! solver.

use crate::clasp::constraint::{Antecedent, ConstraintType, Solver};
use crate::clasp::literal::{Literal, VarType, true_value, value_free};
use crate::clasp::statistics::{StatisticMap, StatisticObject};
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortImplicationsGraph {
    graph: Vec<ShortImplicationNode>,
    binary: [u32; 2],
    ternary: [u32; 2],
    shared: bool,
}

impl ShortImplicationsGraph {
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
        let index = usize::from(learnt);
        let max_id = lits.iter().map(|lit| (!*lit).id()).max().unwrap_or(0) + 1;
        if self.graph.len() < max_id as usize {
            self.resize(max_id);
        }
        let added = if lits.len() == 2 {
            self.add_binary(lits[0], lits[1])
        } else {
            self.add_ternary(lits[0], lits[1], lits[2])
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
}

#[derive(Debug)]
pub struct SharedContext {
    stats: ProblemStats,
    var_info: Vec<VarInfo>,
    btig: ShortImplicationsGraph,
    master: Box<Solver>,
    frozen: bool,
    share_problem: bool,
    share_learnts: bool,
}

impl Default for SharedContext {
    fn default() -> Self {
        let mut context = Self {
            stats: ProblemStats::default(),
            var_info: vec![VarInfo::default()],
            btig: ShortImplicationsGraph::default(),
            master: Box::new(Solver::new()),
            frozen: false,
            share_problem: false,
            share_learnts: false,
        };
        context.refresh_master_link();
        context
    }
}

impl SharedContext {
    pub fn new() -> Self {
        Self::default()
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

    pub fn add_var(&mut self) -> u32 {
        self.add_typed_var(VarType::Atom, VarInfo::FLAG_NANT | VarInfo::FLAG_INPUT)
    }

    pub fn add_typed_var(&mut self, var_type: VarType, flags: u8) -> u32 {
        let mut info = VarInfo::new(flags);
        if matches!(var_type, VarType::Body) {
            info.set(VarInfo::FLAG_BODY);
        }
        if matches!(var_type, VarType::Hybrid) {
            info.set(VarInfo::FLAG_EQ);
        }
        self.var_info.push(info);
        self.stats.vars.num = self.num_vars();
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

    pub fn var_info(&self, var: u32) -> VarInfo {
        self.var_info[var as usize]
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

    pub fn master(&mut self) -> &mut Solver {
        self.refresh_master_link();
        &mut self.master
    }

    pub fn master_ref(&self) -> &Solver {
        &self.master
    }

    pub fn start_add_constraints(&mut self) -> &mut Solver {
        self.refresh_master_link();
        self.frozen = false;
        self.btig.resize((self.num_vars() + 1) << 1);
        self.master.acquire_problem_var(self.num_vars());
        self.master
            .reserve_watch_capacity(((self.num_vars() + 1) << 1) as usize);
        &mut self.master
    }

    pub fn end_init(&mut self) -> bool {
        self.refresh_master_link();
        self.master.acquire_problem_var(self.num_vars());
        self.stats.constraints.other = self.master.num_constraints();
        self.stats.constraints.binary = self.btig.num_binary();
        self.stats.constraints.ternary = self.btig.num_ternary();
        self.btig.mark_shared(false);
        self.frozen = true;
        self.master.propagate()
    }

    pub fn num_binary(&self) -> u32 {
        self.btig.num_binary()
    }

    pub fn num_ternary(&self) -> u32 {
        self.btig.num_ternary()
    }

    pub fn num_constraints(&self) -> u32 {
        self.num_binary() + self.num_ternary() + self.master.num_constraints()
    }

    pub fn allow_implicit(&self, constraint_type: ConstraintType) -> bool {
        if matches!(constraint_type, ConstraintType::Static) {
            !self.physical_share_problem()
        } else {
            true
        }
    }

    pub fn add_imp(&mut self, lits: &[Literal], constraint_type: ConstraintType) -> i32 {
        if !self.allow_implicit(constraint_type) {
            return -1;
        }
        i32::from(
            self.btig
                .add(lits, !matches!(constraint_type, ConstraintType::Static)),
        )
    }

    pub fn physical_share_problem(&self) -> bool {
        self.share_problem
    }

    pub fn physical_share(&self, constraint_type: ConstraintType) -> bool {
        if matches!(constraint_type, ConstraintType::Static) {
            self.share_problem
        } else {
            self.share_learnts
        }
    }

    pub(crate) fn implication_graph(&self) -> &ShortImplicationsGraph {
        &self.btig
    }

    fn refresh_master_link(&mut self) {
        let this = self as *mut SharedContext;
        self.master.set_shared_context(this);
    }
}
