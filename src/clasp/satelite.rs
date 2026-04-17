//! Partial Rust port of `original_clasp/clasp/satelite.h` and the
//! solver-independent helper logic from `original_clasp/src/satelite.cpp`.
//!
//! This module currently ports the occurrence bookkeeping, clause abstraction,
//! queue/mark helpers, and subsumption-related primitives that do not require a
//! concrete `SharedContext` or `Solver`. The preprocessing loop itself remains
//! blocked on the unported shared-context, clause, and solver integration.

use crate::clasp::literal::{Literal, Var_t, lit_false, lit_true};
use crate::clasp::util::left_right_sequence::LeftRightSequence;

pub const EVENT_BCE: u8 = b'B';
pub const EVENT_VAR_ELIM: u8 = b'E';
pub const EVENT_SUBSUMPTION: u8 = b'S';

type ClWList = LeftRightSequence<Literal, u32, 0>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SatPreClause {
    lits: Vec<Literal>,
    abstraction: u64,
    in_queue: bool,
    marked: bool,
}

impl SatPreClause {
    pub fn new(lits: Vec<Literal>) -> Self {
        let abstraction = lits
            .iter()
            .fold(0u64, |acc, &lit| acc | Self::abstract_lit(lit));
        Self {
            lits,
            abstraction,
            in_queue: false,
            marked: false,
        }
    }

    pub const fn abstract_lit(lit: Literal) -> u64 {
        1u64 << ((lit.var() - 1) & 63)
    }

    pub fn size(&self) -> u32 {
        self.lits.len() as u32
    }

    pub fn lits(&self) -> &[Literal] {
        &self.lits
    }

    pub fn lit(&self, index: usize) -> Literal {
        self.lits[index]
    }

    pub fn swap_lits(&mut self, lhs: usize, rhs: usize) {
        self.lits.swap(lhs, rhs);
    }

    pub fn abstraction(&self) -> u64 {
        self.abstraction
    }

    pub fn in_queue(&self) -> bool {
        self.in_queue
    }

    pub fn set_in_queue(&mut self, in_queue: bool) {
        self.in_queue = in_queue;
    }

    pub fn marked(&self) -> bool {
        self.marked
    }

    pub fn set_marked(&mut self, marked: bool) {
        self.marked = marked;
    }

    pub fn strengthen(&mut self, literal: Literal) {
        let position = self
            .lits
            .iter()
            .position(|&candidate| candidate == literal)
            .expect("literal to strengthen must exist in clause");
        self.lits.remove(position);
        self.abstraction = self
            .lits
            .iter()
            .fold(0u64, |acc, &lit| acc | Self::abstract_lit(lit));
    }
}

#[derive(Default)]
pub struct OccurList {
    refs: ClWList,
    pos: u32,
    bce: bool,
    dirty: bool,
    neg: u32,
    lit_mark: u32,
}

impl OccurList {
    pub fn num_occ(&self) -> u32 {
        self.pos + self.neg
    }

    pub fn cost(&self) -> u32 {
        self.pos * self.neg
    }

    pub fn clause_range(&self) -> &[Literal] {
        self.refs.left_view()
    }

    pub fn watchers(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.refs.right_view().copied()
    }

