//! Rust port of original_clasp/libpotassco/potassco/smodels.h and
//! original_clasp/libpotassco/src/smodels.cpp.

use std::collections::HashMap;
use std::io::{Read, Write};

use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, DomModifier, HeadType, Lit, LitSpan, TruthValue, Weight,
    WeightLit, WeightLitSpan, atom, lit, neg,
};
use crate::potassco::enums::{EnumTag, enum_name};
use crate::potassco::error::Error;
use crate::potassco::match_basic_types::{
    ProgramReader, ProgramReaderCore, ProgramReaderHooks, is_digit, match_num, match_term,
    read_program,
};
use crate::potassco::rule_utils::RuleBuilder;
use crate::potassco_check_pre;

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SmodelsType {
    End = 0,
    Basic = 1,
    Cardinality = 2,
    Choice = 3,
    Generate = 4,
    Weight = 5,
    Optimize = 6,
    Disjunctive = 8,
    ClaspIncrement = 90,
    ClaspAssignExt = 91,
    ClaspReleaseExt = 92,
}

impl EnumTag for SmodelsType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::End),
            1 => Some(Self::Basic),
            2 => Some(Self::Cardinality),
            3 => Some(Self::Choice),
            4 => Some(Self::Generate),
            5 => Some(Self::Weight),
            6 => Some(Self::Optimize),
            8 => Some(Self::Disjunctive),
            90 => Some(Self::ClaspIncrement),
            91 => Some(Self::ClaspAssignExt),
            92 => Some(Self::ClaspReleaseExt),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        92
    }

    fn count() -> usize {
        10
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct SmodelsOptions {
    pub clasp_ext: bool,
    pub convert_edges: bool,
    pub convert_heuristic: bool,
    pub filter_converted: bool,
}

impl SmodelsOptions {
    #[must_use]
    pub fn enable_clasp_ext(mut self) -> Self {
        self.clasp_ext = true;
        self
    }

    #[must_use]
    pub fn convert_edges(mut self) -> Self {
        self.convert_edges = true;
        self
    }

    #[must_use]
    pub fn convert_heuristic(mut self) -> Self {
        self.convert_heuristic = true;
        self
    }

    #[must_use]
    pub fn drop_converted(mut self) -> Self {
        self.filter_converted = true;
        self
    }
}

#[derive(Clone, Debug)]
struct PendingHeuristic {
    atom_name: String,
    modifier: DomModifier,
    bias: i32,
    priority: u32,
    cond: Lit,
}

#[derive(Default)]
struct Extra {
    atoms: HashMap<String, Atom>,
    nodes: HashMap<String, u32>,
    heuristics: Vec<PendingHeuristic>,
}

impl Extra {
    fn add_atom(&mut self, name: &str, atom_id: Atom) {
        match self.atoms.entry(name.to_owned()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(atom_id);
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                potassco_check_pre!(
                    *entry.get() == 0,
                    "Redefinition: atom '{}' already exists",
                    name
                );
                entry.insert(atom_id);
            }
        }
    }

    fn reserve_atom(&mut self, name: &str) {
        self.atoms.entry(name.to_owned()).or_insert(0);
    }

    fn add_node(&mut self, name: &str) -> u32 {
        if let Some(node) = self.nodes.get(name) {
            *node
        } else {
            let node = self.nodes.len() as u32;
            self.nodes.insert(name.to_owned(), node);
            node
        }
    }
}

pub struct SmodelsInput<'a> {
    out: &'a mut dyn AbstractProgram,
    extra: Option<Extra>,
    opts: SmodelsOptions,
}

