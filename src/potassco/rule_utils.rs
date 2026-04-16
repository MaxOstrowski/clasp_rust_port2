//! Rust port of original_clasp/libpotassco/potassco/rule_utils.h and
//! original_clasp/libpotassco/src/rule_utils.cpp.

use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, BodyType, HeadType, Lit, LitSpan, Weight, WeightLit,
    WeightLitSpan,
};
use crate::potassco::enums::{EnumTag, enum_max};
use crate::potassco_check_pre;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sum<'a> {
    pub lits: WeightLitSpan<'a>,
    pub bound: Weight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rule<'a> {
    pub ht: HeadType,
    pub head: AtomSpan<'a>,
    pub bt: BodyType,
    pub cond: LitSpan<'a>,
    pub agg: Sum<'a>,
}

impl Default for Rule<'static> {
    fn default() -> Self {
        Self {
            ht: HeadType::Disjunctive,
            head: &[],
            bt: BodyType::Normal,
            cond: &[],
            agg: Sum::default(),
        }
    }
}

impl<'a> Rule<'a> {
    pub fn normal(ht: HeadType, head: AtomSpan<'a>, body: LitSpan<'a>) -> Self {
        Self {
            ht,
            head,
            bt: BodyType::Normal,
            cond: body,
            agg: Sum::default(),
        }
    }

    pub fn sum(ht: HeadType, head: AtomSpan<'a>, sum: Sum<'a>) -> Self {
        Self {
            ht,
            head,
            bt: BodyType::Sum,
            cond: &[],
            agg: sum,
        }
    }

    pub fn sum_with_bound(
        ht: HeadType,
        head: AtomSpan<'a>,
        bound: Weight,
        lits: WeightLitSpan<'a>,
    ) -> Self {
        Self::sum(ht, head, Sum { lits, bound })
    }

    pub fn is_normal(&self) -> bool {
        self.bt == BodyType::Normal
    }

