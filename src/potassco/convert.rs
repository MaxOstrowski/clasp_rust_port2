//! Rust port of original_clasp/libpotassco/potassco/convert.h and
//! original_clasp/libpotassco/src/convert.cpp.

use std::collections::HashMap;

use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, DomModifier, HeadType, Id, Lit, LitSpan, TruthValue, Weight,
    WeightLit, WeightLitSpan, atom, lit, neg, weight,
};
use crate::potassco::enums::enum_name;
use crate::potassco::rule_utils::RuleBuilder;
use crate::potassco_check_pre;

const FALSE_ATOM: Atom = 1;

#[derive(Clone, Debug, Default)]
struct AtomData {
    sm_id: Atom,
    head: bool,
    show: bool,
    external: Option<TruthValue>,
}

impl AtomData {
    fn atom_pred(&self) -> String {
        format!("_atom({})", self.sm_id)
    }

    fn sm(&self) -> Atom {
        self.sm_id
    }
}

#[derive(Clone, Debug)]
struct HeuristicData {
    atom: Atom,
    modifier: DomModifier,
    bias: i32,
    priority: u32,
    cond: Atom,
}

impl HeuristicData {
    fn pred_name(&self, atom_name: &str) -> String {
        format!(
            "_heuristic({},{},{},{})",
            atom_name,
            enum_name(self.modifier),
            self.bias,
            self.priority
        )
    }
}

#[derive(Clone, Debug)]
struct OutTerm {
    atom: Atom,
    last: Atom,
    name: String,
}

impl OutTerm {
    fn new(name: &str) -> Self {
        Self {
            atom: 0,
            last: 0,
            name: format!("_show_term({name})"),
        }
    }

    fn step(&mut self) {
        if self.atom != 0 {
            self.last = self.atom;
            self.atom = 0;
        }
    }
}

#[derive(Clone, Debug)]
enum OutputKind {
    Name(String),
    Edge(i32, i32),
}

#[derive(Clone, Debug)]
struct OutputData {
    atom: Atom,
    kind: OutputKind,
}

impl OutputData {
    fn pred_name(&self) -> String {
        match &self.kind {
            OutputKind::Name(name) => name.clone(),
            OutputKind::Edge(source, target) => format!("_edge({source},{target})"),
        }
    }
}

#[derive(Clone, Debug)]
struct MinimizeData {
    priority: Weight,
    start_pos: usize,
    end_pos: usize,
}

#[derive(Default)]
struct SmData {
    atoms: Vec<AtomData>,
    sym_tab: HashMap<Atom, String>,
    term_tab: HashMap<Id, OutTerm>,
    external: Vec<Atom>,
    heuristic: Vec<HeuristicData>,
    minimize: Vec<MinimizeData>,
    min_lits: Vec<WeightLit>,
    output: Vec<OutputData>,
    rule: RuleBuilder,
    next: Atom,
}

impl SmData {
    fn new() -> Self {
        Self {
            next: FALSE_ATOM + 1,
            ..Self::default()
        }
    }

    fn new_atom(&mut self) -> Atom {
        let id = self.next;
        self.next += 1;
        id
    }

    fn mapped(&self, atom_id: Atom) -> bool {
        self.atoms
            .get(atom_id as usize)
            .is_some_and(|data| data.sm_id != 0)
    }

    fn map_atom(&mut self, atom_id: Atom) -> &mut AtomData {
        let index = atom_id as usize;
        if index >= self.atoms.len() {
            self.atoms.resize(index + 1, AtomData::default());
        }
        if self.atoms[index].sm_id == 0 {
            self.atoms[index].sm_id = self.new_atom();
        }
        &mut self.atoms[index]
    }

    fn map_lit(&mut self, value: Lit) -> Lit {
        let mapped = lit(self.map_atom(atom(value)).sm());
        if value < 0 { -mapped } else { mapped }
    }

    fn map_weight_lit(&mut self, value: WeightLit) -> WeightLit {
        WeightLit {
            lit: self.map_lit(value.lit),
            weight: value.weight,
        }
    }

    fn map_head_atom(&mut self, atom_id: Atom) -> Atom {
        let data = self.map_atom(atom_id);
        data.head = true;
        data.sm()
    }

    fn map_head(&mut self, head: AtomSpan<'_>, head_type: HeadType) -> &mut RuleBuilder {
        self.rule.clear().start_with_type(head_type);
        for &atom_id in head {
            let mapped = self.map_head_atom(atom_id);
            self.rule.add_head(mapped);
        }
        if head.is_empty() {
            self.rule.add_head(FALSE_ATOM);
        }
        &mut self.rule
    }

    fn map_body_lits(&mut self, body: LitSpan<'_>) -> &mut RuleBuilder {
        for &value in body {
            let mapped = self.map_lit(value);
            self.rule.add_goal(mapped);
        }
        &mut self.rule
    }

    fn map_body_weighted(&mut self, body: WeightLitSpan<'_>) -> &mut RuleBuilder {
        for &value in body {
            let mapped = self.map_weight_lit(value);
            self.rule.add_weight_lit(mapped);
        }
        &mut self.rule
    }

