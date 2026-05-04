//! Rust port of original_clasp/libpotassco/potassco/aspif_text.h and
//! original_clasp/libpotassco/src/aspif_text.cpp.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::panic::panic_any;

use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, DomModifier, HeadType, Id, IdSpan, Lit, LitSpan, TruthValue,
    Weight, WeightLit, WeightLitSpan, atom, lit, predicate,
};
use crate::potassco::enums::{EnumTag, enum_name};
use crate::potassco::error::Error;
use crate::potassco::match_basic_types::{
    ProgramReader, ProgramReaderCore, ProgramReaderHooks, ReadMode, is_digit, match_num,
};
use crate::potassco::rule_utils::RuleBuilder;
use crate::potassco::theory_data::{TheoryAtom, TheoryData, TheoryTermType, TupleType, parens};
use crate::potassco_check_pre;

fn is_lower(c: char) -> bool {
    c.is_ascii_lowercase()
}

fn is_alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_atom_prefix(mut input: &str, allow_neg: bool) -> bool {
    if allow_neg {
        input = input.strip_prefix('-').unwrap_or(input);
    }
    let trimmed = input.trim_start_matches('_');
    trimmed.chars().next().is_some_and(is_lower)
}

#[derive(Default)]
struct InputData {
    rule: RuleBuilder,
    symbol: String,
    out_terms: HashMap<String, Id>,
}

impl InputData {
    fn clear_statement(&mut self) {
        self.rule.clear();
        self.symbol.clear();
    }

    fn atoms(&self) -> AtomSpan<'_> {
        self.rule.head()
    }

    fn lits(&self) -> LitSpan<'_> {
        self.rule.body()
    }
}

pub struct AspifTextInput<'a> {
    out: Option<&'a mut dyn AbstractProgram>,
    data: Option<InputData>,
}

impl<'a> AspifTextInput<'a> {
    #[must_use]
    pub fn new(out: &'a mut dyn AbstractProgram) -> Self {
        Self {
            out: Some(out),
            data: None,
        }
    }

    pub fn set_output(&mut self, out: &'a mut dyn AbstractProgram) {
        self.out = Some(out);
    }

    fn data_mut(&mut self) -> &mut InputData {
        self.data
            .as_mut()
            .expect("aspif_text parser state not initialized")
    }

    fn out_mut(&mut self) -> &mut dyn AbstractProgram {
        self.out.as_deref_mut().expect("output not set")
    }

