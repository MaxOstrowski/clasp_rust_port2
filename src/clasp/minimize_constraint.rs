//! Partial Rust port of `original_clasp/clasp/minimize_constraint.h`
//! and the solver-independent parts of `original_clasp/src/minimize_constraint.cpp`.

use crate::clasp::literal::{
    Literal, WeightLiteral, WeightT, Wsum_t, is_sentinel, lit_true, value_free, weight_max,
    weight_min, weight_sum_max,
};
use crate::clasp::mt::{ThreadSafe, memory_order_relaxed};
use crate::clasp::shared_context::SharedContext;
use crate::clasp::solver::Solver;
use crate::clasp::solver_strategies::LowerBound;
use crate::clasp::util::misc_types::RefCount;
use std::cmp::Ordering;
use std::collections::HashMap;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinimizeMode {
    Ignore = 0,
    Optimize = 1,
    Enumerate = 2,
    EnumOpt = 3,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LevelWeight {
    pub level: u32,
    pub next: bool,
    pub weight: WeightT,
}

impl LevelWeight {
    pub const fn new(level: u32, weight: WeightT, next: bool) -> Self {
        Self {
            level,
            next,
            weight,
        }
    }
}

#[derive(Debug)]
pub struct SharedMinimizeData {
    adjust: Vec<Wsum_t>,
    up: [Vec<Wsum_t>; 2],
    lower: Vec<ThreadSafe<Wsum_t>>,
    low_pos: ThreadSafe<u32>,
    mode: MinimizeMode,
    count: RefCount,
    g_count: ThreadSafe<u32>,
    opt_gen: u32,
    pub weights: Vec<LevelWeight>,
    pub prios: Vec<WeightT>,
    lits: Vec<WeightLiteral>,
}

impl SharedMinimizeData {
    pub fn from_parts(
        adjust: Vec<Wsum_t>,
        weights: Vec<LevelWeight>,
        prios: Vec<WeightT>,
        lits: Vec<WeightLiteral>,
        mode: MinimizeMode,
    ) -> Self {
        let sparse = !weights.is_empty();
        Self::from_parts_impl(adjust, weights, prios, lits, mode, sparse)
    }

    pub(crate) fn from_builder_parts(
        adjust: Vec<Wsum_t>,
        weights: Vec<LevelWeight>,
        prios: Vec<WeightT>,
        lits: Vec<WeightLiteral>,
        mode: MinimizeMode,
        sparse: bool,
    ) -> Self {
        Self::from_parts_impl(adjust, weights, prios, lits, mode, sparse)
    }

    fn from_parts_impl(
        adjust: Vec<Wsum_t>,
        mut weights: Vec<LevelWeight>,
        prios: Vec<WeightT>,
        mut lits: Vec<WeightLiteral>,
        mode: MinimizeMode,
        sparse: bool,
    ) -> Self {
        let num_rules = adjust.len();
        let sentinel_weight = if sparse {
            let sentinel_weight = weights.len() as WeightT;
            weights.push(LevelWeight::new(
                num_rules.saturating_sub(1) as u32,
                0,
                false,
            ));
            sentinel_weight
        } else {
            0
        };
        if let Some(last) = lits.last_mut().filter(|last| is_sentinel(last.lit)) {
            last.weight = sentinel_weight;
        } else {
            lits.push(WeightLiteral {
                lit: lit_true,
                weight: sentinel_weight,
            });
        }
        let mut data = Self {
            adjust,
            up: [
                vec![Self::max_bound(); num_rules],
                vec![Self::max_bound(); num_rules],
            ],
            lower: (0..num_rules).map(|_| ThreadSafe::new(0)).collect(),
            low_pos: ThreadSafe::new(num_rules as u32),
            mode,
            count: RefCount::new(1),
            g_count: ThreadSafe::new(0),
            opt_gen: 0,
            weights,
            prios,
            lits,
        };
        data.reset_bounds();
        let _ = data.set_mode(MinimizeMode::Optimize, &[]);
        data
    }

    pub const fn max_bound() -> Wsum_t {
        weight_sum_max
    }

    pub fn share(&self) -> &Self {
        self.count.add(1);
        self
    }

    pub fn release(&self) -> bool {
        self.count.release(1)
    }

    pub fn num_rules(&self) -> u32 {
        self.adjust.len() as u32
    }

    pub fn mode(&self) -> MinimizeMode {
        self.mode
    }

    pub fn generation(&self) -> u32 {
        self.g_count.load(memory_order_relaxed)
    }

    pub fn check_next(&self) -> bool {
        self.mode != MinimizeMode::Enumerate && self.generation() != self.opt_gen
    }

    pub fn optimize(&self) -> bool {
        if self.opt_gen != 0 {
            self.check_next()
        } else {
            self.mode != MinimizeMode::Enumerate
        }
    }

    pub fn adjust(&self, level: u32) -> Wsum_t {
        self.adjust[level as usize]
    }

    pub fn adjust_slice(&self) -> &[Wsum_t] {
        &self.adjust
    }

    pub fn lower(&self, level: u32) -> Wsum_t {
        self.lower[level as usize].load(memory_order_relaxed)
    }

    pub fn upper(&self, level: u32) -> Wsum_t {
        self.upper_slice()[level as usize]
    }

    pub fn upper_slice(&self) -> &[Wsum_t] {
        &self.up[(self.generation() & 1) as usize]
    }

    pub fn sum(&self, level: u32) -> Wsum_t {
        self.sum_slice()[level as usize]
    }

    pub fn sum_slice(&self) -> &[Wsum_t] {
        if self.mode != MinimizeMode::Enumerate {
            self.upper_slice()
        } else {
            &self.up[1]
        }
    }

    pub fn optimum(&self, level: u32) -> Wsum_t {
        let value = self.sum(level);
        value
            + if value != Self::max_bound() {
                self.adjust(level)
            } else {
                0
            }
    }

    pub fn lower_bound(&self) -> LowerBound {
        let level = self.low_pos.load(memory_order_relaxed);
        if level < self.num_rules() {
            return LowerBound {
                level,
                bound: self.lower(level) + self.adjust(level),
            };
        }
        LowerBound::default()
    }

    pub fn level(&self, index: u32) -> u32 {
        if self.num_rules() == 1 {
            0
        } else {
            self.level_weight(self.lits[index as usize].weight).level
        }
    }

    pub fn weight(&self, index: u32) -> WeightT {
        if self.num_rules() == 1 {
            self.lits[index as usize].weight
        } else {
            self.level_weight(self.lits[index as usize].weight).weight
        }
    }

    pub fn weight_at_level(&self, lit: WeightLiteral, level: u32) -> WeightT {
        if self.num_rules() == 1 {
            return lit.weight * i32::from(level == 0);
        }
        let mut position = lit.weight as usize;
        loop {
            let weight = &self.weights[position];
            if weight.level == level {
                return weight.weight;
            }
            if !weight.next {
                return 0;
            }
            position += 1;
        }
    }

    pub fn literals(&self) -> &[WeightLiteral] {
        &self.lits
    }

    pub fn begin(&self) -> SharedMinimizeIter<'_> {
        self.iter()
    }

    pub fn end(&self) -> SharedMinimizeIter<'_> {
        SharedMinimizeIter {
            data: &self.lits,
            index: self.lits.len(),
        }
    }

    pub fn iter(&self) -> SharedMinimizeIter<'_> {
        SharedMinimizeIter {
            data: &self.lits,
            index: 0,
        }
    }

    pub fn set_mode(&mut self, mode: MinimizeMode, bound: &[Wsum_t]) -> bool {
        self.mode = mode;
        if !bound.is_empty() {
            self.g_count.store(0, memory_order_relaxed);
            self.opt_gen = 0;
            let bound_size = bound.len().min(self.num_rules() as usize);
            for (idx, bound_value) in bound.iter().copied().enumerate().take(bound_size) {
                let adjust = self.adjust[idx];
                let value = if adjust >= 0 || (Self::max_bound() + adjust) >= bound_value {
                    bound_value - adjust
                } else {
                    Self::max_bound()
                };
                if value - self.lower(idx as u32) < 0 {
                    return false;
                }
                self.up[0][idx] = value;
            }
            for value in &mut self.up[0][bound_size..] {
                *value = Self::max_bound();
            }
        }
        true
    }

    pub fn reset_bounds(&mut self) {
        self.g_count.store(0, memory_order_relaxed);
        self.opt_gen = 0;
        for lower in &self.lower {
            lower.store(0, memory_order_relaxed);
        }
        self.low_pos.store(self.num_rules(), memory_order_relaxed);
        for upper in &mut self.up {
            upper.fill(Self::max_bound());
        }
        if self.weights.is_empty() {
            return;
        }
        let mut lit_index = 0usize;
        let mut w_pos = 0usize;
        while w_pos < self.weights.len() {
            debug_assert!(self.weights[w_pos].weight >= 0);
            let mut cursor = w_pos;
            let mut num_lits = 0i64;
            while self.weights[cursor].next {
                cursor += 1;
                if self.weights[cursor].weight < 0 {
                    if num_lits == 0 {
                        while lit_index < self.lits.len()
                            && (self.lits[lit_index].weight as usize) <= w_pos
                        {
                            if (self.lits[lit_index].weight as usize) == w_pos {
                                num_lits += 1;
                            }
                            lit_index += 1;
                        }
                    }
                    let level = self.weights[cursor].level as usize;
                    let delta = i64::from(self.weights[cursor].weight) * num_lits;
                    let current = self.lower[level].load(memory_order_relaxed);
                    self.lower[level].store(current + delta, memory_order_relaxed);
                }
            }
            w_pos = cursor + 1;
        }
    }

    pub fn set_optimum(&mut self, new_opt: &[Wsum_t]) -> &[Wsum_t] {
        if self.opt_gen != 0 {
            return &self.up[(self.opt_gen & 1) as usize];
        }
        let mut generation = self.g_count.load(memory_order_relaxed);
        let next = (1 - (generation & 1)) as usize;
        self.up[next].clear();
        self.up[next].extend_from_slice(new_opt);
        if self.mode != MinimizeMode::Enumerate {
            generation = generation.wrapping_add(1);
            if generation == 0 {
                generation = 2;
            }
            self.g_count.store(generation, memory_order_relaxed);
        }
        &self.up[next]
    }

    pub fn set_lower(&self, level: u32, lower: Wsum_t) {
        self.lower[level as usize].store(lower, memory_order_relaxed);
    }

    pub fn inc_lower(&self, level: u32, lower: Wsum_t) -> Wsum_t {
        let slot = &self.lower[level as usize];
        let mut stored = slot.load(memory_order_relaxed);
        loop {
            if stored >= lower {
                return stored;
            }
            let mut expected = stored;
            if slot.compare_exchange_weak(&mut expected, lower, memory_order_relaxed) {
                let mut stored_level = self.low_pos.load(memory_order_relaxed);
                while stored_level == self.num_rules() || level > stored_level {
                    let mut expected_level = stored_level;
                    if self.low_pos.compare_exchange_weak(
                        &mut expected_level,
                        level,
                        memory_order_relaxed,
                    ) {
                        break;
                    }
                    stored_level = expected_level;
                }
                return lower;
            }
            stored = expected;
        }
    }

    pub fn mark_optimal(&mut self) {
        self.opt_gen = self.generation();
    }

    pub fn add(&self, lhs: &mut [Wsum_t], lit: WeightLiteral) {
        self.add_weight(lhs, lit);
    }

    pub fn add_weight(&self, lhs: &mut [Wsum_t], lit: WeightLiteral) {
        if self.weights.is_empty() {
            lhs[0] += i64::from(lit.weight);
        } else {
            self.add_level_weight(lhs, lit.weight as usize);
        }
    }

    pub fn add_level_weight(&self, lhs: &mut [Wsum_t], mut position: usize) {
        loop {
            let weight = &self.weights[position];
            lhs[weight.level as usize] += i64::from(weight.weight);
            if !weight.next {
                return;
            }
            position += 1;
        }
    }

    pub fn sub(&self, lhs: &mut [Wsum_t], lit: WeightLiteral, active_level: &mut u32) {
        self.sub_weight(lhs, lit, active_level);
    }

    pub fn sub_weight(&self, lhs: &mut [Wsum_t], lit: WeightLiteral, active_level: &mut u32) {
        if self.weights.is_empty() {
            lhs[0] -= i64::from(lit.weight);
        } else {
            self.sub_level_weight(lhs, lit.weight as usize, active_level);
        }
    }

    pub fn sub_level_weight(
        &self,
        lhs: &mut [Wsum_t],
        mut position: usize,
        active_level: &mut u32,
    ) {
        let first = &self.weights[position];
        if first.level < *active_level {
            *active_level = first.level;
        }
        loop {
            let weight = &self.weights[position];
            lhs[weight.level as usize] -= i64::from(weight.weight);
            if !weight.next {
                return;
            }
            position += 1;
        }
    }

    pub fn implies_weight(
        &self,
        lhs: &mut [Wsum_t],
        lit: WeightLiteral,
        rhs: &[Wsum_t],
        level: &mut u32,
    ) -> bool {
        if self.weights.is_empty() {
            lhs[0] + i64::from(lit.weight) > rhs[0]
        } else {
            self.imp_level_weight(lhs, lit.weight as usize, rhs, level)
        }
    }

    pub fn imp(
        &self,
        lhs: &mut [Wsum_t],
        lit: WeightLiteral,
        rhs: &[Wsum_t],
        level: &mut u32,
    ) -> bool {
        self.implies_weight(lhs, lit, rhs, level)
    }

    pub fn imp_level_weight(
        &self,
        lhs: &mut [Wsum_t],
        position: usize,
        rhs: &[Wsum_t],
        level: &mut u32,
    ) -> bool {
        let weight = &self.weights[position];
        debug_assert!(*level <= weight.level);
        while *level != weight.level && lhs[*level as usize] == rhs[*level as usize] {
            *level += 1;
        }
        let mut next_position = Some(position);
        for idx in *level as usize..self.num_rules() as usize {
            let mut temp = lhs[idx];
            if let Some(current_pos) =
                next_position.filter(|current| self.weights[*current].level as usize == idx)
            {
                let current = &self.weights[current_pos];
                temp += i64::from(current.weight);
                next_position = if current.next {
                    Some(current_pos + 1)
                } else {
                    None
                };
            }
            if temp != rhs[idx] {
                return temp > rhs[idx];
            }
        }
        false
    }

    fn level_weight(&self, position: WeightT) -> &LevelWeight {
        &self.weights[position as usize]
    }
}

