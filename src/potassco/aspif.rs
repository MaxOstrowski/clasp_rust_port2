//! Rust port of original_clasp/libpotassco/potassco/aspif.h and
//! original_clasp/libpotassco/src/aspif.cpp.

use std::borrow::Cow;
use std::io::{Read, Write};

use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, BodyType, DomModifier, HeadType, Id, IdSpan, Lit, LitSpan,
    TruthValue, Weight, WeightLit, WeightLitSpan, atom, lit, neg, to_span,
};
use crate::potassco::enums::EnumTag;
use crate::potassco::match_basic_types::{
    ProgramReader, ProgramReaderCore, ProgramReaderHooks, read_program,
};
use crate::potassco::rule_utils::RuleBuilder;
use crate::potassco_check_pre;

const MAX_ASPIF_VERSION: u32 = 2;

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AspifType {
    End = 0,
    Rule = 1,
    Minimize = 2,
    Project = 3,
    Output = 4,
    External = 5,
    Assume = 6,
    Heuristic = 7,
    Edge = 8,
    Theory = 9,
    Comment = 10,
}

impl EnumTag for AspifType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::End),
            1 => Some(Self::Rule),
            2 => Some(Self::Minimize),
            3 => Some(Self::Project),
            4 => Some(Self::Output),
            5 => Some(Self::External),
            6 => Some(Self::Assume),
            7 => Some(Self::Heuristic),
            8 => Some(Self::Edge),
            9 => Some(Self::Theory),
            10 => Some(Self::Comment),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        10
    }

    fn count() -> usize {
        11
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OutputType {
    Atom = 0,
    Term = 1,
    Cond = 2,
}

impl EnumTag for OutputType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Atom),
            1 => Some(Self::Term),
            2 => Some(Self::Cond),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        2
    }

    fn count() -> usize {
        3
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TheoryType {
    Number = 0,
    Symbol = 1,
    Compound = 2,
    Element = 4,
    Atom = 5,
    AtomWithGuard = 6,
}

impl EnumTag for TheoryType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Number),
            1 => Some(Self::Symbol),
            2 => Some(Self::Compound),
            4 => Some(Self::Element),
            5 => Some(Self::Atom),
            6 => Some(Self::AtomWithGuard),
            _ => None,
        }
    }

    fn min_value() -> Self::Repr {
        0
    }

    fn max_value() -> Self::Repr {
        6
    }

    fn count() -> usize {
        7
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OutputMapping {
    Term = 0,
    Atom = 1,
    AtomFact = 2,
}

struct Extra {
    rule: RuleBuilder,
    ids: Vec<Id>,
    facts: Vec<Atom>,
    fact_terms: Vec<String>,
    sym: String,
    next_fact: u32,
}

impl Extra {
    fn pop_fact(&mut self) -> Atom {
        let len = self.facts.len();
        let idx = (self.next_fact as usize) % len;
        self.next_fact = self.next_fact.wrapping_add(1);
        self.facts[idx]
    }

    fn has_fact(&self) -> bool {
        !self.facts.is_empty()
    }
}

pub struct AspifInput<'a> {
    out: &'a mut dyn AbstractProgram,
    data: Option<Extra>,
    version: u32,
    next_term: Id,
    fact: Atom,
    initial_fact: Atom,
    map_output: OutputMapping,
}