    fn add_output(&mut self, atom_id: Atom, name: String) {
        match self.sym_tab.entry(atom_id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(name.clone());
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                potassco_check_pre!(
                    entry.get() == &name,
                    "Redefinition: atom '{}' already shown as '{}'",
                    atom_id,
                    entry.get()
                );
                return;
            }
        }
        self.output.push(OutputData {
            atom: atom_id,
            kind: OutputKind::Name(name),
        });
    }

    fn add_term(&mut self, term_id: Id, name: &str) {
        match self.term_tab.entry(term_id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(OutTerm::new(name));
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                potassco_check_pre!(
                    entry.get().name == format!("_show_term({name})"),
                    "Redefinition: term '{}' already defined as '{}'",
                    term_id,
                    entry.get().name
                );
            }
        }
    }

    fn add_minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        if self
            .minimize
            .last()
            .is_none_or(|entry| entry.priority != priority)
        {
            self.minimize.push(MinimizeData {
                priority,
                start_pos: self.min_lits.len(),
                end_pos: self.min_lits.len(),
            });
        }
        let entry = self.minimize.last_mut().expect("minimize entry exists");
        for &weighted_lit in lits {
            let mut mapped = weighted_lit;
            if weight(mapped) < 0 {
                mapped.lit = -mapped.lit;
                mapped.weight = -mapped.weight;
            }
            self.min_lits.push(mapped);
        }
        entry.end_pos = self.min_lits.len();
    }

    fn add_external(&mut self, atom_id: Atom, value: TruthValue) {
        let mapped = self.map_atom(atom_id);
        if !mapped.head {
            mapped.external = Some(value);
            self.external.push(atom_id);
        }
    }

    fn add_heuristic(
        &mut self,
        atom_id: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        cond: Atom,
    ) {
        self.heuristic.push(HeuristicData {
            atom: atom_id,
            modifier,
            bias,
            priority,
            cond,
        });
    }

    fn flush_step(&mut self) {
        self.minimize.clear();
        self.min_lits.clear();
        self.external.clear();
        self.heuristic.clear();
        self.output.clear();
    }
}

pub struct SmodelsConvert<'a> {
    out: &'a mut dyn AbstractProgram,
    data: SmData,
    ext: bool,
}

impl<'a> SmodelsConvert<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn AbstractProgram, enable_clasp_ext: bool) -> Self {
        Self {
            out,
            data: SmData::new(),
            ext: enable_clasp_ext,
        }
    }

    pub fn get(&mut self, input: Lit) -> Lit {
        self.data.map_lit(input)
    }

    #[must_use]
    pub fn max_atom(&self) -> u32 {
        self.data.next - 1
    }

    fn make_atom(&mut self, lits: LitSpan<'_>, last: Lit, named: bool) -> Atom {
        let size = lits.len() + usize::from(last != 0);
        let front = lits.first().copied().unwrap_or(last);
        if size != 1 || front <= 0 || (named && self.data.map_atom(atom(front)).show) {
            let id = self.data.new_atom();
            self.data.rule.clear().add_head(id);
            self.data.map_body_lits(lits);
            if last != 0 {
                self.data.rule.add_goal(last);
            }
            self.data.rule.end(Some(self.out));
            id
        } else {
            let mapped = self.data.map_atom(atom(front));
            mapped.show = named;
            mapped.sm()
        }
    }

    fn flush(&mut self) {
        self.flush_minimize();
        self.flush_external();
        self.flush_heuristic();
        self.flush_symbols();
        let false_lit = -lit(FALSE_ATOM);
        self.out.assume(&[false_lit]);
        self.data.flush_step();
    }

    fn flush_minimize(&mut self) {
        if self.data.minimize.is_empty() {
            return;
        }
        self.data
            .minimize
            .sort_by_key(|entry| (entry.priority, entry.start_pos));
        let mut current_priority = None;
        for entry in self.data.minimize.clone() {
            if current_priority != Some(entry.priority) {
                if current_priority.is_some() {
                    self.data.rule.end(Some(self.out));
                }
                self.data.rule.clear().start_minimize(entry.priority);
                current_priority = Some(entry.priority);
            }
            let slice = self.data.min_lits[entry.start_pos..entry.end_pos].to_vec();
            self.data.map_body_weighted(&slice);
        }
        self.data.rule.end(Some(self.out));
    }

    fn flush_external(&mut self) {
        self.data.rule.clear();
        for atom_id in self.data.external.clone() {
            let mapped = self.data.map_atom(atom_id).clone();
            let value = mapped.external.unwrap_or(TruthValue::Free);
            if !self.ext {
                if mapped.head {
                    continue;
                }
                let out_atom = mapped.sm();
                if value == TruthValue::Free {
                    self.data.rule.add_head(out_atom);
                } else if value == TruthValue::True {
                    self.out.rule(HeadType::Disjunctive, &[out_atom], &[]);
                }
            } else {
                self.out.external(mapped.sm(), value);
            }
        }
        if !self.data.rule.head().is_empty() {
            let head = self.data.rule.head().to_vec();
            self.out.rule(HeadType::Choice, &head, &[]);
        }
    }

    fn flush_heuristic(&mut self) {
        for heuristic in self.data.heuristic.clone() {
            if !self.data.mapped(heuristic.atom) {
                continue;
            }
            let mapped_atom = {
                let mapped = self.data.map_atom(heuristic.atom);
                mapped.sm()
            };
            let existing_name = self.data.sym_tab.get(&mapped_atom).cloned();
            let atom_name = if let Some(name) = existing_name {
                name
            } else {
                let atom_name = {
                    let mapped = self.data.map_atom(heuristic.atom);
                    mapped.show = true;
                    mapped.atom_pred()
                };
                self.data.add_output(mapped_atom, atom_name.clone());
                atom_name
            };
            let pred = heuristic.pred_name(&atom_name);
            self.out.output_atom(heuristic.cond, &pred);
        }
    }

    fn flush_symbols(&mut self) {
        self.data.output.sort_by_key(|entry| entry.atom);
        for output in self.data.output.clone() {
            self.out.output_atom(output.atom, &output.pred_name());
        }
    }
}