impl<'a> SmodelsInput<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn AbstractProgram, opts: SmodelsOptions) -> Self {
        Self {
            out,
            extra: None,
            opts,
        }
    }

    fn extra_mut(&mut self) -> &mut Extra {
        self.extra.get_or_insert_with(Extra::default)
    }

    fn match_body(&mut self, reader: &mut ProgramReaderCore, rule: &mut RuleBuilder) {
        let len = reader.match_uint("number of body literals expected");
        let mut neg_count = reader.match_uint("number of negative body literals expected");
        rule.start_body();
        for _ in 0..len {
            let mut value = lit(reader.match_atom("atom expected"));
            if neg_count > 0 {
                value = -value;
                neg_count -= 1;
            }
            rule.add_goal(value);
        }
    }

    fn match_sum(
        &mut self,
        reader: &mut ProgramReaderCore,
        rule: &mut RuleBuilder,
        weighted: bool,
    ) {
        let mut bound = reader.match_uint("bound expected");
        let mut len = reader.match_uint("number of body literals expected");
        let mut neg_count = reader.match_uint("number of negative body literals expected");
        if !weighted {
            std::mem::swap(&mut len, &mut bound);
            std::mem::swap(&mut bound, &mut neg_count);
        }
        let mut lits = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let mut value = lit(reader.match_atom("atom expected"));
            if neg_count > 0 {
                value = -value;
                neg_count -= 1;
            }
            lits.push(value);
        }
        rule.start_sum(bound as Weight);
        if weighted {
            for value in lits {
                let weight = reader.match_weight(true, "non-negative weight expected");
                rule.add_goal_with_weight(value, weight);
            }
        } else {
            for value in lits {
                rule.add_goal(value);
            }
        }
    }

    fn read_rules(&mut self, reader: &mut ProgramReaderCore) {
        let mut rule = RuleBuilder::default();
        let mut min_priority = 0;
        loop {
            let rule_type = reader.match_enum::<SmodelsType>("rule type expected");
            if rule_type == SmodelsType::End {
                break;
            }
            rule.clear();
            match rule_type {
                SmodelsType::Choice | SmodelsType::Disjunctive => {
                    let head_type = if rule_type == SmodelsType::Choice {
                        HeadType::Choice
                    } else {
                        HeadType::Disjunctive
                    };
                    rule.start_with_type(head_type);
                    let head_len = reader.match_atom("positive head size expected");
                    for _ in 0..head_len {
                        rule.add_head(reader.match_atom("atom expected"));
                    }
                    self.match_body(reader, &mut rule);
                    rule.end(Some(self.out));
                }
                SmodelsType::Basic => {
                    rule.start_with_type(HeadType::Disjunctive)
                        .add_head(reader.match_atom("atom expected"));
                    self.match_body(reader, &mut rule);
                    rule.end(Some(self.out));
                }
                SmodelsType::Cardinality | SmodelsType::Weight => {
                    rule.start_with_type(HeadType::Disjunctive)
                        .add_head(reader.match_atom("atom expected"));
                    self.match_sum(reader, &mut rule, rule_type == SmodelsType::Weight);
                    rule.end(Some(self.out));
                }
                SmodelsType::Optimize => {
                    rule.start_minimize(min_priority);
                    min_priority += 1;
                    self.match_sum(reader, &mut rule, true);
                    rule.end(Some(self.out));
                }
                SmodelsType::ClaspIncrement => {
                    let ok = self.opts.clasp_ext && reader.match_id("id expected") == 0;
                    reader.require(ok, "unrecognized rule type");
                }
                SmodelsType::ClaspAssignExt => {
                    reader.require(self.opts.clasp_ext, "unrecognized rule type");
                    let atom_id = reader.match_atom("atom expected");
                    let code = reader.match_uint_range(0, 2, "0..2 expected");
                    self.out.external(atom_id, decode_external_value(code));
                }
                SmodelsType::ClaspReleaseExt => {
                    reader.require(self.opts.clasp_ext, "unrecognized rule type");
                    let atom_id = reader.match_atom("atom expected");
                    self.out.external(atom_id, TruthValue::Release);
                }
                _ => reader.error("unrecognized rule type"),
            }
        }
    }

    fn read_symbols(&mut self, reader: &mut ProgramReaderCore) {
        if self.extra.is_none() && (self.opts.convert_edges || self.opts.convert_heuristic) {
            self.extra = Some(Extra::default());
        }
        loop {
            let atom_id = reader.match_atom_or_zero("atom expected");
            if atom_id == 0 {
                break;
            }
            reader.match_char(' ');
            let mut name = String::new();
            loop {
                let c = reader.get();
                reader.require(c != '\0', "atom name expected!");
                if c == '\n' {
                    break;
                }
                name.push(c);
            }
            let mapped = self.extra.is_some() && self.map_symbol(atom_id, &name);
            if !mapped {
                self.out.output_atom(atom_id, &name);
            }
            if self.opts.convert_heuristic {
                self.extra_mut().add_atom(&name, atom_id);
            }
        }
        if let Some(extra) = self.extra.as_mut() {
            for pending in std::mem::take(&mut extra.heuristics) {
                if let Some(&atom_id) = extra.atoms.get(&pending.atom_name) {
                    if atom_id == 0 {
                        continue;
                    }
                    let cond = [pending.cond];
                    self.out.heuristic(
                        atom_id,
                        pending.modifier,
                        pending.bias,
                        pending.priority,
                        &cond,
                    );
                }
            }
        }
        if !reader.incremental() {
            self.extra = None;
        }
    }

    fn map_symbol(&mut self, atom_id: Atom, name: &str) -> bool {
        let atom_lit = lit(atom_id);
        if self.opts.convert_edges {
            let mut n0 = "";
            let mut n1 = "";
            if match_edge_pred(name, &mut n0, &mut n1) {
                let source = self.extra_mut().add_node(n0) as i32;
                let target = self.extra_mut().add_node(n1) as i32;
                let cond = [atom_lit];
                self.out.acyc_edge(source, target, &cond);
                return self.opts.filter_converted;
            }
        }

        if self.opts.convert_heuristic {
            let mut atom_name = "";
            let mut modifier = DomModifier::Init;
            let mut bias = 0;
            let mut priority = 0;
            if match_dom_heu_pred(
                name,
                &mut atom_name,
                &mut modifier,
                &mut bias,
                &mut priority,
            ) {
                let cond = [atom_lit];
                let target_atom = self.extra_mut().atoms.get(atom_name).copied();
                if let Some(target_atom) = target_atom {
                    if target_atom != 0 {
                        self.out
                            .heuristic(target_atom, modifier, bias, priority, &cond);
                    } else {
                        self.extra_mut().heuristics.push(PendingHeuristic {
                            atom_name: atom_name.to_owned(),
                            modifier,
                            bias,
                            priority,
                            cond: atom_lit,
                        });
                    }
                } else {
                    self.extra_mut().reserve_atom(atom_name);
                    self.extra_mut().heuristics.push(PendingHeuristic {
                        atom_name: atom_name.to_owned(),
                        modifier,
                        bias,
                        priority,
                        cond: atom_lit,
                    });
                }
            }
        }
        false
    }

    fn read_compute(&mut self, reader: &mut ProgramReaderCore) {
        for (part, positive) in [("B+", true), ("B-", false)] {
            reader.skip_ws();
            let matched = reader.r#match(part);
            reader.require(matched, "compute statement expected");
            reader.match_char('\n');
            loop {
                let mut value = reader.match_atom_or_zero("atom expected") as Lit;
                if value == 0 {
                    break;
                }
                if positive {
                    value = neg(value);
                }
                let body = [value];
                self.out.rule(HeadType::Disjunctive, &[], &body);
            }
        }
    }

    fn read_extra(&mut self, reader: &mut ProgramReaderCore) {
        reader.skip_ws();
        if reader.r#match("E") {
            loop {
                let atom_id = reader.match_atom_or_zero("atom expected");
                if atom_id == 0 {
                    break;
                }
                self.out.external(atom_id, TruthValue::Free);
            }
        }
        let _ = reader.match_uint("number of models expected");
    }
}