    fn match_opt(&mut self, reader: &mut ProgramReaderCore, token: &str) -> bool {
        if reader.r#match(token) {
            reader.skip_ws();
            true
        } else {
            false
        }
    }

    fn match_delim(&mut self, reader: &mut ProgramReaderCore, expected: char) {
        reader.match_char(expected);
        reader.skip_ws();
    }

    fn match_atoms(&mut self, reader: &mut ProgramReaderCore, separators: &str) {
        if is_lower(reader.skip_ws()) {
            loop {
                let value = self.match_lit(reader);
                reader.require(value > 0, "positive atom expected");
                self.data_mut().rule.add_head(value as Atom);
                let next = reader.peek();
                if !separators.contains(next) {
                    break;
                }
                reader.get();
                reader.skip_ws();
            }
        }
    }

    fn match_lits(&mut self, reader: &mut ProgramReaderCore) {
        if is_lower(reader.skip_ws()) {
            loop {
                let value = self.match_lit(reader);
                self.data_mut().rule.add_goal(value);
                if !self.match_opt(reader, ",") {
                    break;
                }
            }
        }
    }

    fn match_condition(&mut self, reader: &mut ProgramReaderCore) {
        self.data_mut().rule.start_body();
        if self.match_opt(reader, ":") {
            self.match_lits(reader);
        }
    }

    fn match_agg(&mut self, reader: &mut ProgramReaderCore) {
        self.match_delim(reader, '{');
        if !self.match_opt(reader, "}") {
            loop {
                let mut wlit = WeightLit {
                    lit: self.match_lit(reader),
                    weight: 1,
                };
                if self.match_opt(reader, "=") {
                    wlit.weight = self.match_int(reader);
                }
                self.data_mut().rule.add_weight_lit(wlit);
                if !self.match_opt(reader, ",") {
                    break;
                }
            }
            self.match_delim(reader, '}');
        }
    }

    fn match_lit(&mut self, reader: &mut ProgramReaderCore) -> Lit {
        let sign = if self.match_opt(reader, "not ") {
            -1
        } else {
            1
        };
        (self.match_id(reader) as Lit) * sign
    }

    fn match_int(&mut self, reader: &mut ProgramReaderCore) -> i32 {
        let value = reader.match_int("integer expected");
        reader.skip_ws();
        value
    }

    fn match_id(&mut self, reader: &mut ProgramReaderCore) -> Atom {
        let current = reader.get();
        let next = reader.peek();
        reader.require(is_lower(current), "<id> expected");
        reader.require(!is_lower(next), "<pos-integer> expected");
        if current == 'x' && (is_digit(next) || next == '_') {
            if next == '_' {
                reader.get();
            }
            let value = self.match_int(reader);
            reader.require(value > 0, "<pos-integer> expected");
            return value as Atom;
        }
        reader.skip_ws();
        (current as u8 - b'a' + 1) as Atom
    }

    fn push_symbol(&mut self, c: char) {
        self.data_mut().symbol.push(c);
    }

    fn match_term(&mut self, reader: &mut ProgramReaderCore) {
        let c = reader.peek();
        if is_lower(c) || c == '_' {
            loop {
                self.push_symbol(reader.get());
                let next = reader.peek();
                if !(is_alnum(next) || next == '_') {
                    break;
                }
            }
            reader.skip_ws();
            if self.match_opt(reader, "(") {
                self.push_symbol('(');
                loop {
                    self.match_atom_arg(reader);
                    if !self.match_opt(reader, ",") {
                        break;
                    }
                    self.push_symbol(',');
                }
                self.match_delim(reader, ')');
                self.push_symbol(')');
            }
        } else if c == '"' {
            self.match_str(reader);
        } else {
            reader.error("<term> expected");
        }
        reader.skip_ws();
    }

    fn match_atom_arg(&mut self, reader: &mut ProgramReaderCore) {
        let mut paren = 0i32;
        loop {
            let c = reader.peek();
            if c == '\0' {
                break;
            }
            if c == '"' {
                self.match_str(reader);
                continue;
            }
            if (c == ')' && {
                paren -= 1;
                paren < 0
            }) || (c == ',' && paren == 0)
            {
                break;
            }
            if c == '(' {
                paren += 1;
            }
            self.push_symbol(reader.get());
            reader.skip_ws();
        }
    }

    fn match_str(&mut self, reader: &mut ProgramReaderCore) {
        self.match_delim(reader, '"');
        self.push_symbol('"');
        let mut quoted = false;
        loop {
            let c = reader.peek();
            if c == '\0' || (c == '"' && !quoted) {
                break;
            }
            quoted = !quoted && c == '\\';
            self.push_symbol(reader.get());
        }
        self.match_delim(reader, '"');
        self.push_symbol('"');
    }

    fn match_heu_mod(&mut self, reader: &mut ProgramReaderCore) -> DomModifier {
        const MODIFIERS: [DomModifier; 6] = [
            DomModifier::Level,
            DomModifier::Sign,
            DomModifier::Factor,
            DomModifier::Init,
            DomModifier::True,
            DomModifier::False,
        ];
        let first = reader.peek();
        for modifier in MODIFIERS {
            let name = enum_name(modifier);
            if !name.is_empty() && name.starts_with(first) && reader.r#match(name) {
                reader.skip_ws();
                return modifier;
            }
        }
        reader.error("unrecognized heuristic modification")
    }

    fn finish_rule(&mut self) {
        let mut rule = std::mem::take(&mut self.data_mut().rule);
        rule.end(Some(self.out_mut()));
        self.data_mut().rule = rule;
    }

    fn match_rule(&mut self, reader: &mut ProgramReaderCore, first: char) {
        if first == '{' {
            self.match_delim(reader, '{');
            self.data_mut().rule.start_with_type(HeadType::Choice);
            self.match_atoms(reader, ";,");
            self.match_delim(reader, '}');
        } else {
            self.data_mut().rule.start();
            self.match_atoms(reader, ";|");
        }
        if self.match_opt(reader, ":-") {
            let c = reader.skip_ws();
            if !is_digit(c) && c != '-' {
                self.data_mut().rule.start_body();
                self.match_lits(reader);
            } else {
                let bound = self.match_int(reader);
                self.data_mut().rule.start_sum(bound);
                self.match_agg(reader);
            }
        }
        self.match_delim(reader, '.');
        self.finish_rule();
    }

    fn match_directive(&mut self, reader: &mut ProgramReaderCore) -> bool {
        if self.match_opt(reader, "#minimize") {
            self.data_mut().rule.start_minimize(0);
            self.match_agg(reader);
            let prio = if self.match_opt(reader, "@") {
                self.match_int(reader)
            } else {
                0
            };
            self.match_delim(reader, '.');
            self.data_mut().rule.set_bound(prio);
            self.finish_rule();
        } else if self.match_opt(reader, "#project") {
            self.data_mut().rule.start();
            if self.match_opt(reader, "{") {
                self.match_atoms(reader, ",");
                self.match_delim(reader, '}');
            }
            self.match_delim(reader, '.');
            let atoms = self.data_mut().atoms().to_vec();
            self.out_mut().project(&atoms);
        } else if self.match_opt(reader, "#output") {
            self.match_term(reader);
            self.match_condition(reader);
            self.match_delim(reader, '.');
            let symbol = self.data_mut().symbol.clone();
            let lits = self.data_mut().lits().to_vec();
            let cond = if lits.len() == 1 { lits[0] } else { -1 };
            let mut out_atom = cond >= 0 && is_atom_prefix(&symbol, true);
            if self.match_opt(reader, "[") {
                out_atom = false;
                let is_term = self.match_opt(reader, "term");
                reader.require(is_term, "'term' expected");
                self.match_delim(reader, ']');
            }
            if out_atom {
                self.out_mut().output_atom(atom(lits[0]), &symbol);
            } else {
                let next_id = self.data_mut().out_terms.len() as Id;
                let entry = self
                    .data_mut()
                    .out_terms
                    .entry(symbol.clone())
                    .or_insert(next_id);
                let term_id = *entry;
                if term_id == next_id {
                    self.out_mut().output_term(term_id, &symbol);
                }
                self.out_mut().output(term_id, &lits);
            }
        } else if self.match_opt(reader, "#external") {
            let atom_id = self.match_id(reader);
            let mut value = TruthValue::False;
            self.match_delim(reader, '.');
            if self.match_opt(reader, "[") {
                const VALUES: [TruthValue; 4] = [
                    TruthValue::Free,
                    TruthValue::True,
                    TruthValue::False,
                    TruthValue::Release,
                ];
                let mut matched = false;
                for candidate in VALUES {
                    if self.match_opt(reader, enum_name(candidate)) {
                        value = candidate;
                        matched = true;
                        break;
                    }
                }
                reader.require(matched, "<value> expected");
                self.match_delim(reader, ']');
            }
            self.out_mut().external(atom_id, value);
        } else if self.match_opt(reader, "#assume") {
            self.data_mut().rule.start_body();
            if self.match_opt(reader, "{") {
                self.match_lits(reader);
                self.match_delim(reader, '}');
            }
            self.match_delim(reader, '.');
            let lits = self.data_mut().lits().to_vec();
            self.out_mut().assume(&lits);
        } else if self.match_opt(reader, "#heuristic") {
            let atom_id = self.match_id(reader);
            self.match_condition(reader);
            self.match_delim(reader, '.');
            self.match_delim(reader, '[');
            let bias = self.match_int(reader);
            let mut priority = 0;
            if self.match_opt(reader, "@") {
                priority = self.match_int(reader);
                reader.require(priority >= 0, "positive priority expected");
            }
            self.match_delim(reader, ',');
            let modifier = self.match_heu_mod(reader);
            self.match_delim(reader, ']');
            let lits = self.data_mut().lits().to_vec();
            self.out_mut()
                .heuristic(atom_id, modifier, bias, priority as u32, &lits);
        } else if self.match_opt(reader, "#edge") {
            self.match_delim(reader, '(');
            let source = self.match_int(reader);
            self.match_delim(reader, ',');
            let target = self.match_int(reader);
            self.match_delim(reader, ')');
            self.match_condition(reader);
            self.match_delim(reader, '.');
            let lits = self.data_mut().lits().to_vec();
            self.out_mut().acyc_edge(source, target, &lits);
        } else if self.match_opt(reader, "#step") {
            reader.require(reader.incremental(), "#step requires incremental program");
            self.match_delim(reader, '.');
            return false;
        } else if self.match_opt(reader, "#incremental") {
            self.match_delim(reader, '.');
        } else {
            reader.error("unrecognized directive");
        }
        true
    }

    fn parse_statements(&mut self, reader: &mut ProgramReaderCore) {
        self.data = Some(InputData::default());
        loop {
            let current = reader.skip_ws();
            if current == '\0' {
                break;
            }
            self.data_mut().clear_statement();
            if current == '.' {
                self.match_delim(reader, '.');
            } else if current == '#' {
                if !self.match_directive(reader) {
                    break;
                }
            } else if current == '%' {
                reader.skip_line();
            } else {
                self.match_rule(reader, current);
            }
        }
        self.data = None;
    }
}

