use core::ops::{Index, IndexMut};

use crate::clasp::constraint::{Antecedent, ClauseHead, Constraint, PropResult, Solver};
use crate::clasp::literal::{
    LitVec, Literal, ValT, ValueVec, Var_t, VarVec, true_value, value_free, value_true,
};
use crate::clasp::pod_vector::{PodVectorT, VectorLike, size32};
use crate::clasp::util::left_right_sequence::LeftRightSequence;
use crate::potassco::bits::{
    right_most_bit, store_clear_mask, store_set_mask, test_any, test_mask,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClauseWatch {
    pub head: *mut ClauseHead,
}

impl ClauseWatch {
    pub const fn new(head: *mut ClauseHead) -> Self {
        Self { head }
    }

    pub const fn eq_head(head: *mut ClauseHead) -> ClauseWatchEqHead {
        ClauseWatchEqHead::new(head)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClauseWatchEqHead {
    pub head: *mut ClauseHead,
}

impl ClauseWatchEqHead {
    pub const fn new(head: *mut ClauseHead) -> Self {
        Self { head }
    }

    pub fn matches(&self, watch: &ClauseWatch) -> bool {
        self.head == watch.head
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericWatch {
    pub con: *mut Constraint,
    pub data: u32,
}

impl GenericWatch {
    pub const fn new(con: *mut Constraint, data: u32) -> Self {
        Self { con, data }
    }

    pub const fn eq_constraint(con: *mut Constraint) -> GenericWatchEqConstraint {
        GenericWatchEqConstraint::new(con)
    }

    pub fn propagate(&mut self, solver: &mut Solver, literal: Literal) -> PropResult {
        let constraint = unsafe { self.con.as_mut() }
            .expect("generic watch requires a non-null constraint pointer");
        constraint.propagate(solver, literal, &mut self.data)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericWatchEqConstraint {
    pub con: *mut Constraint,
}

impl GenericWatchEqConstraint {
    pub const fn new(con: *mut Constraint) -> Self {
        Self { con }
    }

    pub fn matches(&self, watch: &GenericWatch) -> bool {
        self.con == watch.con
    }
}

pub type WatchList = LeftRightSequence<ClauseWatch, GenericWatch, 0>;

pub fn release_vec(watches: &mut WatchList) {
    watches.reset();
}

#[allow(non_snake_case)]
pub fn releaseVec(watches: &mut WatchList) {
    release_vec(watches);
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasonStore32 {
    entries: PodVectorT<Antecedent>,
}

impl ReasonStore32 {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn reserve(&mut self, count: usize) {
        self.entries.reserve(count);
    }

    pub fn resize(&mut self, new_len: usize) {
        self.entries.resize(new_len, Antecedent::new());
    }

    pub fn truncate(&mut self, new_len: usize) {
        self.entries.truncate(new_len);
    }

    pub fn push_back(&mut self, antecedent: Antecedent) {
        self.entries.push_back(antecedent);
    }

    pub fn data(&self, var: u32) -> u32 {
        Self::decode(&self.entries[var as usize])
    }

    pub fn set_data(&mut self, var: u32, data: u32) {
        Self::encode(&mut self.entries[var as usize], data);
    }

    pub fn encode(antecedent: &mut Antecedent, data: u32) {
        *antecedent.as_u64_mut() =
            (u64::from(data) << 32) | u64::from(*antecedent.as_u64_mut() as u32);
    }

    pub fn decode(antecedent: &Antecedent) -> u32 {
        (antecedent.as_u64() >> 32) as u32
    }
}

impl Index<usize> for ReasonStore32 {
    type Output = Antecedent;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for ReasonStore32 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasonStore32Value {
    antecedent: Antecedent,
}

impl ReasonStore32Value {
    pub fn new(antecedent: Antecedent, data: u32) -> Self {
        let mut value = antecedent;
        if data != u32::MAX {
            ReasonStore32::encode(&mut value, data);
            assert_eq!(value.type_(), Antecedent::GENERIC);
        }
        Self { antecedent: value }
    }

    pub const fn ante(&self) -> Antecedent {
        self.antecedent
    }

    pub fn data(&self) -> u32 {
        if self.antecedent.type_() == Antecedent::GENERIC {
            ReasonStore32::decode(&self.antecedent)
        } else {
            u32::MAX
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasonStore64 {
    entries: PodVectorT<Antecedent>,
    pub dv: VarVec,
}

impl ReasonStore64 {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn reserve(&mut self, count: usize) {
        self.entries.reserve(count);
    }

    pub fn resize(&mut self, new_len: usize) {
        self.entries.resize(new_len, Antecedent::new());
    }

    pub fn truncate(&mut self, new_len: usize) {
        self.entries.truncate(new_len);
        self.dv.truncate(new_len);
    }

    pub fn push_back(&mut self, antecedent: Antecedent) {
        self.entries.push_back(antecedent);
    }

    pub fn data_size(&self) -> u32 {
        size32(&self.dv)
    }

    pub fn data_resize(&mut self, new_len: u32) {
        if new_len > self.data_size() {
            self.dv.resize(new_len as usize, u32::MAX);
        }
    }

    pub fn data(&self, var: u32) -> u32 {
        if var < self.data_size() {
            self.dv[var as usize]
        } else {
            u32::MAX
        }
    }

    pub fn set_data(&mut self, var: u32, data: u32) {
        self.data_resize(var + 1);
        self.dv[var as usize] = data;
    }
}

impl Index<usize> for ReasonStore64 {
    type Output = Antecedent;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for ReasonStore64 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasonStore64Value {
    antecedent: Antecedent,
    data: u32,
}

impl ReasonStore64Value {
    pub const fn new(antecedent: Antecedent, data: u32) -> Self {
        Self { antecedent, data }
    }

    pub const fn ante(&self) -> Antecedent {
        self.antecedent
    }

    pub const fn data(&self) -> u32 {
        self.data
    }
}

#[cfg(target_pointer_width = "64")]
pub type ReasonVec = ReasonStore64;
#[cfg(target_pointer_width = "32")]
pub type ReasonVec = ReasonStore32;

#[cfg(target_pointer_width = "64")]
pub type ReasonWithData = ReasonStore64Value;
#[cfg(target_pointer_width = "32")]
pub type ReasonWithData = ReasonStore32Value;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValueSet {
    pub rep: u8,
}

#[allow(non_upper_case_globals)]
impl ValueSet {
    pub const user_value: u32 = 0x03u32;
    pub const saved_value: u32 = 0x0Cu32;
    pub const pref_value: u32 = 0x30u32;
    pub const def_value: u32 = 0xC0u32;

    pub fn sign(&self) -> bool {
        test_any(right_most_bit(self.rep), 0xAAu8)
    }

    pub const fn empty(&self) -> bool {
        self.rep == 0
    }

    pub fn has(&self, value: u32) -> bool {
        test_any(u32::from(self.rep), value)
    }

    pub fn get(&self, value: u32) -> ValT {
        ((u32::from(self.rep) & value) / right_most_bit(value)) as ValT
    }

    pub fn set(&mut self, which: u32, value: ValT) {
        let mask = which as u8;
        store_clear_mask(&mut self.rep, mask);
        store_set_mask(&mut self.rep, value * right_most_bit(mask));
    }

    pub fn save(&mut self, value: ValT) {
        store_clear_mask(&mut self.rep, Self::saved_value as u8);
        store_set_mask(&mut self.rep, value << 2);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Assignment {
    pub trail: LitVec,
    pub front: u32,
    assign_: PodVectorT<u32>,
    reason_: ReasonVec,
    pref_: PodVectorT<ValueSet>,
    elims_: u32,
    units_: u32,
}

impl Assignment {
    const ELIM_MASK: u32 = 0xFFFFFFF0u32;
    const SEEN_MASK_VAR: u32 = 0b1100u32;
    const VALUE_MASK: u32 = 0b0011u32;
    const LEVEL_SHIFT: u32 = 4u32;

    pub fn q_empty(&self) -> bool {
        self.front == size32(&self.trail)
    }

    pub fn q_size(&self) -> u32 {
        size32(&self.trail) - self.front
    }

    pub fn q_pop(&mut self) -> Literal {
        let literal = self.trail[self.front as usize];
        self.front += 1;
        literal
    }

    pub fn q_reset(&mut self) {
        self.front = size32(&self.trail)
    }

    pub fn num_vars(&self) -> u32 {
        size32(&self.assign_)
    }

    pub fn assigned(&self) -> u32 {
        size32(&self.trail)
    }

    pub fn free(&self) -> u32 {
        self.num_vars() - (self.assigned() + self.elims_)
    }

    pub const fn max_level(&self) -> u32 {
        (1u32 << 28) - 2
    }

    pub fn value(&self, var: Var_t) -> ValT {
        (self.assign_[var as usize] & Self::VALUE_MASK) as ValT
    }

    pub fn level(&self, var: Var_t) -> u32 {
        self.assign_[var as usize] >> Self::LEVEL_SHIFT
    }

    pub fn valid(&self, var: Var_t) -> bool {
        !test_mask(self.assign_[var as usize], Self::ELIM_MASK)
    }

    pub fn pref(&self, var: Var_t) -> ValueSet {
        if (var as usize) < self.pref_.len() {
            self.pref_[var as usize]
        } else {
            ValueSet::default()
        }
    }

    pub fn reason(&self, var: Var_t) -> &Antecedent {
        &self.reason_[var as usize]
    }

    pub fn data(&self, var: Var_t) -> u32 {
        self.reason_.data(var)
    }

    pub fn reserve(&mut self, count: u32) {
        self.assign_.reserve(count as usize);
        self.reason_.reserve(count as usize);
    }

    pub fn ensure_var(&mut self, var: Var_t) {
        let needed = var as usize + 1;
        if self.assign_.len() < needed {
            self.resize(var + 1);
        }
    }

    pub fn resize(&mut self, new_vars: u32) {
        self.assign_.resize(new_vars as usize, 0);
        self.reason_.resize(new_vars as usize);
    }

    pub fn truncate_vars(&mut self, new_vars: u32) {
        let new_len = new_vars as usize + 1;
        let old_trail = self.trail.as_slice().to_vec();
        let front_prefix = old_trail[..(self.front as usize).min(old_trail.len())]
            .iter()
            .filter(|lit| lit.var() <= new_vars)
            .count() as u32;
        let units_prefix = old_trail[..(self.units_ as usize).min(old_trail.len())]
            .iter()
            .filter(|lit| lit.var() <= new_vars)
            .count() as u32;
        let kept: Vec<Literal> = old_trail
            .into_iter()
            .filter(|lit| lit.var() <= new_vars)
            .collect();

        self.assign_.truncate(new_len);
        self.reason_.truncate(new_len);
        self.pref_.truncate(new_len);
        self.trail.assign_from_slice(&kept);
        self.front = front_prefix.min(size32(&self.trail));
        self.units_ = units_prefix.min(size32(&self.trail));
        self.elims_ = (1..=new_vars).filter(|&var| !self.valid(var)).count() as u32;
    }

    pub fn add_var(&mut self) -> Var_t {
        self.assign_.push_back(0);
        self.reason_.push_back(Antecedent::new());
        self.num_vars() - 1
    }

    pub fn set_raw_assignment(&mut self, var: Var_t, value: ValT, level: u32) {
        self.ensure_var(var);
        self.assign_[var as usize] = (level << Self::LEVEL_SHIFT) | u32::from(value);
    }

    pub fn push_trail_literal(&mut self, literal: Literal) {
        self.trail.push_back(literal);
    }

    pub fn clear_trail(&mut self) {
        self.trail.clear();
        self.front = 0;
    }

    pub fn request_prefs(&mut self) {
        if self.pref_.len() != self.assign_.len() {
            self.pref_.resize(self.assign_.len(), ValueSet::default());
        }
    }

    pub fn eliminate(&mut self, var: Var_t) {
        assert_eq!(
            self.value(var),
            value_free,
            "can not eliminate assigned var"
        );
        if self.valid(var) {
            self.assign_[var as usize] = Self::ELIM_MASK | u32::from(value_true);
            self.elims_ += 1;
        }
    }

    pub fn assign(&mut self, literal: Literal, level: u32, antecedent: Antecedent) -> bool {
        self.assign_impl(literal, level, antecedent, None)
    }

    pub fn assign_with_data(
        &mut self,
        literal: Literal,
        level: u32,
        antecedent: Antecedent,
        data: u32,
    ) -> bool {
        self.assign_impl(literal, level, antecedent, Some(data))
    }

    pub fn undo_trail(&mut self, first: usize, save: bool) {
        let stop = self.trail[first];
        if save {
            self.request_prefs();
            self.pop_until::<true>(stop);
        } else {
            self.pop_until::<false>(stop);
        }
        self.q_reset();
    }

    pub fn undo_last(&mut self) {
        let var = self.trail.back().var();
        self.clear(var);
        self.trail.pop_back();
    }

    pub fn last(&self) -> Literal {
        *self.trail.back()
    }

    pub fn last_mut(&mut self) -> &mut Literal {
        self.trail.back_mut()
    }

    pub fn units(&self) -> u32 {
        self.units_
    }

    pub fn seen_var(&self, var: Var_t) -> bool {
        test_any(self.assign_[var as usize], Self::SEEN_MASK_VAR)
    }

    pub fn seen_literal(&self, literal: Literal) -> bool {
        test_any(
            self.assign_[literal.var() as usize],
            Self::seen_mask(literal),
        )
    }

    pub fn values(&self, out: &mut ValueVec) {
        out.clear();
        out.reserve(self.assign_.len());
        for value in self.assign_.as_slice() {
            out.push_back((value & Self::VALUE_MASK) as ValT);
        }
    }

    pub fn set_seen_var(&mut self, var: Var_t) {
        store_set_mask(&mut self.assign_[var as usize], Self::SEEN_MASK_VAR);
    }

    pub fn set_seen_literal(&mut self, literal: Literal) {
        store_set_mask(
            &mut self.assign_[literal.var() as usize],
            Self::seen_mask(literal),
        );
    }

    pub fn clear_seen(&mut self, var: Var_t) {
        store_clear_mask(&mut self.assign_[var as usize], Self::SEEN_MASK_VAR);
    }

    pub fn clear_value(&mut self, var: Var_t) {
        store_clear_mask(&mut self.assign_[var as usize], Self::VALUE_MASK);
    }

    pub fn set_value(&mut self, var: Var_t, value: ValT) {
        assert!(self.value(var) == value || self.value(var) == value_free);
        self.assign_[var as usize] |= u32::from(value);
    }

    pub fn set_reason(&mut self, var: Var_t, antecedent: Antecedent) {
        self.reason_[var as usize] = antecedent;
    }

    pub fn set_data(&mut self, var: Var_t, data: u32) {
        self.reason_.set_data(var, data);
    }

    pub fn set_pref(&mut self, var: Var_t, which: u32, value: ValT) {
        self.pref_[var as usize].set(which, value);
    }

    pub fn mark_units(&mut self) -> bool {
        while self.units_ != self.front {
            let var = self.trail[self.units_ as usize].var();
            self.set_seen_var(var);
            self.units_ += 1;
        }
        true
    }

    pub fn set_units(&mut self, units: u32) {
        self.units_ = units;
    }

    pub fn reset_prefs(&mut self) {
        let len = self.pref_.len();
        self.pref_.assign_fill(len, ValueSet::default());
    }

    pub fn clear(&mut self, var: Var_t) {
        self.assign_[var as usize] = 0;
    }

    fn assign_impl(
        &mut self,
        literal: Literal,
        level: u32,
        antecedent: Antecedent,
        data: Option<u32>,
    ) -> bool {
        let var = literal.var();
        let current = self.value(var);
        if current == value_free {
            assert!(self.valid(var));
            self.assign_[var as usize] =
                (level << Self::LEVEL_SHIFT) + u32::from(true_value(literal));
            self.reason_[var as usize] = antecedent;
            if let Some(data) = data {
                self.reason_.set_data(var, data);
            }
            self.trail.push_back(literal);
            true
        } else {
            current == true_value(literal)
        }
    }

    fn pop_until<const SAVE_VALUE: bool>(&mut self, stop: Literal) {
        loop {
            let literal = *self.trail.back();
            self.trail.pop_back();
            let var = literal.var();
            if SAVE_VALUE {
                let value = self.value(var);
                self.pref_[var as usize].save(value);
            }
            self.clear(var);
            if literal == stop {
                break;
            }
        }
    }

    fn seen_mask(literal: Literal) -> u32 {
        u32::from(true_value(literal)) << 2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImpliedLiteral {
    pub lit: Literal,
    pub level: u32,
    pub ante: ReasonWithData,
}

impl ImpliedLiteral {
    pub fn new(lit: Literal, level: u32, antecedent: Antecedent) -> Self {
        Self::with_data(lit, level, antecedent, u32::MAX)
    }

    pub fn with_data(lit: Literal, level: u32, antecedent: Antecedent, data: u32) -> Self {
        Self {
            lit,
            level,
            ante: ReasonWithData::new(antecedent, data),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ImpliedList {
    pub lits: PodVectorT<ImpliedLiteral>,
    pub level: u32,
    pub front: u32,
}

impl ImpliedList {
    pub fn len(&self) -> usize {
        self.lits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lits.is_empty()
    }

    pub fn find(&mut self, literal: Literal) -> Option<&mut ImpliedLiteral> {
        self.lits
            .as_mut_slice()
            .iter_mut()
            .find(|entry| entry.lit == literal)
    }

    pub fn add(&mut self, dl: u32, literal: ImpliedLiteral) {
        if dl > self.level {
            self.level = dl;
        }
        self.lits.push_back(literal);
    }

    pub fn assign(&mut self, solver: &mut Solver) -> bool {
        assert!(self.front as usize <= self.lits.len());
        let dl = solver.decision_level();
        let mut ok = !solver.has_conflict();
        let start = self.front as usize;
        let pending: Vec<ImpliedLiteral> = self.lits.as_slice()[start..].to_vec();
        let mut write = start;

        for implied in pending {
            if implied.level <= dl {
                if ok {
                    ok = solver.force_with_data(
                        implied.lit,
                        implied.ante.ante(),
                        implied.ante.data(),
                    );
                }
                if implied.level < dl || implied.ante.ante().is_null() {
                    self.lits.as_mut_slice()[write] = implied;
                    write += 1;
                }
            }
        }

        self.lits.truncate(write);
        self.level = dl * u32::from(!self.lits.is_empty());
        self.front = if self.level > solver.root_level() {
            self.front
        } else {
            size32(&self.lits)
        };
        ok
    }

    pub fn active(&self, dl: u32) -> bool {
        dl < self.level && self.front as usize != self.lits.len()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, ImpliedLiteral> {
        self.lits.as_slice().iter()
    }
}

impl<'a> IntoIterator for &'a ImpliedList {
    type Item = &'a ImpliedLiteral;
    type IntoIter = core::slice::Iter<'a, ImpliedLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub type SolverSet = crate::potassco::bits::Bitset<u64>;