impl ProgramReaderHooks for SmodelsInput<'_> {
    fn do_attach(&mut self, reader: &mut ProgramReaderCore, incremental: &mut bool) -> bool {
        let next = reader.peek();
        if is_digit(next) && (next != '9' || self.opts.clasp_ext) {
            *incremental = next == '9';
            self.out.init_program(*incremental);
            true
        } else {
            false
        }
    }

    fn do_parse(&mut self, reader: &mut ProgramReaderCore) -> bool {
        self.out.begin_step();
        self.read_rules(reader);
        self.read_symbols(reader);
        self.read_compute(reader);
        self.read_extra(reader);
        self.out.end_step();
        true
    }

    fn do_reset(&mut self, _reader: &mut ProgramReaderCore) {}
}

pub fn read_smodels<R: Read>(input: R, out: &mut dyn AbstractProgram, opts: SmodelsOptions) -> i32 {
    let mut reader = ProgramReader::new(SmodelsInput::new(out, opts));
    read_program(input, &mut reader)
}

/// Rust port of the non-copyable C++ `SmodelsOutput` writer.
///
/// ```compile_fail
/// use rust_clasp::potassco::smodels::SmodelsOutput;
///
/// let writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
/// let _clone = writer.clone();
/// ```
///
/// ```compile_fail
/// use rust_clasp::potassco::smodels::SmodelsOutput;
///
/// let writer = SmodelsOutput::new(Vec::<u8>::new(), false, 0);
/// let _moved = writer;
/// let _copy_like_use = writer;
/// ```
pub struct SmodelsOutput<W: Write> {
    os: W,
    false_atom: Atom,
    section: u8,
    ext: bool,
    incremental: bool,
    false_head_used: bool,
}