impl ProgramReaderHooks for AspifTextInput<'_> {
    fn do_attach(&mut self, reader: &mut ProgramReaderCore, incremental: &mut bool) -> bool {
        let mut next = reader.peek();
        if self.out.is_some() && (next == '\0' || is_lower(next) || ".#%{:".contains(next)) {
            while next == '%' {
                reader.skip_line();
                next = reader.skip_ws();
            }
            *incremental = self.match_opt(reader, "#incremental");
            if *incremental {
                self.match_delim(reader, '.');
            }
            self.out_mut().init_program(*incremental);
            return true;
        }
        false
    }

    fn do_parse(&mut self, reader: &mut ProgramReaderCore) -> bool {
        self.out_mut().begin_step();
        self.parse_statements(reader);
        self.out_mut().end_step();
        true
    }
}

pub fn read_aspif_text<R: Read>(input: R, reader: &mut ProgramReader<AspifTextInput<'_>>) {
    if !reader.accept(input) || !reader.parse(ReadMode::Complete) {
        reader.core().error("invalid input format");
    }
}

#[derive(Clone, Debug)]
enum BodyDirective {
    Normal(Vec<Lit>),
    Sum { bound: Weight, lits: Vec<WeightLit> },
    Count { bound: Weight, lits: Vec<Lit> },
}