impl<'a> AspifInput<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn AbstractProgram, map_output: OutputMapping, fact: Atom) -> Self {
        Self {
            out,
            data: None,
            version: 0,
            next_term: 0,
            fact,
            initial_fact: fact,
            map_output,
        }
    }

    fn out_term(&mut self, term: &str, cond: LitSpan<'_>) {
        let term_id = self.next_term;
        self.next_term = self.next_term.wrapping_add(1);
        self.out.output_term(term_id, term);
        self.out.output(term_id, cond);
    }

    fn data_mut(&mut self) -> &mut Extra {
        self.data
            .as_mut()
            .expect("aspif parser state is not initialized")
    }

    fn emit_rule(&mut self) {
        let (is_minimize, head_type, head, body_type, body, bound, sum_lits) = {
            let rule = &self.data_mut().rule;
            (
                rule.is_minimize(),
                rule.head_type(),
                rule.head().to_vec(),
                rule.body_type(),
                rule.body().to_vec(),
                rule.bound(),
                rule.sum_lits().to_vec(),
            )
        };
        if body_type == BodyType::Normal {
            self.out.rule(head_type, &head, &body);
        } else if is_minimize {
            self.out.minimize(bound, &sum_lits);
        } else {
            self.out.rule_weighted(head_type, &head, bound, &sum_lits);
        }
    }

    fn match_atoms(&mut self, reader: &mut ProgramReaderCore) {
        let len = reader.match_uint("number of atoms expected");
        for _ in 0..len {
            let atom = reader.match_atom("atom expected");
            self.data_mut().rule.add_head(atom);
        }
    }

    fn match_lits(&mut self, reader: &mut ProgramReaderCore) {
        self.data_mut().rule.start_body();
        let len = reader.match_uint("number of literals expected");
        for _ in 0..len {
            let lit = reader.match_lit("literal expected");
            self.data_mut().rule.add_goal(lit);
        }
    }

    fn match_weighted_lits(&mut self, reader: &mut ProgramReaderCore, positive: bool) {
        let len = reader.match_uint("number of literals expected");
        for _ in 0..len {
            let wlit = reader.match_wlit(positive, "weight literal expected");
            self.data_mut().rule.add_weight_lit(wlit);
        }
    }

    fn match_string(&mut self, reader: &mut ProgramReaderCore) {
        let len = reader.match_uint("non-negative string length expected") as usize;
        reader.match_char(' ');
        let sym = &mut self.data_mut().sym;
        sym.clear();
        for _ in 0..len {
            let c = reader.get();
            reader.require(c != '\0', "invalid string");
            sym.push(c);
        }
    }

    fn match_ids(&mut self, reader: &mut ProgramReaderCore) {
        let len = reader.match_uint("number of terms expected") as usize;
        let ids = &mut self.data_mut().ids;
        ids.clear();
        ids.reserve(len);
        for _ in 0..len {
            ids.push(reader.match_id("term id expected"));
        }
    }

    fn match_output(&mut self, reader: &mut ProgramReaderCore, t: OutputType) {
        match t {
            OutputType::Atom => {
                let atom = reader.match_atom("atom expected");
                self.match_string(reader);
                let name = self.data_mut().sym.clone();
                self.out.output_atom(atom, &name);
            }
            OutputType::Term => {
                let term = reader.match_id("term id expected");
                self.match_string(reader);
                let name = self.data_mut().sym.clone();
                self.out.output_term(term, &name);
            }
            OutputType::Cond => {
                let term = reader.match_id("term id expected");
                self.match_lits(reader);
                let body = self.data_mut().rule.body().to_vec();
                self.out.output(term, &body);
            }
        }
    }

    fn match_theory(&mut self, reader: &mut ProgramReaderCore, t: TheoryType) {
        let id = reader.match_id("term id expected");
        match t {
            TheoryType::Number => {
                let number = reader.match_int("number expected");
                self.out.theory_term_number(id, number);
            }
            TheoryType::Symbol => {
                self.match_string(reader);
                let name = self.data_mut().sym.clone();
                self.out.theory_term_symbol(id, &name);
            }
            TheoryType::Compound => {
                let compound = reader.match_int("unrecognized compound term type");
                self.match_ids(reader);
                let ids = self.data_mut().ids.clone();
                self.out.theory_term_compound(id, compound, &ids);
            }
            TheoryType::Element => {
                self.match_ids(reader);
                let ids = self.data_mut().ids.clone();
                self.match_lits(reader);
                let body = self.data_mut().rule.body().to_vec();
                self.out.theory_element(id, &ids, &body);
            }
            TheoryType::Atom => {
                let atom_term = reader.match_id("term id expected");
                self.match_ids(reader);
                let ids = self.data_mut().ids.clone();
                self.out.theory_atom(id, atom_term, &ids);
            }
            TheoryType::AtomWithGuard => {
                let atom_term = reader.match_id("term id expected");
                self.match_ids(reader);
                let ids = self.data_mut().ids.clone();
                let op = reader.match_id("guard id expected");
                let rhs = reader.match_id("guard rhs expected");
                self.out.theory_atom_guarded(id, atom_term, &ids, op, rhs);
            }
        }
    }
}