impl AbstractProgram for SmodelsConvert<'_> {
    fn init_program(&mut self, incremental: bool) {
        self.out.init_program(incremental);
    }

    fn begin_step(&mut self) {
        self.out.begin_step();
        for term in self.data.term_tab.values_mut() {
            term.step();
        }
    }

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        if !head.is_empty() || head_type == HeadType::Disjunctive {
            self.data.map_head(head, head_type).start_body();
            self.data.map_body_lits(body).end(Some(self.out));
        }
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        if head.is_empty() && head_type != HeadType::Disjunctive {
            return;
        }
        potassco_check_pre!(
            body.iter().all(|weighted_lit| weight(*weighted_lit) >= 0),
            "negative weights in body are not supported"
        );
        if bound <= 0 {
            self.rule(head_type, head, &[]);
            return;
        }
        self.data.map_head(head, head_type).start_sum(bound);
        self.data.map_body_weighted(body);
        let mapped_head = self.data.rule.head().to_vec();
        let mapped_body = self.data.rule.sum_lits().to_vec();
        if head_type == HeadType::Disjunctive && mapped_head.len() == 1 {
            self.data.rule.end(Some(self.out));
            return;
        }
        let aux_head = self.data.new_atom();
        let aux_lit = lit(aux_head);
        self.out
            .rule_weighted(HeadType::Disjunctive, &[aux_head], bound, &mapped_body);
        self.out.rule(head_type, &mapped_head, &[aux_lit]);
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        self.data.add_minimize(priority, lits);
    }

    fn output_atom(&mut self, atom_id: Atom, name: &str) {
        potassco_check_pre!(atom_id != 0, "atom expected");
        let cond = lit(atom_id);
        let mapped = self.make_atom(&[cond], 0, true);
        self.data.add_output(mapped, name.to_owned());
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        self.data.add_term(term_id, name);
    }

    fn output(&mut self, term_id: Id, condition: LitSpan<'_>) {
        let last = self
            .data
            .term_tab
            .get(&term_id)
            .map(|term| term.last)
            .unwrap_or(0);
        potassco_check_pre!(
            self.data.term_tab.contains_key(&term_id),
            "Undefined: term {} is unknown",
            term_id
        );
        let cond_atom = self.make_atom(condition, neg(last), false);
        let needs_output_atom = self
            .data
            .term_tab
            .get(&term_id)
            .is_some_and(|term| term.atom == 0);
        if needs_output_atom {
            let atom_id = self.data.new_atom();
            let term = self
                .data
                .term_tab
                .get_mut(&term_id)
                .expect("term checked above");
            term.atom = atom_id;
            self.data.output.push(OutputData {
                atom: atom_id,
                kind: OutputKind::Name(term.name.clone()),
            });
        }
        let term_atom = self
            .data
            .term_tab
            .get(&term_id)
            .map(|term| term.atom)
            .expect("term checked above");
        self.data
            .rule
            .clear()
            .add_head(term_atom)
            .add_goal(lit(cond_atom))
            .end(Some(self.out));
    }

    fn external(&mut self, atom_id: Atom, value: TruthValue) {
        self.data.add_external(atom_id, value);
    }

    fn heuristic(
        &mut self,
        atom_id: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        if !self.ext {
            self.out
                .heuristic(atom_id, modifier, bias, priority, condition);
        }
        let cond_atom = self.make_atom(condition, 0, true);
        self.data
            .add_heuristic(atom_id, modifier, bias, priority, cond_atom);
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        if !self.ext {
            self.out.acyc_edge(source, target, condition);
        }
        let cond_atom = self.make_atom(condition, 0, true);
        self.data.output.push(OutputData {
            atom: cond_atom,
            kind: OutputKind::Edge(source, target),
        });
    }

    fn end_step(&mut self) {
        self.flush();
        self.out.end_step();
    }
}