#[derive(Clone, Debug)]
enum Directive {
    Rule {
        head_type: HeadType,
        head: Vec<Atom>,
        body: BodyDirective,
    },
    Minimize {
        priority: Weight,
        lits: Vec<WeightLit>,
    },
    Project(Vec<Atom>),
    Output {
        name: String,
        cond: Vec<Lit>,
    },
    External {
        atom: Atom,
        value: TruthValue,
    },
    Assume(Vec<Lit>),
    Heuristic {
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: Vec<Lit>,
    },
    Edge {
        source: i32,
        target: i32,
        condition: Vec<Lit>,
    },
}

#[derive(Default)]
struct OutputData {
    directives: Vec<Directive>,
    atom_to_name: HashMap<Atom, String>,
    term_to_name: HashMap<Id, String>,
    conditions: Vec<Vec<Lit>>,
    theory: TheoryData,
    eq: Vec<String>,
    aux_pred: String,
    start_atom: Atom,
    max_gen_atom: Atom,
    max_named_atom: Atom,
    hide: bool,
}

impl OutputData {
    fn new() -> Self {
        Self {
            aux_pred: "x_".to_owned(),
            ..Self::default()
        }
    }

    fn add_theory_condition(&mut self, cond: LitSpan<'_>) -> Id {
        if cond.is_empty() {
            0
        } else {
            self.conditions.push(cond.to_vec());
            self.conditions.len() as Id
        }
    }

    fn theory_condition(&self, id: Id) -> &[Lit] {
        &self.conditions[id as usize - 1]
    }

    fn is_reserved_name(&self, atom_id: Atom, name: &str, arity: u32) -> bool {
        let aux_arity = u32::from(self.aux_pred.ends_with('('));
        if arity == aux_arity && name.starts_with(&self.aux_pred) {
            let mut suffix = &name[self.aux_pred.len()..];
            if arity == 1 {
                if !suffix.ends_with(')') {
                    return true;
                }
                suffix = &suffix[..suffix.len() - 1];
            }
            let mut matched = suffix;
            let mut number = atom_id as i32;
            if !match_num(&mut matched, None, Some(&mut number))
                || !matched.is_empty()
                || number != atom_id as i32
            {
                return true;
            }
        }
        false
    }

    fn add_output_name(&mut self, name: String, cond: LitSpan<'_>) {
        self.directives.push(Directive::Output {
            name,
            cond: cond.to_vec(),
        });
    }

    fn add_output_term(&mut self, term_id: Id, cond: LitSpan<'_>) {
        let name = self.term_to_name.get(&term_id).cloned().unwrap_or_else(|| {
            panic_any(Error::InvalidArgument(format!(
                "Undefined: term {term_id} is undefined"
            )))
        });
        self.add_output_name(name, cond);
    }