impl ProgramReaderHooks for AspifInput<'_> {
    fn do_attach(&mut self, reader: &mut ProgramReaderCore, incremental: &mut bool) -> bool {
        if !reader.r#match("asp ") {
            return false;
        }
        self.version = reader.match_uint_range(1, 2, "unsupported major version");
        let _ = reader.match_uint_range(0, 0, "unsupported minor version");
        let _ = reader.match_uint("revision number expected");
        while reader.r#match(" ") {}
        *incremental = reader.r#match("incremental");
        reader.match_char('\n');
        self.out.init_program(*incremental);
        true
    }

    fn do_parse(&mut self, reader: &mut ProgramReaderCore) -> bool {
        self.data = Some(Extra {
            rule: RuleBuilder::default(),
            ids: Vec::new(),
            facts: Vec::new(),
            fact_terms: Vec::new(),
            sym: String::new(),
            next_fact: 0,
        });
        let fact = self.fact;
        if self.fact != 0 || self.map_output == OutputMapping::AtomFact {
            self.data_mut().facts.push(fact);
        }
        self.out.begin_step();
        let keep_facts = self.version == 1 && self.map_output == OutputMapping::Atom;
        loop {
            let rule_type = reader.match_enum::<AspifType>("rule type or 0 expected");
            if rule_type == AspifType::End {
                break;
            }
            match rule_type {
                AspifType::Rule => {
                    let head_type = reader.match_enum::<HeadType>("invalid head type");
                    self.data_mut().rule.start_with_type(head_type);
                    self.match_atoms(reader);
                    let body_type = reader.match_enum::<BodyType>("invalid body type");
                    if body_type == BodyType::Normal {
                        self.match_lits(reader);
                        if keep_facts && self.data_mut().rule.is_fact() {
                            let head = self.data_mut().rule.head().to_vec();
                            if let Some(&fact_atom) = head.first() {
                                self.data_mut().facts.push(fact_atom);
                            }
                        }
                    } else {
                        reader.require(body_type == BodyType::Sum, "unexpected body type");
                        let bound = reader.match_weight(true, "weight expected");
                        self.data_mut().rule.start_sum(bound);
                        self.match_weighted_lits(reader, true);
                    }
                    self.emit_rule();
                }
                AspifType::Minimize => {
                    let prio = reader.match_weight(false, "priority expected");
                    self.data_mut().rule.start_minimize(prio);
                    self.match_weighted_lits(reader, false);
                    self.emit_rule();
                }
                AspifType::Project => {
                    self.match_atoms(reader);
                    let head = self.data_mut().rule.head().to_vec();
                    self.out.project(&head);
                }
                AspifType::Output => {
                    if self.version == 1 {
                        self.match_string(reader);
                        self.match_lits(reader);
                        let cond = self.data_mut().rule.body().to_vec();
                        let name = self.data_mut().sym.clone();
                        if self.map_output != OutputMapping::Term
                            && (cond.is_empty() || (cond.len() == 1 && cond[0] > 0))
                        {
                            if let Some(&single) = cond.first() {
                                self.out.output_atom(atom(single), &name);
                            } else if self.data_mut().has_fact() {
                                let fact_atom = self.data_mut().pop_fact();
                                self.out.output_atom(fact_atom, &name);
                            } else {
                                self.data_mut().fact_terms.push(name);
                            }
                        } else {
                            self.out_term(&name, &cond);
                        }
                    } else {
                        let out_type = reader.match_enum::<OutputType>("invalid output directive");
                        self.match_output(reader, out_type);
                    }
                }
                AspifType::External => {
                    let atom = reader.match_atom_or_zero("atom expected");
                    if atom != 0 {
                        let value = reader.match_enum::<TruthValue>("value expected");
                        self.out.external(atom, value);
                    }
                }
                AspifType::Assume => {
                    self.match_lits(reader);
                    let body = self.data_mut().rule.body().to_vec();
                    self.out.assume(&body);
                }
                AspifType::Heuristic => {
                    let modifier = reader.match_enum::<DomModifier>("invalid heuristic modifier");
                    let atom = reader.match_atom("atom expected");
                    let bias = reader.match_int("invalid heuristic bias");
                    let prio = reader.match_uint("invalid heuristic priority");
                    self.match_lits(reader);
                    let body = self.data_mut().rule.body().to_vec();
                    self.out.heuristic(atom, modifier, bias, prio, &body);
                }
                AspifType::Edge => {
                    let start = reader.match_int("invalid edge, start node expected");
                    let end = reader.match_int("invalid edge, end node expected");
                    self.match_lits(reader);
                    let body = self.data_mut().rule.body().to_vec();
                    self.out.acyc_edge(start, end, &body);
                }
                AspifType::Theory => {
                    let theory_type = reader.match_enum::<TheoryType>("invalid theory directive");
                    self.match_theory(reader, theory_type);
                }
                AspifType::Comment => {
                    reader.skip_line();
                }
                AspifType::End => unreachable!(),
            }
            self.data_mut().rule.clear();
        }
        let pending_terms = self.data_mut().fact_terms.clone();
        for sym in pending_terms {
            if self.data_mut().has_fact() {
                let fact_atom = self.data_mut().pop_fact();
                self.out.output_atom(fact_atom, &sym);
            } else {
                self.out_term(&sym, &[]);
            }
        }
        if self.fact == 0 {
            if let Some(&fact_atom) = self.data_mut().facts.first() {
                self.fact = fact_atom;
            }
        }
        self.out.end_step();
        self.data = None;
        true
    }

    fn do_reset(&mut self, _reader: &mut ProgramReaderCore) {
        self.data = None;
        self.version = 0;
        self.next_term = 0;
        self.fact = self.initial_fact;
    }
}