pub struct SharedMinimizeIter<'a> {
    data: &'a [WeightLiteral],
    index: usize,
}

impl<'a> Iterator for SharedMinimizeIter<'a> {
    type Item = &'a WeightLiteral;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.data.len() || is_sentinel(self.data[self.index].lit) {
            return None;
        }
        let item = &self.data[self.index];
        self.index += 1;
        Some(item)
    }
}

impl<'a> IntoIterator for &'a SharedMinimizeData {
    type Item = &'a WeightLiteral;
    type IntoIter = SharedMinimizeIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MLit {
    lit: Literal,
    prio: WeightT,
    weight: WeightT,
}

#[derive(Debug, Default)]
pub struct MinimizeBuilder {
    lits: Vec<MLit>,
}

impl MinimizeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.lits.clear();
    }

    pub fn empty(&self) -> bool {
        self.lits.is_empty()
    }

    pub fn add_literal(&mut self, prio: WeightT, lit: WeightLiteral) -> &mut Self {
        self.lits.push(MLit {
            lit: lit.lit,
            prio,
            weight: lit.weight,
        });
        self
    }

    pub fn add_adjust(&mut self, prio: WeightT, adjust: WeightT) -> &mut Self {
        self.add_literal(
            prio,
            WeightLiteral {
                lit: lit_true,
                weight: adjust,
            },
        )
    }

    pub fn add_shared(&mut self, con: &SharedMinimizeData) -> &mut Self {
        if con.num_rules() == 1 {
            let prio = con.prios.first().copied().unwrap_or(0);
            for &lit in con {
                self.add_literal(prio, lit);
            }
        } else {
            for &lit in con {
                let mut weight = lit.weight as usize;
                loop {
                    let level_weight = &con.weights[weight];
                    let prio = con
                        .prios
                        .get(level_weight.level as usize)
                        .copied()
                        .unwrap_or(-(level_weight.level as WeightT));
                    self.add_literal(
                        prio,
                        WeightLiteral {
                            lit: lit.lit,
                            weight: level_weight.weight,
                        },
                    );
                    if !level_weight.next {
                        break;
                    }
                    weight += 1;
                }
            }
        }
        for (index, &adjust) in con.adjust_slice().iter().enumerate() {
            if adjust == 0 {
                continue;
            }
            let prio = con.prios.get(index).copied().unwrap_or(-(index as WeightT));
            let mut remaining = adjust;
            while remaining < i64::from(weight_min) {
                self.add_adjust(prio, weight_min);
                remaining -= i64::from(weight_min);
            }
            while remaining > i64::from(weight_max) {
                self.add_adjust(prio, weight_max);
                remaining -= i64::from(weight_max);
            }
            self.add_adjust(prio, remaining as WeightT);
        }
        self
    }

    pub fn build(&mut self, ctx: &mut SharedContext) -> Option<Box<SharedMinimizeData>> {
        assert!(
            !ctx.frozen(),
            "cannot build minimize data for a frozen context"
        );
        let ok = {
            if !ctx.ok() {
                false
            } else {
                let num_vars = ctx.num_vars();
                let solver = ctx.master();
                solver.acquire_problem_var(num_vars);
                solver.propagate()
            }
        };
        if !ok || self.empty() {
            self.clear();
            return None;
        }

        let mut prios = Vec::new();
        let mut adjust = Vec::new();
        let mut weights = Vec::new();
        self.prepare_levels(ctx.master_ref(), &mut adjust, &mut prios);
        if prios.len() > 1 {
            self.merge_levels(&mut adjust, &mut weights);
        } else if prios.is_empty() {
            prios.push(0);
            adjust.push(0);
        }
        let sparse = prios.len() > 1;
        let product = self.create_shared(ctx, adjust, prios, sparse.then_some(weights));
        self.clear();
        Some(Box::new(product))
    }

    fn prepare_levels(
        &mut self,
        solver: &Solver,
        adjust_out: &mut Vec<Wsum_t>,
        prios_out: &mut Vec<WeightT>,
    ) {
        self.lits.sort_by(|lhs, rhs| {
            rhs.prio
                .cmp(&lhs.prio)
                .then_with(|| lhs.lit.cmp(&rhs.lit))
                .then_with(|| rhs.weight.cmp(&lhs.weight))
        });
        prios_out.clear();
        adjust_out.clear();

        let mut out = Vec::with_capacity(self.lits.len());
        let mut index = 0usize;
        while index < self.lits.len() {
            let prio = self.lits[index].prio;
            let mut removed = 0i64;
            while index < self.lits.len() && self.lits[index].prio == prio {
                let mut lit = self.lits[index].lit;
                let mut weight = i64::from(self.lits[index].weight);
                let mut next = index + 1;
                while next < self.lits.len()
                    && self.lits[next].prio == prio
                    && self.lits[next].lit.var() == lit.var()
                {
                    if self.lits[next].lit == lit {
                        weight += i64::from(self.lits[next].weight);
                    } else {
                        weight -= i64::from(self.lits[next].weight);
                        removed += i64::from(self.lits[next].weight);
                    }
                    next += 1;
                }
                if weight < 0 {
                    removed += weight;
                    lit = !lit;
                    weight = -weight;
                }
                if weight != 0 && solver.value(lit.var()) == value_free {
                    assert!(
                        (i64::from(weight_min)..=i64::from(weight_max)).contains(&weight),
                        "MinimizeBuilder: weight too large"
                    );
                    out.push(MLit {
                        lit,
                        prio: prios_out.len() as WeightT,
                        weight: weight as WeightT,
                    });
                } else if solver.is_true(lit) {
                    removed += weight;
                }
                index = next;
            }
            prios_out.push(prio);
            adjust_out.push(removed);
        }
        self.lits = out;
    }

    fn merge_levels(&mut self, adjust: &mut [Wsum_t], weights_out: &mut Vec<LevelWeight>) {
        self.lits.sort_by(|lhs, rhs| {
            lhs.lit
                .var()
                .cmp(&rhs.lit.var())
                .then_with(|| lhs.prio.cmp(&rhs.prio))
                .then_with(|| rhs.weight.cmp(&lhs.weight))
        });
        let mut out = Vec::with_capacity(self.lits.len());
        weights_out.clear();
        let mut index = 0usize;
        while index < self.lits.len() {
            let first = self.lits[index];
            let weight_pos = weights_out.len() as WeightT;
            weights_out.push(LevelWeight::new(first.prio as u32, first.weight, false));
            index += 1;
            while index < self.lits.len() && self.lits[index].lit.var() == first.lit.var() {
                let current = self.lits[index];
                let current_level = current.prio as u32;
                if let Some(last) = weights_out.last_mut() {
                    last.next = true;
                }
                let mut weight = current.weight;
                if current.lit.sign() != first.lit.sign() {
                    adjust[current_level as usize] += i64::from(current.weight);
                    weight = -current.weight;
                }
                weights_out.push(LevelWeight::new(current_level, weight, false));
                index += 1;
            }
            out.push(MLit {
                lit: first.lit,
                prio: first.prio,
                weight: weight_pos,
            });
        }
        self.lits = out;
    }

    fn create_shared(
        &mut self,
        ctx: &mut SharedContext,
        adjust: Vec<Wsum_t>,
        prios: Vec<WeightT>,
        weights: Option<Vec<LevelWeight>>,
    ) -> SharedMinimizeData {
        self.lits
            .sort_by(|lhs, rhs| Self::compare_weight(weights.as_deref(), lhs, rhs));
        let mut out_lits = Vec::with_capacity(self.lits.len());
        let mut out_weights = Vec::new();
        let mut index_by_chain: HashMap<Vec<LevelWeight>, WeightT> = HashMap::new();
        for lit in &self.lits {
            ctx.set_frozen(lit.lit.var(), true);
            let weight = if let Some(weight_table) = weights.as_deref() {
                let chain = Self::collect_chain(weight_table, lit.weight as usize);
                if let Some(&index) = index_by_chain.get(&chain) {
                    index
                } else {
                    let index = out_weights.len() as WeightT;
                    out_weights.extend(chain.iter().copied());
                    index_by_chain.insert(chain, index);
                    index
                }
            } else {
                lit.weight
            };
            out_lits.push(WeightLiteral {
                lit: lit.lit,
                weight,
            });
        }
        SharedMinimizeData::from_builder_parts(
            adjust,
            out_weights,
            prios,
            out_lits,
            MinimizeMode::Optimize,
            weights.is_some(),
        )
    }

    fn collect_chain(weights: &[LevelWeight], start: usize) -> Vec<LevelWeight> {
        let mut chain = Vec::new();
        let mut index = start;
        loop {
            let weight = weights[index];
            chain.push(weight);
            if !weight.next {
                break;
            }
            index += 1;
        }
        chain
    }

    fn compare_weight(weights: Option<&[LevelWeight]>, lhs: &MLit, rhs: &MLit) -> Ordering {
        let left = Self::comes_before(weights, lhs, rhs);
        let right = Self::comes_before(weights, rhs, lhs);
        match (left, right) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }

    fn comes_before(weights: Option<&[LevelWeight]>, lhs: &MLit, rhs: &MLit) -> bool {
        let Some(weights) = weights else {
            return lhs.weight > rhs.weight;
        };
        let mut lhs_pos = lhs.weight as usize;
        let mut rhs_pos = rhs.weight as usize;
        loop {
            let lhs_weight = &weights[lhs_pos];
            let rhs_weight = &weights[rhs_pos];
            if lhs_weight.level != rhs_weight.level {
                return if lhs_weight.level < rhs_weight.level {
                    lhs_weight.weight > 0
                } else {
                    rhs_weight.weight < 0
                };
            }
            if lhs_weight.weight != rhs_weight.weight {
                return lhs_weight.weight > rhs_weight.weight;
            }
            if !lhs_weight.next {
                return rhs_weight.next && weights[rhs_pos + 1].weight < 0;
            }
            if !rhs_weight.next {
                lhs_pos += 1;
                break;
            }
            lhs_pos += 1;
            rhs_pos += 1;
        }
        weights[lhs_pos].weight > 0
    }
}