    fn assign_atom_name(&mut self, atom_id: Atom, name: &str) -> Option<String> {
        self.max_named_atom = self.max_named_atom.max(atom_id);
        self.atom_to_name.insert(atom_id, name.to_owned())
    }

    fn get_atom_name(&self, atom_id: Atom) -> Option<&str> {
        self.atom_to_name.get(&atom_id).map(String::as_str)
    }

    fn generated_name(&mut self, atom_id: Atom) -> String {
        if self.max_gen_atom == 0 {
            self.hide = true;
        }
        self.max_gen_atom = self.max_gen_atom.max(atom_id);
        if self.aux_pred.ends_with('(') {
            format!("{}{atom_id})", self.aux_pred)
        } else {
            format!("{}{atom_id}", self.aux_pred)
        }
    }

    fn print_name(&mut self, lit_id: Lit, out: &mut String) {
        if lit_id < 0 {
            out.push_str("not ");
        }
        let atom_id = atom(lit_id);
        if let Some(name) = self.get_atom_name(atom_id) {
            out.push_str(name);
        } else {
            out.push_str(&self.generated_name(atom_id));
        }
    }

    fn print_condition(&mut self, cond: &[Lit], out: &mut String, init: &str) {
        let mut sep = init;
        for &lit_id in cond {
            out.push_str(sep);
            self.print_name(lit_id, out);
            sep = ", ";
        }
    }

    fn print_minimize(&mut self, priority: Weight, lits: &[WeightLit], out: &mut String) {
        out.push_str("#minimize{");
        if lits.is_empty() {
            out.push_str(&format!("0@{priority}"));
        } else {
            for (index, wlit) in lits.iter().enumerate() {
                if index != 0 {
                    out.push_str("; ");
                }
                out.push_str(&format!("{}@{},{} : ", wlit.weight, priority, index + 1));
                self.print_name(wlit.lit, out);
            }
        }
        out.push('}');
    }

    fn print_aggregate(&mut self, body: &BodyDirective, out: &mut String) {
        match body {
            BodyDirective::Count { bound, lits } => {
                out.push_str(&format!("{bound} #count{{"));
                for (index, &lit_id) in lits.iter().enumerate() {
                    if index != 0 {
                        out.push_str("; ");
                    }
                    out.push_str(&format!("{} : ", index + 1));
                    self.print_name(lit_id, out);
                }
                out.push('}');
            }
            BodyDirective::Sum { bound, lits } => {
                out.push_str(&format!("{bound} #sum{{"));
                for (index, wlit) in lits.iter().enumerate() {
                    if index != 0 {
                        out.push_str("; ");
                    }
                    out.push_str(&format!("{},{} : ", wlit.weight, index + 1));
                    self.print_name(wlit.lit, out);
                }
                out.push('}');
            }
            BodyDirective::Normal(_) => {}
        }
    }

    fn append_term(&self, term_id: Id, out: &mut String) {
        let term = self.theory.get_term(term_id).expect("term exists");
        match term.term_type() {
            TheoryTermType::Number => out.push_str(&term.number().expect("number").to_string()),
            TheoryTermType::Symbol => out.push_str(term.symbol().expect("symbol")),
            TheoryTermType::Compound => {
                if term.is_function() {
                    let functor = self
                        .theory
                        .get_term(term.function().expect("function id"))
                        .expect("function term exists");
                    let symbol = functor.symbol().expect("function symbol");
                    if term.size() <= 2
                        && "/!<=>+-*\\?&@|:;~^.".contains(symbol.chars().next().unwrap_or('\0'))
                    {
                        let args = term.terms();
                        if args.len() == 2 {
                            self.append_term(args[0], out);
                            out.push(' ');
                            out.push_str(symbol);
                            out.push(' ');
                            self.append_term(args[1], out);
                        } else if let Some(&arg) = args.first() {
                            out.push_str(symbol);
                            self.append_term(arg, out);
                        } else {
                            out.push_str(symbol);
                        }
                        return;
                    }
                    out.push_str(symbol);
                }
                let tuple = if term.is_tuple() {
                    parens(term.tuple().expect("tuple type"))
                } else {
                    parens(TupleType::Paren)
                };
                out.push_str(&tuple[..1]);
                for (index, &arg) in term.terms().iter().enumerate() {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    self.append_term(arg, out);
                }
                out.push_str(&tuple[1..]);
            }
        }
    }