pub fn read_aspif<R: Read>(prg: R, out: &mut dyn AbstractProgram) -> i32 {
    let mut reader = ProgramReader::new(AspifInput::new(out, OutputMapping::Atom, 0));
    read_program(prg, &mut reader)
}

#[derive(Clone, Debug, Default)]
struct OutTerm {
    name: String,
    atom: Atom,
    last: Atom,
}

#[derive(Default)]
struct OutputData {
    out_terms: Vec<OutTerm>,
    mapping: Vec<Id>,
    true_atom: Atom,
}

impl OutputData {
    fn add_term(&mut self, term_id: Id, term_name: &str) {
        let idx = term_id as usize;
        if self.out_terms.len() <= idx {
            self.out_terms.resize_with(idx + 1, OutTerm::default);
        }
        potassco_check_pre!(
            self.out_terms[idx].name.is_empty(),
            "Redefinition: term {} already defined",
            term_id
        );
        self.out_terms[idx].name = term_name.to_owned();
    }
}

pub struct AspifOutput<W: Write> {
    os: W,
    data: Option<OutputData>,
    version: u32,
    identity_max: u32,
    next_atom: u32,
}

impl<W: Write> AspifOutput<W> {
    pub fn new(os: W, version: u32) -> Self {
        potassco_check_pre!(version <= MAX_ASPIF_VERSION, "unexpected version");
        Self {
            os,
            data: None,
            version: if version == 0 {
                MAX_ASPIF_VERSION
            } else {
                version
            },
            identity_max: 0,
            next_atom: 0,
        }
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn into_inner(self) -> W {
        self.os
    }

    fn data_mut(&mut self) -> &mut OutputData {
        self.data.get_or_insert_with(OutputData::default)
    }

    fn write_scalar<T: std::fmt::Display>(&mut self, value: T) {
        write!(self.os, " {}", value).expect("aspif writer failed");
    }

    fn write_enum<E: EnumTag>(&mut self, value: E)
    where
        E::Repr: std::fmt::Display,
    {
        self.write_scalar(value.to_underlying());
    }

    fn write_slice<T: std::fmt::Display>(&mut self, values: &[T]) {
        self.write_scalar(values.len());
        for value in values {
            self.write_scalar(value);
        }
    }

    fn write_weight_lit_slice(&mut self, values: WeightLitSpan<'_>) {
        self.write_scalar(values.len());
        for value in values {
            self.write_scalar(value.lit);
            self.write_scalar(value.weight);
        }
    }

    fn write_string(&mut self, value: &str) {
        write!(self.os, " {} ", value.len()).expect("aspif writer failed");
        self.os
            .write_all(value.as_bytes())
            .expect("aspif writer failed");
    }

    fn start_dir(&mut self, rule: AspifType) {
        write!(self.os, "{}", rule.to_underlying()).expect("aspif writer failed");
    }

    fn end_dir(&mut self) {
        self.os.write_all(b"\n").expect("aspif writer failed");
    }

    fn new_atom(&mut self) -> Atom {
        let ret = self.next_atom;
        self.next_atom = self.next_atom.wrapping_add(1);
        ret
    }

    fn map_atom(&mut self, atom: Atom) -> Atom {
        if atom == 0 {
            return 0;
        }
        if self.next_atom == 0 || atom <= self.identity_max {
            self.identity_max = self.identity_max.max(atom);
            return atom;
        }
        let key = (atom - self.identity_max) as usize;
        if self
            .data
            .as_ref()
            .is_none_or(|data| data.mapping.len() <= key)
        {
            self.data_mut().mapping.resize(key + 1, 0);
        }
        if self
            .data
            .as_ref()
            .is_some_and(|data| data.mapping[key] != 0)
        {
            return self.data.as_ref().expect("output data present").mapping[key];
        }
        let mapped = self.new_atom();
        self.data_mut().mapping[key] = mapped;
        mapped
    }

    fn map_atoms<'a>(&mut self, atoms: AtomSpan<'a>) -> Cow<'a, [Atom]> {
        let max_atom = if self.version() == 1 {
            atoms.iter().copied().max().unwrap_or(0)
        } else {
            0
        };
        if self.next_atom == 0 || max_atom <= self.identity_max {
            self.identity_max = self.identity_max.max(max_atom);
            Cow::Borrowed(atoms)
        } else {
            Cow::Owned(atoms.iter().copied().map(|a| self.map_atom(a)).collect())
        }
    }

    fn map_lits<'a>(&mut self, lits: LitSpan<'a>) -> Cow<'a, [Lit]> {
        let max_atom = if self.version() == 1 {
            lits.iter().map(|&lit| atom(lit)).max().unwrap_or(0)
        } else {
            0
        };
        if self.next_atom == 0 || max_atom <= self.identity_max {
            self.identity_max = self.identity_max.max(max_atom);
            Cow::Borrowed(lits)
        } else {
            Cow::Owned(
                lits.iter()
                    .map(|&value| {
                        let mapped = self.map_atom(atom(value));
                        if value < 0 { neg(mapped) } else { lit(mapped) }
                    })
                    .collect(),
            )
        }
    }

    fn map_weight_lits<'a>(&mut self, lits: WeightLitSpan<'a>) -> Cow<'a, [WeightLit]> {
        let max_atom = if self.version() == 1 {
            lits.iter().map(|&lit| atom(lit)).max().unwrap_or(0)
        } else {
            0
        };
        if self.next_atom == 0 || max_atom <= self.identity_max {
            self.identity_max = self.identity_max.max(max_atom);
            Cow::Borrowed(lits)
        } else {
            Cow::Owned(
                lits.iter()
                    .copied()
                    .map(|mut value| {
                        let mapped = self.map_atom(atom(value));
                        value.lit = if value.lit < 0 {
                            neg(mapped)
                        } else {
                            lit(mapped)
                        };
                        value
                    })
                    .collect(),
            )
        }
    }

    fn aux_rule(&mut self, head: Atom, body: LitSpan<'_>) {
        self.start_dir(AspifType::Rule);
        self.write_enum(HeadType::Disjunctive);
        self.write_slice(to_span(&head));
        self.write_enum(BodyType::Normal);
        self.write_slice(body);
        self.end_dir();
    }
}