    pub fn is_sum(&self) -> bool {
        self.bt != BodyType::Normal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PartState {
    Open,
    Started,
    Finished,
}

impl PartState {
    fn started(self) -> bool {
        matches!(self, Self::Started | Self::Finished)
    }

    fn finished(self) -> bool {
        matches!(self, Self::Finished)
    }

    fn open(self) -> bool {
        matches!(self, Self::Open)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeadKind {
    Rule(HeadType),
    Minimize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleBuilder {
    head_kind: HeadKind,
    head_state: PartState,
    head: Vec<Atom>,
    body_kind: BodyType,
    body_state: PartState,
    body: Vec<Lit>,
    sum_lits: Vec<WeightLit>,
    bound: Option<Weight>,
}

impl Default for RuleBuilder {
    fn default() -> Self {
        Self {
            head_kind: HeadKind::Rule(HeadType::Disjunctive),
            head_state: PartState::Open,
            head: Vec::new(),
            body_kind: BodyType::Normal,
            body_state: PartState::Open,
            body: Vec::new(),
            sum_lits: Vec::new(),
            bound: None,
        }
    }
}

impl RuleBuilder {
    fn head_label(is_head: bool) -> &'static str {
        if is_head { "Head" } else { "Body" }
    }

    fn start_part(&mut self, is_head: bool) {
        if !self.frozen() {
            let state = if is_head {
                self.head_state
            } else {
                self.body_state
            };
            potassco_check_pre!(
                state.open(),
                "{} already started",
                Self::head_label(is_head)
            );
            if is_head {
                if !self.body_state.open() {
                    self.body_state = PartState::Finished;
                }
            } else if !self.head_state.open() {
                self.head_state = PartState::Finished;
            }
        } else {
            self.clear();
        }
        if is_head {
            self.head_state = PartState::Started;
        } else {
            self.body_state = PartState::Started;
        }
    }

    fn start_head_with(&mut self, head_type: HeadKind) {
        self.start_part(true);
        self.head_kind = head_type;
        self.head.clear();
    }

    fn start_body_with(&mut self, body_type: BodyType, bound: Option<Weight>) {
        self.start_part(false);
        self.body_kind = body_type;
        self.bound = bound;
        self.body.clear();
        self.sum_lits.clear();
    }

    pub fn swap(&mut self, other: &mut Self) {
        core::mem::swap(self, other);
    }

    pub fn clear(&mut self) -> &mut Self {
        *self = Self::default();
        self
    }

    pub fn frozen(&self) -> bool {
        self.head_state.finished() && self.body_state.finished()
    }

    pub fn is_fact(&self) -> bool {
        self.head_type() == HeadType::Disjunctive
            && self.body_type() == BodyType::Normal
            && self.body.is_empty()
            && self.head.len() == 1
    }

    pub fn start(&mut self) -> &mut Self {
        self.start_head_with(HeadKind::Rule(HeadType::Disjunctive));
        self
    }

    pub fn start_with_type(&mut self, head_type: HeadType) -> &mut Self {
        self.start_head_with(HeadKind::Rule(head_type));
        self
    }

    pub fn start_minimize(&mut self, priority: Weight) -> &mut Self {
        self.start_head_with(HeadKind::Minimize);
        self.start_body_with(BodyType::Sum, Some(priority));
        self
    }

    pub fn start_body(&mut self) -> &mut Self {
        self.start_body_with(BodyType::Normal, None);
        self
    }

    pub fn start_sum(&mut self, bound: Weight) -> &mut Self {
        if !self.is_minimize() || self.frozen() {
            self.start_body_with(BodyType::Sum, Some(bound));
        }
        self
    }

    pub fn set_bound(&mut self, bound: Weight) -> &mut Self {
        potassco_check_pre!(
            self.body_type() != BodyType::Normal && !self.frozen(),
            "Invalid call to setBound"
        );
        self.bound = Some(bound);
        self
    }

    pub fn add_head(&mut self, atom: Atom) -> &mut Self {
        if !self.head_state.started() {
            self.start();
        }
        potassco_check_pre!(!self.head_state.finished(), "Head already frozen");
        self.head.push(atom);
        self
    }

    pub fn add_goal(&mut self, lit: Lit) -> &mut Self {
        if self.body_type() == BodyType::Normal {
            if !self.body_state.started() {
                self.start_body();
            }
            potassco_check_pre!(!self.body_state.finished(), "Body already frozen");
            self.body.push(lit);
        } else {
            self.add_weight_lit(WeightLit { lit, weight: 1 });
        }
        self
    }

    pub fn add_goal_with_weight(&mut self, lit: Lit, weight: Weight) -> &mut Self {
        self.add_weight_lit(WeightLit { lit, weight })
    }

    pub fn add_weight_lit(&mut self, lit: WeightLit) -> &mut Self {
        if self.body_type() == BodyType::Normal {
            if !self.body_state.started() {
                self.start_body();
            }
            potassco_check_pre!(
                lit.weight == 1,
                "non-trivial weight literal not supported in normal body"
            );
            potassco_check_pre!(!self.body_state.finished(), "Body already frozen");
            self.body.push(lit.lit);
        } else {
            if !self.body_state.started() {
                self.start_sum(0);
            }
            potassco_check_pre!(!self.body_state.finished(), "Sum already frozen");
            if lit.weight != 0 {
                self.sum_lits.push(lit);
            }
        }
        self
    }

    pub fn clear_body(&mut self) -> &mut Self {
        self.body_kind = BodyType::Normal;
        self.body_state = PartState::Open;
        self.body.clear();
        self.sum_lits.clear();
        self.bound = None;
        self
    }

    pub fn clear_head(&mut self) -> &mut Self {
        self.head_kind = HeadKind::Rule(HeadType::Disjunctive);
        self.head_state = PartState::Open;
        self.head.clear();
        self
    }

    pub fn weaken(&mut self, to: BodyType, reset_weights: bool) -> &mut Self {
        potassco_check_pre!(!self.is_minimize(), "Invalid call to weaken");
        let current = self.body_type();
        if current != BodyType::Normal && current != to {
            if to == BodyType::Normal {
                self.body = self.sum_lits.iter().map(|lit| lit.lit).collect();
                self.sum_lits.clear();
                self.bound = None;
            } else if to == BodyType::Count && !self.sum_lits.is_empty() && reset_weights {
                let min_weight = self
                    .sum_lits
                    .iter()
                    .map(|lit| lit.weight)
                    .min()
                    .expect("non-empty sum literals have a minimum weight");
                for lit in &mut self.sum_lits {
                    lit.weight = 1;
                }
                let sum_bound = self.bound();
                self.bound = Some((sum_bound + (min_weight - 1)) / min_weight);
            }
            self.body_kind = to;
        }
        self
    }

    pub fn head_type(&self) -> HeadType {
        match self.head_kind {
            HeadKind::Rule(head_type) => head_type,
            HeadKind::Minimize => HeadType::from_underlying(enum_max::<HeadType>() + 1)
                .unwrap_or(HeadType::Disjunctive),
        }
    }

    pub fn head(&self) -> AtomSpan<'_> {
        &self.head
    }

    pub fn is_minimize(&self) -> bool {
        matches!(self.head_kind, HeadKind::Minimize)
    }

    pub fn body_type(&self) -> BodyType {
        self.body_kind
    }

    pub fn body(&self) -> LitSpan<'_> {
        if self.body_kind == BodyType::Normal {
            &self.body
        } else {
            &[]
        }
    }

    pub fn bound(&self) -> Weight {
        self.bound.unwrap_or(-1)
    }

    pub fn sum_lits(&self) -> WeightLitSpan<'_> {
        if self.body_kind == BodyType::Normal {
            &[]
        } else {
            &self.sum_lits
        }
    }

    pub fn find_sum_lit(&self, lit: Lit) -> Option<&WeightLit> {
        self.sum_lits()
            .iter()
            .find(|candidate| candidate.lit == lit)
    }

    pub fn sum(&self) -> Sum<'_> {
        Sum {
            lits: self.sum_lits(),
            bound: self.bound(),
        }
    }

    pub fn rule(&self) -> Rule<'_> {
        let mut ret = Rule {
            ht: self.head_type(),
            head: self.head(),
            bt: self.body_type(),
            cond: &[],
            agg: Sum::default(),
        };
        if ret.bt == BodyType::Normal {
            ret.cond = self.body();
        } else {
            ret.agg = self.sum();
        }
        ret
    }

    pub fn end(&mut self, out: Option<&mut dyn AbstractProgram>) -> &mut Self {
        self.head_state = PartState::Finished;
        self.body_state = PartState::Finished;
        if let Some(out) = out {
            if self.body_type() == BodyType::Normal {
                out.rule(self.head_type(), self.head(), self.body());
            } else {
                let sum = self.sum();
                if self.is_minimize() {
                    out.minimize(sum.bound, sum.lits);
                } else {
                    out.rule_weighted(self.head_type(), self.head(), sum.bound, sum.lits);
                }
            }
        }
        self
    }
}