    fn print_theory_atom(&mut self, atom_data: &TheoryAtom, out: &mut String) {
        out.push('&');
        self.append_term(atom_data.term(), out);
        out.push('{');
        let mut outer_sep = "";
        for &element_id in atom_data.elements() {
            out.push_str(outer_sep);
            let element = self.theory.get_element(element_id).expect("element exists");
            let mut inner_sep = "";
            for &term_id in element.terms() {
                out.push_str(inner_sep);
                self.append_term(term_id, out);
                inner_sep = ", ";
            }
            if element.condition() != 0 {
                let condition = self.theory_condition(element.condition()).to_vec();
                self.print_condition(&condition, out, " : ");
            }
            outer_sep = "; ";
        }
        out.push('}');
        if let Some(&guard) = atom_data.guard() {
            out.push(' ');
            self.append_term(guard, out);
        }
        if let Some(&rhs) = atom_data.rhs() {
            out.push(' ');
            self.append_term(rhs, out);
        }
    }

    fn visit_theory_atoms(&mut self, out: &mut String) {
        let atoms = self.theory.curr_atoms().to_vec();
        for atom_data in atoms {
            if atom_data.atom() == 0 {
                self.print_theory_atom(&atom_data, out);
                out.push_str(".\n");
            } else {
                let mut name = String::new();
                self.print_theory_atom(&atom_data, &mut name);
                potassco_check_pre!(
                    atom_data.atom() >= self.start_atom,
                    "Redefinition: theory atom '{}:{}' already defined in a previous step",
                    atom_data.atom(),
                    name
                );
                let previous = self.assign_atom_name(atom_data.atom(), &name);
                potassco_check_pre!(
                    previous.is_none(),
                    "Redefinition: theory atom '{}:{}' already defined as {}",
                    atom_data.atom(),
                    name,
                    previous.unwrap_or_default()
                );
            }
        }
    }

    fn show_atom(&mut self, name: &str, seen: &mut HashSet<String>, out: &mut String) {
        let (id, arity) = predicate(name);
        if arity <= 0 {
            potassco_check_pre!(arity == 0, "invalid predicate");
            out.push_str(&format!("#show {id}/0.\n"));
        } else {
            let pred = format!("{id}/{arity}");
            if seen.insert(pred.clone()) {
                out.push_str(&format!("#show {pred}.\n"));
            }
        }
        self.hide = false;
    }