impl<W: Write> AbstractProgram for AspifOutput<W> {
    fn init_program(&mut self, incremental: bool) {
        write!(self.os, "asp {} 0 0", self.version()).expect("aspif writer failed");
        if incremental {
            self.os
                .write_all(b" incremental")
                .expect("aspif writer failed");
        }
        self.os.write_all(b"\n").expect("aspif writer failed");
    }

    fn begin_step(&mut self) {
        if let Some(data) = self.data.as_mut() {
            for term in &mut data.out_terms {
                if term.atom != 0 {
                    term.last = term.atom;
                    term.atom = 0;
                }
            }
        }
    }

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        let head = self.map_atoms(head).into_owned();
        let body = self.map_lits(body).into_owned();
        self.start_dir(AspifType::Rule);
        self.write_enum(head_type);
        self.write_slice(&head);
        self.write_enum(BodyType::Normal);
        self.write_slice(&body);
        self.end_dir();
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        let head = self.map_atoms(head).into_owned();
        let body = self.map_weight_lits(body).into_owned();
        self.start_dir(AspifType::Rule);
        self.write_enum(head_type);
        self.write_slice(&head);
        self.write_enum(BodyType::Sum);
        self.write_scalar(bound);
        self.write_weight_lit_slice(&body);
        self.end_dir();
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        let lits = self.map_weight_lits(lits).into_owned();
        self.start_dir(AspifType::Minimize);
        self.write_scalar(priority);
        self.write_weight_lit_slice(&lits);
        self.end_dir();
    }

    fn output_atom(&mut self, atom: Atom, name: &str) {
        potassco_check_pre!(atom != 0, "atom expected");
        let mapped = self.map_atom(atom);
        self.start_dir(AspifType::Output);
        if self.version() == 1 {
            let cond = [lit(mapped)];
            self.write_string(name);
            self.write_slice(&cond);
        } else {
            self.write_enum(OutputType::Atom);
            self.write_scalar(mapped);
            self.write_string(name);
        }
        self.end_dir();
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        if self.version() != 1 {
            self.start_dir(AspifType::Output);
            self.write_enum(OutputType::Term);
            self.write_scalar(term_id);
            self.write_string(name);
            self.end_dir();
        } else {
            self.data_mut().add_term(term_id, name);
        }
    }

    fn output(&mut self, term_id: Id, condition: LitSpan<'_>) {
        let mapped = self.map_lits(condition).into_owned();
        if self.version() != 1 {
            self.start_dir(AspifType::Output);
            self.write_enum(OutputType::Cond);
            self.write_scalar(term_id);
            self.write_slice(&mapped);
            self.end_dir();
            return;
        }
        let index = term_id as usize;
        let term_known = self.data.as_ref().is_some_and(|data| {
            index < data.out_terms.len() && !data.out_terms[index].name.is_empty()
        });
        potassco_check_pre!(term_known, "Undefined: term {} is unknown", term_id);

        if self.data.as_ref().is_some_and(|data| data.true_atom == 0) {
            self.next_atom = self.identity_max + 1;
            let true_atom = self.new_atom();
            self.data_mut().true_atom = true_atom;
            self.aux_rule(true_atom, &[]);
        }

        let true_atom = self.data.as_ref().expect("output data present").true_atom;
        let term_last = self.data.as_ref().expect("output data present").out_terms[index].last;
        let mut term_atom = self.data.as_ref().expect("output data present").out_terms[index].atom;
        let term_name = self.data.as_ref().expect("output data present").out_terms[index]
            .name
            .clone();
        if term_atom == 0 {
            term_atom = self.new_atom();
            self.data_mut().out_terms[index].atom = term_atom;
            let cond = [lit(term_atom), lit(true_atom)];
            self.start_dir(AspifType::Output);
            self.write_string(&term_name);
            self.write_slice(&cond);
            self.end_dir();
        }
        let mut body = Vec::with_capacity(2);
        let mut aux = 0;
        if !mapped.is_empty() {
            if mapped.len() > 1 {
                aux = lit(self.new_atom());
                body.push(aux);
            } else {
                body.push(mapped[0]);
            }
        }
        if term_last != 0 {
            body.push(neg(term_last));
        }
        self.aux_rule(term_atom, &body);
        if aux != 0 {
            self.aux_rule(atom(aux), &mapped);
        }
    }

    fn external(&mut self, atom: Atom, value: TruthValue) {
        let mapped = self.map_atom(atom);
        self.start_dir(AspifType::External);
        self.write_scalar(mapped);
        self.write_enum(value);
        self.end_dir();
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        let lits = self.map_lits(lits).into_owned();
        self.start_dir(AspifType::Assume);
        self.write_slice(&lits);
        self.end_dir();
    }

    fn project(&mut self, atoms: AtomSpan<'_>) {
        let atoms = self.map_atoms(atoms).into_owned();
        self.start_dir(AspifType::Project);
        self.write_slice(&atoms);
        self.end_dir();
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        let condition = self.map_lits(condition).into_owned();
        self.start_dir(AspifType::Edge);
        self.write_scalar(source);
        self.write_scalar(target);
        self.write_slice(&condition);
        self.end_dir();
    }

    fn heuristic(
        &mut self,
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        let mapped_atom = self.map_atom(atom);
        let condition = self.map_lits(condition).into_owned();
        self.start_dir(AspifType::Heuristic);
        self.write_enum(modifier);
        self.write_scalar(mapped_atom);
        self.write_scalar(bias);
        self.write_scalar(priority);
        self.write_slice(&condition);
        self.end_dir();
    }

    fn theory_term_number(&mut self, term_id: Id, number: i32) {
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::Number);
        self.write_scalar(term_id);
        self.write_scalar(number);
        self.end_dir();
    }

    fn theory_term_symbol(&mut self, term_id: Id, name: &str) {
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::Symbol);
        self.write_scalar(term_id);
        self.write_string(name);
        self.end_dir();
    }

    fn theory_term_compound(&mut self, term_id: Id, compound: i32, args: IdSpan<'_>) {
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::Compound);
        self.write_scalar(term_id);
        self.write_scalar(compound);
        self.write_slice(args);
        self.end_dir();
    }

    fn theory_element(&mut self, element_id: Id, terms: IdSpan<'_>, cond: LitSpan<'_>) {
        let cond = self.map_lits(cond).into_owned();
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::Element);
        self.write_scalar(element_id);
        self.write_slice(terms);
        self.write_slice(&cond);
        self.end_dir();
    }

    fn theory_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: IdSpan<'_>) {
        let mapped_atom = self.map_atom(atom_or_zero);
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::Atom);
        self.write_scalar(mapped_atom);
        self.write_scalar(term_id);
        self.write_slice(elements);
        self.end_dir();
    }

    fn theory_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: IdSpan<'_>,
        op: Id,
        rhs: Id,
    ) {
        let mapped_atom = self.map_atom(atom_or_zero);
        self.start_dir(AspifType::Theory);
        self.write_enum(TheoryType::AtomWithGuard);
        self.write_scalar(mapped_atom);
        self.write_scalar(term_id);
        self.write_slice(elements);
        self.write_scalar(op);
        self.write_scalar(rhs);
        self.end_dir();
    }

    fn end_step(&mut self) {
        self.os.write_all(b"0\n").expect("aspif writer failed");
    }
}