impl<W: Write> SmodelsOutput<W> {
    #[must_use]
    pub fn new(os: W, enable_clasp_ext: bool, false_atom: Atom) -> Self {
        Self {
            os,
            false_atom,
            section: 0,
            ext: enable_clasp_ext,
            incremental: false,
            false_head_used: false,
        }
    }

    pub fn into_inner(self) -> W {
        self.os
    }

    fn start_rule(&mut self, rule_type: SmodelsType) -> &mut Self {
        potassco_check_pre!(
            self.section == 0
                || rule_type == SmodelsType::End
                || rule_type >= SmodelsType::ClaspIncrement,
            "adding rules after symbols not supported"
        );
        write!(self.os, "{}", rule_type.to_underlying()).expect("smodels writer failed");
        self
    }

    fn add_unsigned(&mut self, value: u32) -> &mut Self {
        write!(self.os, " {value}").expect("smodels writer failed");
        self
    }

    fn add_atom_value(&mut self, value: Atom) -> &mut Self {
        self.add_unsigned(value)
    }

    fn end_rule(&mut self) -> &mut Self {
        self.os.write_all(b"\n").expect("smodels writer failed");
        self
    }

    fn add_head(&mut self, head_type: HeadType, head: AtomSpan<'_>) -> &mut Self {
        if head.is_empty() {
            potassco_check_pre!(
                self.false_atom != 0 && head_type == HeadType::Disjunctive,
                "empty head requires false atom"
            );
            self.false_head_used = true;
            return self.add_atom_value(self.false_atom);
        }
        if head_type == HeadType::Choice || head.len() > 1 {
            self.add_unsigned(head.len() as u32);
        }
        for &atom_id in head {
            self.add_atom_value(atom_id);
        }
        self
    }

    fn add_body(&mut self, lits: LitSpan<'_>) -> &mut Self {
        let negatives = lits.iter().filter(|&&lit_value| lit_value < 0).count() as u32;
        self.add_unsigned(lits.len() as u32);
        self.add_unsigned(negatives);
        for sign in [-1, 1] {
            for &lit_value in lits {
                if lit_value.signum() == sign {
                    self.add_atom_value(atom(lit_value));
                }
            }
        }
        self
    }

    fn smodels_lit(weighted_lit: WeightLit) -> Lit {
        if weighted_lit.weight >= 0 {
            weighted_lit.lit
        } else {
            -weighted_lit.lit
        }
    }