    fn end_step(&mut self, more: bool) -> String {
        let mut out = String::new();
        self.visit_theory_atoms(&mut out);
        let directives = std::mem::take(&mut self.directives);
        for directive in directives {
            match directive {
                Directive::Rule {
                    head_type,
                    head,
                    body,
                } => {
                    let has_head = !head.is_empty() || head_type == HeadType::Choice;
                    if head_type == HeadType::Choice {
                        out.push('{');
                    }
                    if !head.is_empty() {
                        let sep = if head_type == HeadType::Choice {
                            ";"
                        } else {
                            "|"
                        };
                        for (index, atom_id) in head.into_iter().enumerate() {
                            if index != 0 {
                                out.push_str(sep);
                            }
                            self.print_name(lit(atom_id), &mut out);
                        }
                    }
                    if head_type == HeadType::Choice {
                        out.push('}');
                    }
                    let needs_body = match &body {
                        BodyDirective::Normal(cond) => !cond.is_empty() || !has_head,
                        BodyDirective::Sum { .. } | BodyDirective::Count { .. } => true,
                    };
                    if needs_body {
                        out.push_str(if has_head { " :- " } else { ":- " });
                    }
                    match body {
                        BodyDirective::Normal(cond) => {
                            if needs_body {
                                self.print_condition(&cond, &mut out, "");
                            }
                        }
                        aggregate => self.print_aggregate(&aggregate, &mut out),
                    }
                    out.push_str(".\n");
                }
                Directive::Minimize { priority, lits } => {
                    self.print_minimize(priority, &lits, &mut out);
                    out.push_str(".\n");
                }
                Directive::Project(atoms) => {
                    out.push_str("#project{");
                    let cond = atoms.into_iter().map(lit).collect::<Vec<_>>();
                    self.print_condition(&cond, &mut out, "");
                    out.push_str("}.\n");
                }
                Directive::Output { name, cond } => {
                    out.push_str("#show ");
                    out.push_str(&name);
                    self.print_condition(&cond, &mut out, " : ");
                    out.push_str(".\n");
                }
                Directive::External { atom, value } => {
                    out.push_str("#external ");
                    self.print_name(lit(atom), &mut out);
                    if value != TruthValue::False {
                        out.push_str(&format!(". [{}]\n", enum_name(value)));
                    } else {
                        out.push_str(".\n");
                    }
                }
                Directive::Assume(lits) => {
                    out.push_str("#assume{");
                    self.print_condition(&lits, &mut out, "");
                    out.push_str("}.\n");
                }
                Directive::Heuristic {
                    atom,
                    modifier,
                    bias,
                    priority,
                    condition,
                } => {
                    out.push_str("#heuristic ");
                    self.print_name(lit(atom), &mut out);
                    self.print_condition(&condition, &mut out, " : ");
                    out.push_str(&format!(". [{bias}"));
                    if priority != 0 {
                        out.push_str(&format!("@{priority}"));
                    }
                    out.push_str(&format!(", {}]\n", enum_name(modifier)));
                }
                Directive::Edge {
                    source,
                    target,
                    condition,
                } => {
                    out.push_str(&format!("#edge({source},{target})"));
                    self.print_condition(&condition, &mut out, " : ");
                    out.push_str(".\n");
                }
            }
        }
        if self.max_gen_atom != 0 {
            let mut seen = HashSet::new();
            if self.start_atom <= self.max_named_atom {
                for atom_id in self.start_atom..=self.max_named_atom {
                    if let Some(name) = self.get_atom_name(atom_id).map(str::to_owned) {
                        if !name.starts_with('&') {
                            self.show_atom(&name, &mut seen, &mut out);
                        }
                    }
                }
            }
            let eq = std::mem::take(&mut self.eq);
            for name in eq {
                self.show_atom(&name, &mut seen, &mut out);
            }
            if std::mem::replace(&mut self.hide, false) {
                out.push_str("#show.\n");
            }
        }
        if !more {
            self.theory.reset();
            self.conditions.clear();
        }
        out
    }
}

pub struct AspifTextOutput<'a> {
    writer: &'a mut dyn Write,
    data: OutputData,
    step: i32,
}

impl<'a> AspifTextOutput<'a> {
    #[must_use]
    pub fn new(writer: &'a mut dyn Write) -> Self {
        Self {
            writer,
            data: OutputData::new(),
            step: -2,
        }
    }

    pub fn set_atom_pred(&mut self, pred: &str) {
        let mut arity = 0u32;
        let mut name = pred;
        if let Some(pos) = pred.rfind('/') {
            potassco_check_pre!(pos == pred.len() - 2, "invalid atom predicate arity");
            let suffix = pred.as_bytes()[pred.len() - 1];
            potassco_check_pre!(
                suffix == b'0' || suffix == b'1',
                "invalid atom predicate arity"
            );
            arity = u32::from(suffix == b'1');
            name = &pred[..pos];
        }
        let (id, parsed_arity) = predicate(name);
        potassco_check_pre!(
            parsed_arity == 0 && id == name && is_atom_prefix(id, false),
            "invalid atom predicate '{}'",
            pred
        );
        self.data.aux_pred = if arity == 0 {
            id.to_owned()
        } else {
            format!("{id}(")
        };
    }

    fn write_now(&mut self, text: &str) {
        self.writer
            .write_all(text.as_bytes())
            .expect("aspif_text writer failed");
    }
}