    pub fn replace_watchers(&mut self, watchers: &[u32]) {
        self.refs.shrink_right_to(0);
        for &watch in watchers {
            self.refs.push_right(watch);
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn add_watch(&mut self, clause_id: u32) {
        self.refs.push_right(clause_id);
    }

    pub fn remove_watch(&mut self, clause_id: u32) {
        let index = self
            .watchers()
            .position(|watch| watch == clause_id)
            .expect("watch must exist");
        self.refs.erase_right(index);
    }

    pub fn add(&mut self, id: u32, sign: bool) {
        self.pos += u32::from(!sign);
        self.neg += u32::from(sign);
        self.refs.push_left(Literal::new(id, sign));
    }

    pub fn remove(&mut self, id: u32, sign: bool, update_clause_list: bool) {
        self.pos -= u32::from(!sign);
        self.neg -= u32::from(sign);
        if update_clause_list {
            let key = Literal::new(id, sign);
            let index = self
                .clause_range()
                .iter()
                .position(|&entry| entry == key)
                .expect("occurrence must exist");
            self.refs.erase_left(index);
        } else {
            self.dirty = true;
        }
    }

    pub const fn mask(sign: bool) -> u32 {
        1u32 + sign as u32
    }

    pub fn marked(&self, sign: bool) -> bool {
        (self.lit_mark & Self::mask(sign)) != 0
    }

    pub fn mark(&mut self, sign: bool) {
        self.lit_mark = Self::mask(sign);
    }

    pub fn unmark(&mut self) {
        self.lit_mark = 0;
    }

    pub fn lit_mark(&self) -> u32 {
        self.lit_mark
    }

    pub fn bce(&self) -> bool {
        self.bce
    }

    pub fn set_bce(&mut self, value: bool) {
        self.bce = value;
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, value: bool) {
        self.dirty = value;
    }

    pub fn pos(&self) -> u32 {
        self.pos
    }

    pub fn neg(&self) -> u32 {
        self.neg
    }
}

pub fn less_occ_cost(occurs: &[OccurList], lhs: Var_t, rhs: Var_t) -> bool {
    occurs[lhs as usize].cost() < occurs[rhs as usize].cost()
}

#[derive(Default)]
pub struct SatElite {
    occurs: Vec<OccurList>,
    n_occ: u32,
}

impl SatElite {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resize_occ(&mut self, ns: u32) {
        if ns > self.n_occ {
            let grown = ((u64::from(self.n_occ) * 3) / 2).min(u64::from(u32::MAX)) as u32;
            let target = ns.max(grown);
            self.occurs.resize_with(target as usize, OccurList::default);
            self.n_occ = target;
        }
    }

    pub fn cleanup(&mut self) {
        self.occurs.clear();
        self.n_occ = 0;
    }

    pub fn num_occ_slots(&self) -> u32 {
        self.n_occ
    }

    pub fn occur(&self, var: Var_t) -> &OccurList {
        &self.occurs[var as usize]
    }

    pub fn occur_mut(&mut self, var: Var_t) -> &mut OccurList {
        &mut self.occurs[var as usize]
    }

    pub fn mark_all(&mut self, lits: &[Literal]) {
        for &lit in lits {
            self.occur_mut(lit.var()).mark(lit.sign());
        }
    }

    pub fn unmark_all(&mut self, lits: &[Literal]) {
        for &lit in lits {
            self.occur_mut(lit.var()).unmark();
        }
    }

    pub fn find_unmarked_lit(&self, clause: &SatPreClause, start: u32) -> u32 {
        let mut index = start as usize;
        while index < clause.lits().len() {
            let lit = clause.lits()[index];
            if !self.occur(lit.var()).marked(lit.sign()) {
                break;
            }
            index += 1;
        }
        index as u32
    }

    pub fn subsumes(
        &mut self,
        clause: &SatPreClause,
        other: &SatPreClause,
        mut res: Literal,
    ) -> Literal {
        if other.size() < clause.size() || (clause.abstraction() & !other.abstraction()) != 0 {
            return lit_false;
        }
        if clause.size() < 10 || other.size() < 10 {
            for &lhs in clause.lits() {
                if let Some(rhs) = other.lits().iter().find(|&&lit| lit.var() == lhs.var()) {
                    if lhs.sign() == rhs.sign() {
                        continue;
                    }
                    if res == lit_true || res == lhs {
                        res = lhs;
                        continue;
                    }
                }
                return lit_false;
            }
        } else {
            self.mark_all(other.lits());
            for &lit in clause.lits() {
                if self.occur(lit.var()).lit_mark() == 0 {
                    res = lit_false;
                    break;
                }
                if self.occur(lit.var()).marked(!lit.sign()) {
                    if res != lit_true && res != lit {
                        res = lit_false;
                        break;
                    }
                    res = lit;
                }
            }
            self.unmark_all(other.lits());
        }
        res
    }

    pub fn subsumed(
        &mut self,
        clause_lits: &mut Vec<Literal>,
        clauses: &mut [SatPreClause],
    ) -> bool {
        let mut strengthen = 0i32;
        let mut write = 0usize;
        let original_len = clause_lits.len();
        for read in 0..original_len {
            let lit = clause_lits[read];
            if self.occur(lit.var()).lit_mark() == 0 {
                strengthen -= 1;
                continue;
            }

            let current_watchers = self.occur(lit.var()).watchers().collect::<Vec<_>>();
            let mut kept_watchers = Vec::with_capacity(current_watchers.len());
            let mut remove_lit = false;

            for (watch_index, clause_id) in current_watchers.iter().copied().enumerate() {
                let clause_index = clause_id as usize;
                let clause = &mut clauses[clause_index];
                if clause.lit(0) == lit {
                    let next_unmarked = self.find_unmarked_lit(clause, 1) as usize;
                    if next_unmarked == clause.lits().len() {
                        kept_watchers.extend_from_slice(&current_watchers[watch_index..]);
                        self.occur_mut(lit.var()).replace_watchers(&kept_watchers);
                        return true;
                    }
                    clause.swap_lits(0, next_unmarked);
                    let new_watch = clause.lit(0);
                    self.occur_mut(new_watch.var()).add_watch(clause_id);
                    if self.occur(new_watch.var()).lit_mark() != 0
                        && self.find_unmarked_lit(clause, (next_unmarked + 1) as u32)
                            == clause.size()
                    {
                        self.occur_mut(new_watch.var()).unmark();
                        strengthen += 1;
                    }
                } else if self.find_unmarked_lit(clause, 1) == clause.size() {
                    self.occur_mut(lit.var()).unmark();
                    kept_watchers.extend_from_slice(&current_watchers[watch_index..]);
                    remove_lit = true;
                    break;
                } else {
                    kept_watchers.push(clause_id);
                }
            }

            self.occur_mut(lit.var()).replace_watchers(&kept_watchers);
            if !remove_lit {
                if write != read {
                    clause_lits[write] = clause_lits[read];
                }
                write += 1;
            }
        }

        clause_lits.truncate(write);
        while strengthen > 0 {
            let remove_index = clause_lits
                .iter()
                .position(|lit| self.occur(lit.var()).lit_mark() == 0)
                .expect("expected an unmarked literal to remove after strengthening");
            let last = clause_lits.pop().expect("clause_lits must be non-empty");
            if remove_index < clause_lits.len() {
                clause_lits[remove_index] = last;
            }
            strengthen -= 1;
        }
        false
    }

    pub fn trivial_resolvent(&self, clause: &SatPreClause, pivot: Var_t) -> bool {
        clause
            .lits()
            .iter()
            .copied()
            .any(|lit| lit.var() != pivot && self.occur(lit.var()).marked(!lit.sign()))
    }
}