    fn add_weighted_body(
        &mut self,
        bound: Weight,
        lits: WeightLitSpan<'_>,
        cardinality: bool,
    ) -> &mut Self {
        let negatives = lits
            .iter()
            .filter(|&&weighted_lit| Self::smodels_lit(weighted_lit) < 0)
            .count() as u32;
        if !cardinality {
            self.add_unsigned(bound as u32);
        }
        self.add_unsigned(lits.len() as u32);
        self.add_unsigned(negatives);
        if cardinality {
            self.add_unsigned(bound as u32);
        }
        for sign in [-1, 1] {
            for &weighted_lit in lits {
                if Self::smodels_lit(weighted_lit).signum() == sign {
                    self.add_atom_value(atom(weighted_lit));
                }
            }
        }
        if !cardinality {
            for sign in [-1, 1] {
                for &weighted_lit in lits {
                    if Self::smodels_lit(weighted_lit).signum() == sign {
                        self.add_unsigned(weighted_lit.weight.unsigned_abs());
                    }
                }
            }
        }
        self
    }
}

impl<W: Write> AbstractProgram for SmodelsOutput<W> {
    fn init_program(&mut self, incremental: bool) {
        potassco_check_pre!(
            !incremental || self.ext,
            "incremental programs not supported in smodels format"
        );
        self.incremental = incremental;
    }

    fn begin_step(&mut self) {
        self.section = 0;
        self.false_head_used = false;
        if self.ext && self.incremental {
            self.start_rule(SmodelsType::ClaspIncrement)
                .add_unsigned(0)
                .end_rule();
        }
    }

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        if head.is_empty() && head_type == HeadType::Choice {
            return;
        }
        potassco_check_pre!(
            self.false_atom != 0 || !head.is_empty(),
            "empty head requires false atom"
        );
        let rule_type = if head_type == HeadType::Choice {
            SmodelsType::Choice
        } else if head.len() > 1 {
            SmodelsType::Disjunctive
        } else {
            SmodelsType::Basic
        };
        self.start_rule(rule_type)
            .add_head(head_type, head)
            .add_body(body)
            .end_rule();
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        if head.is_empty() && head_type == HeadType::Choice {
            return;
        }
        potassco_check_pre!(
            head_type == HeadType::Disjunctive && head.len() < 2,
            "normal head expected"
        );
        potassco_check_pre!(
            self.false_atom != 0 || !head.is_empty(),
            "empty head requires false atom"
        );
        let normalized_bound = bound.max(0);
        let mut rule_type = SmodelsType::Cardinality;
        for &weighted_lit in body {
            potassco_check_pre!(weighted_lit.weight >= 0, "negative weights not supported");
            if weighted_lit.weight != 1 {
                rule_type = SmodelsType::Weight;
            }
        }
        self.start_rule(rule_type)
            .add_head(head_type, head)
            .add_weighted_body(
                normalized_bound,
                body,
                rule_type == SmodelsType::Cardinality,
            )
            .end_rule();
    }

    fn minimize(&mut self, _priority: Weight, lits: WeightLitSpan<'_>) {
        self.start_rule(SmodelsType::Optimize)
            .add_weighted_body(0, lits, false)
            .end_rule();
    }

    fn output_atom(&mut self, atom_id: Atom, name: &str) {
        potassco_check_pre!(
            self.section <= 1,
            "adding symbols after compute not supported"
        );
        potassco_check_pre!(atom_id != 0, "atom expected");
        if self.section == 0 {
            self.start_rule(SmodelsType::End).end_rule();
            self.section = 1;
        }
        writeln!(self.os, "{atom_id} {name}").expect("smodels writer failed");
    }

    fn external(&mut self, atom_id: Atom, value: TruthValue) {
        potassco_check_pre!(
            self.ext,
            "external directive not supported in smodels format"
        );
        if value != TruthValue::Release {
            self.start_rule(SmodelsType::ClaspAssignExt)
                .add_atom_value(atom_id)
                .add_unsigned(encode_external_value(value))
                .end_rule();
        } else {
            self.start_rule(SmodelsType::ClaspReleaseExt)
                .add_atom_value(atom_id)
                .end_rule();
        }
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        potassco_check_pre!(
            self.section < 2,
            "at most one compute statement supported in smodels format"
        );
        while self.section != 2 {
            self.start_rule(SmodelsType::End).end_rule();
            self.section += 1;
        }
        self.os.write_all(b"B+\n").expect("smodels writer failed");
        for &lit_value in lits {
            if lit_value > 0 {
                writeln!(self.os, "{}", atom(lit_value)).expect("smodels writer failed");
            }
        }
        self.os
            .write_all(b"0\nB-\n")
            .expect("smodels writer failed");
        for &lit_value in lits {
            if lit_value < 0 {
                writeln!(self.os, "{}", atom(lit_value)).expect("smodels writer failed");
            }
        }
        if self.false_head_used && self.false_atom != 0 {
            writeln!(self.os, "{}", self.false_atom).expect("smodels writer failed");
        }
        self.os.write_all(b"0\n").expect("smodels writer failed");
    }

    fn end_step(&mut self) {
        if self.section < 2 {
            self.assume(&[]);
        }
        self.os.write_all(b"1\n").expect("smodels writer failed");
    }
}