impl AbstractProgram for AspifTextOutput<'_> {
    fn init_program(&mut self, incremental: bool) {
        if self.step != -2 {
            self.data = OutputData::new();
        }
        self.step = if incremental { 0 } else { -1 };
    }

    fn begin_step(&mut self) {
        if self.step >= 0 {
            if self.step != 0 {
                self.write_now(&format!("% #program step({}).\n", self.step));
                self.data.theory.update();
            } else {
                self.write_now("% #program base.\n");
            }
            self.step += 1;
            self.data.start_atom = self.data.max_named_atom.max(self.data.max_gen_atom) + 1;
        }
    }

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        self.data.directives.push(Directive::Rule {
            head_type,
            head: head.to_vec(),
            body: BodyDirective::Normal(body.to_vec()),
        });
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        lits: WeightLitSpan<'_>,
    ) {
        if lits.is_empty() {
            self.rule(head_type, head, &[]);
            return;
        }
        if lits.windows(2).any(|pair| pair[0].weight != pair[1].weight) {
            self.data.directives.push(Directive::Rule {
                head_type,
                head: head.to_vec(),
                body: BodyDirective::Sum {
                    bound,
                    lits: lits.to_vec(),
                },
            });
        } else {
            let normalized = (bound + lits[0].weight - 1) / lits[0].weight;
            self.data.directives.push(Directive::Rule {
                head_type,
                head: head.to_vec(),
                body: BodyDirective::Count {
                    bound: normalized,
                    lits: lits.iter().map(|wlit| wlit.lit).collect(),
                },
            });
        }
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        self.data.directives.push(Directive::Minimize {
            priority,
            lits: lits.to_vec(),
        });
    }

    fn output_atom(&mut self, atom_id: Atom, name: &str) {
        potassco_check_pre!(atom_id != 0, "atom expected");
        if !is_atom_prefix(name, true) {
            self.data.add_output_name(name.to_owned(), &[lit(atom_id)]);
            return;
        }
        let (pred_name, arity) = predicate(name);
        potassco_check_pre!(arity >= 0, "invalid atom name <{}:{}>", atom_id, name);
        if self.data.is_reserved_name(atom_id, name, arity as u32) {
            panic_any(Error::InvalidArgument(format!(
                "atom name <{atom_id}:{name}> is reserved"
            )));
        }
        let canonical = if arity == 0 { pred_name } else { name };
        if let Some(previous) = self.data.assign_atom_name(atom_id, canonical) {
            if previous != canonical {
                self.write_now(&format!("{canonical} :- {previous}.\n"));
                self.data.eq.push(canonical.to_owned());
                self.data.atom_to_name.insert(atom_id, previous);
            }
        }
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        if let Some(existing) = self.data.term_to_name.insert(term_id, name.to_owned()) {
            potassco_check_pre!(
                existing == name,
                "Redefinition: term {} already defined as {}",
                term_id,
                existing
            );
            self.data.term_to_name.insert(term_id, existing);
        }
    }

    fn output(&mut self, term_id: Id, condition: LitSpan<'_>) {
        self.data.add_output_term(term_id, condition);
    }

    fn project(&mut self, atoms: AtomSpan<'_>) {
        self.data
            .directives
            .push(Directive::Project(atoms.to_vec()));
    }

    fn external(&mut self, atom_id: Atom, value: TruthValue) {
        self.data.directives.push(Directive::External {
            atom: atom_id,
            value,
        });
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        self.data.directives.push(Directive::Assume(lits.to_vec()));
    }

    fn heuristic(
        &mut self,
        atom_id: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        self.data.directives.push(Directive::Heuristic {
            atom: atom_id,
            modifier,
            bias,
            priority,
            condition: condition.to_vec(),
        });
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        self.data.directives.push(Directive::Edge {
            source,
            target,
            condition: condition.to_vec(),
        });
    }

    fn theory_term_number(&mut self, term_id: Id, number: i32) {
        self.data.theory.add_term_number(term_id, number);
    }

    fn theory_term_symbol(&mut self, term_id: Id, name: &str) {
        self.data.theory.add_term_symbol(term_id, name);
    }

    fn theory_term_compound(&mut self, term_id: Id, compound: i32, args: IdSpan<'_>) {
        if compound >= 0 {
            self.data
                .theory
                .add_term_function(term_id, compound as Id, args);
        } else {
            let tuple = TupleType::from_underlying(compound).unwrap_or_else(|| {
                panic_any(Error::InvalidArgument(format!(
                    "invalid tuple type {compound}"
                )))
            });
            self.data.theory.add_term_tuple(term_id, tuple, args);
        }
    }

    fn theory_element(&mut self, element_id: Id, terms: IdSpan<'_>, cond: LitSpan<'_>) {
        let cond_id = self.data.add_theory_condition(cond);
        self.data.theory.add_element(element_id, terms, cond_id);
    }

    fn theory_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: IdSpan<'_>) {
        self.data.theory.add_atom(atom_or_zero, term_id, elements);
    }

    fn theory_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: IdSpan<'_>,
        op: Id,
        rhs: Id,
    ) {
        self.data
            .theory
            .add_atom_guarded(atom_or_zero, term_id, elements, op, rhs);
    }

    fn end_step(&mut self) {
        let rendered = self.data.end_step(self.step >= 0);
        self.write_now(&rendered);
    }
}