fn decode_external_value(code: u32) -> TruthValue {
    match code {
        0 => TruthValue::False,
        1 => TruthValue::True,
        2 => TruthValue::Free,
        _ => panic!("invalid external value code"),
    }
}

fn encode_external_value(value: TruthValue) -> u32 {
    match value {
        TruthValue::False => 0,
        TruthValue::True => 1,
        TruthValue::Free => 2,
        TruthValue::Release => panic_any_release(),
    }
}

fn panic_any_release() -> ! {
    std::panic::panic_any(Error::InvalidArgument(
        "release value must use release directive".to_owned(),
    ))
}

pub fn match_edge_pred<'a>(input: &'a str, left: &mut &'a str, right: &mut &'a str) -> bool {
    if let Some(mut tail) = input.strip_prefix("_acyc_") {
        if !match_num(&mut tail, None, None) || !tail.starts_with('_') {
            return false;
        }
        tail = &tail[1..];
        if !match_num(&mut tail, Some(left), None) || !tail.starts_with('_') {
            return false;
        }
        tail = &tail[1..];
        return match_num(&mut tail, Some(right), None) && tail.is_empty();
    }
    if let Some(mut tail) = input.strip_prefix("_edge(") {
        if let Some(src) = match_term(&mut tail) {
            *left = src;
        } else {
            return false;
        }
        if !tail.starts_with(',') {
            return false;
        }
        tail = &tail[1..];
        if let Some(dst) = match_term(&mut tail) {
            *right = dst;
        } else {
            return false;
        }
        return tail == ")";
    }
    false
}

pub fn match_dom_heu_pred<'a>(
    input: &'a str,
    atom_name: &mut &'a str,
    modifier: &mut DomModifier,
    bias: &mut i32,
    priority: &mut u32,
) -> bool {
    let Some(mut tail) = input.strip_prefix("_heuristic(") else {
        return false;
    };
    let Some(atom_term) = match_term(&mut tail) else {
        return false;
    };
    *atom_name = atom_term;
    if !tail.starts_with(',') {
        return false;
    }
    tail = &tail[1..];
    let Some((heu_modifier, remaining)) = match_dom_modifier(tail) else {
        return false;
    };
    *modifier = heu_modifier;
    tail = remaining;
    if !tail.starts_with(',') {
        return false;
    }
    tail = &tail[1..];
    let mut parsed_bias = 0;
    if !match_num(&mut tail, None, Some(&mut parsed_bias)) {
        return false;
    }
    *bias = parsed_bias;
    *priority = parsed_bias.unsigned_abs();
    if tail.starts_with(',') {
        tail = &tail[1..];
        let mut parsed_priority = 0;
        if !match_num(&mut tail, None, Some(&mut parsed_priority)) || parsed_priority < 0 {
            return false;
        }
        *priority = parsed_priority as u32;
    }
    tail == ")"
}

fn match_dom_modifier(input: &str) -> Option<(DomModifier, &str)> {
    for modifier in [
        DomModifier::Level,
        DomModifier::Sign,
        DomModifier::Factor,
        DomModifier::Init,
        DomModifier::True,
        DomModifier::False,
    ] {
        let name = enum_name(modifier);
        if let Some(rest) = input.strip_prefix(name) {
            return Some((modifier, rest));
        }
    }
    None
}
