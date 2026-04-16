//! Rust port of original_clasp/libpotassco/potassco/reify.h and
//! original_clasp/libpotassco/src/reify.cpp.

use std::collections::HashMap;
use std::hash::Hash;
use std::io::{Read, Write};

use crate::potassco::aspif::read_aspif;
use crate::potassco::basic_types::{
    AbstractProgram, Atom, AtomSpan, DomModifier, HeadType, Id, IdSpan, Lit, LitSpan, TruthValue,
    Weight, WeightLit, WeightLitSpan, atom, lit,
};
use crate::potassco::enums::enum_name;
use crate::potassco::graph::Graph;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReifierOptions {
    pub calculate_sccs: bool,
    pub reify_step: bool,
}

#[derive(Clone, Copy)]
struct Head {
    head_type: HeadType,
    id: usize,
}

#[derive(Clone, Copy)]
struct Normal {
    id: usize,
}

#[derive(Clone, Copy)]
struct Sum {
    id: usize,
    bound: Weight,
}

#[derive(Default)]
struct StepData {
    theory_tuples: HashMap<Vec<Id>, usize>,
    theory_element_tuples: HashMap<Vec<Id>, usize>,
    lit_tuples: HashMap<Vec<Lit>, usize>,
    atom_tuples: HashMap<Vec<Atom>, usize>,
    weight_lit_tuples: HashMap<Vec<WeightLit>, usize>,
    graph: Graph<Atom>,
    nodes: HashMap<Atom, u32>,
}

impl StepData {
    fn clear(&mut self) {
        self.theory_tuples.clear();
        self.theory_element_tuples.clear();
        self.lit_tuples.clear();
        self.atom_tuples.clear();
        self.weight_lit_tuples.clear();
        self.graph.clear();
        self.nodes.clear();
    }
}

pub struct Reifier<W: Write> {
    out: W,
    step_data: StepData,
    step: usize,
    calculate_sccs: bool,
    reify_step: bool,
}

trait TupleValue: Copy + Eq + Hash + Ord {
    fn render(self) -> String;
}

impl TupleValue for u32 {
    fn render(self) -> String {
        self.to_string()
    }
}

impl TupleValue for i32 {
    fn render(self) -> String {
        self.to_string()
    }
}

impl TupleValue for WeightLit {
    fn render(self) -> String {
        format!("{},{}", self.lit, self.weight)
    }
}

impl<W: Write> Reifier<W> {
    #[must_use]
    pub fn new(out: W, opts: ReifierOptions) -> Self {
        Self {
            out,
            step_data: StepData::default(),
            step: 0,
            calculate_sccs: opts.calculate_sccs,
            reify_step: opts.reify_step,
        }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn parse<R: Read>(&mut self, input: R) -> i32 {
        read_aspif(input, self)
    }

    fn print_fact(&mut self, name: &str, args: &[String]) {
        self.out
            .write_all(name.as_bytes())
            .expect("reifier write failed");
        self.out.write_all(b"(").expect("reifier write failed");
        if let Some((first, rest)) = args.split_first() {
            self.out
                .write_all(first.as_bytes())
                .expect("reifier write failed");
            for arg in rest {
                self.out.write_all(b",").expect("reifier write failed");
                self.out
                    .write_all(arg.as_bytes())
                    .expect("reifier write failed");
            }
        }
        self.out.write_all(b").\n").expect("reifier write failed");
    }

    fn print_step_fact(&mut self, name: &str, mut args: Vec<String>) {
        if self.reify_step {
            args.push(self.step.to_string());
        }
        self.print_fact(name, &args);
    }

    fn insert_tuple<T>(map: &mut HashMap<Vec<T>, usize>, args: &[T]) -> (usize, Option<Vec<T>>)
    where
        T: TupleValue,
    {
        let mut owned = args.to_vec();
        owned.sort();
        owned.dedup();
        if let Some(&id) = map.get(&owned) {
            return (id, None);
        }

        let id = map.len();
        map.insert(owned.clone(), id);
        (id, Some(owned))
    }

    fn theory_tuple(&mut self, args: IdSpan<'_>) -> usize {
        let owned = args.to_vec();
        if let Some(&id) = self.step_data.theory_tuples.get(&owned) {
            return id;
        }

        let id = self.step_data.theory_tuples.len();
        self.print_step_fact("theory_tuple", vec![id.to_string()]);
        for (index, value) in owned.iter().enumerate() {
            self.print_step_fact(
                "theory_tuple",
                vec![id.to_string(), index.to_string(), value.to_string()],
            );
        }
        self.step_data.theory_tuples.insert(owned, id);
        id
    }

    fn theory_element_tuple(&mut self, args: IdSpan<'_>) -> usize {
        let (id, values) = Self::insert_tuple(&mut self.step_data.theory_element_tuples, args);
        if let Some(values) = values {
            self.print_step_fact("theory_element_tuple", vec![id.to_string()]);
            for value in values {
                self.print_step_fact("theory_element_tuple", vec![id.to_string(), value.render()]);
            }
        }
        id
    }

    fn lit_tuple(&mut self, args: LitSpan<'_>) -> usize {
        let (id, values) = Self::insert_tuple(&mut self.step_data.lit_tuples, args);
        if let Some(values) = values {
            self.print_step_fact("literal_tuple", vec![id.to_string()]);
            for value in values {
                self.print_step_fact("literal_tuple", vec![id.to_string(), value.render()]);
            }
        }
        id
    }

    fn atom_tuple(&mut self, args: AtomSpan<'_>) -> usize {
        let (id, values) = Self::insert_tuple(&mut self.step_data.atom_tuples, args);
        if let Some(values) = values {
            self.print_step_fact("atom_tuple", vec![id.to_string()]);
            for value in values {
                self.print_step_fact("atom_tuple", vec![id.to_string(), value.render()]);
            }
        }
        id
    }

    fn weight_lit_tuple(&mut self, args: WeightLitSpan<'_>) -> usize {
        let (id, values) = Self::insert_tuple(&mut self.step_data.weight_lit_tuples, args);
        if let Some(values) = values {
            self.print_step_fact("weighted_literal_tuple", vec![id.to_string()]);
            for value in values {
                self.print_step_fact(
                    "weighted_literal_tuple",
                    vec![id.to_string(), value.render()],
                );
            }
        }
        id
    }

    fn add_node(&mut self, atom: Atom) -> u32 {
        if let Some(&id) = self.step_data.nodes.get(&atom) {
            return id;
        }
        let id = self.step_data.graph.add_node(atom);
        self.step_data.nodes.insert(atom, id);
        id
    }

    fn calculate_sccs<L>(&mut self, head: AtomSpan<'_>, body: &[L])
    where
        L: crate::potassco::basic_types::AtomOf + crate::potassco::basic_types::LitOf + Copy,
    {
        for &head_atom in head {
            let head_id = self.add_node(head_atom);
            for &body_atom in body {
                if lit(body_atom) > 0 {
                    let body_id = self.add_node(atom(body_atom));
                    self.step_data.graph.add_edge(head_id, body_id);
                }
            }
        }
    }
}

fn render_head(head: Head) -> String {
    let name = if head.head_type == HeadType::Disjunctive {
        "disjunction"
    } else {
        "choice"
    };
    format!("{name}({})", head.id)
}

fn render_normal(value: Normal) -> String {
    format!("normal({})", value.id)
}

fn render_sum(value: Sum) -> String {
    format!("sum({},{})", value.id, value.bound)
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

impl<W: Write> AbstractProgram for Reifier<W> {
    fn init_program(&mut self, incremental: bool) {
        if incremental {
            self.print_fact("tag", &[String::from("incremental")]);
        }
    }

    fn begin_step(&mut self) {}

    fn rule(&mut self, head_type: HeadType, head: AtomSpan<'_>, body: LitSpan<'_>) {
        let head_id = self.atom_tuple(head);
        let body_id = self.lit_tuple(body);
        self.print_step_fact(
            "rule",
            vec![
                render_head(Head {
                    head_type,
                    id: head_id,
                }),
                render_normal(Normal { id: body_id }),
            ],
        );
        if self.calculate_sccs {
            self.calculate_sccs(head, body);
        }
    }

    fn rule_weighted(
        &mut self,
        head_type: HeadType,
        head: AtomSpan<'_>,
        bound: Weight,
        body: WeightLitSpan<'_>,
    ) {
        let head_id = self.atom_tuple(head);
        let body_id = self.weight_lit_tuple(body);
        self.print_step_fact(
            "rule",
            vec![
                render_head(Head {
                    head_type,
                    id: head_id,
                }),
                render_sum(Sum { id: body_id, bound }),
            ],
        );
        if self.calculate_sccs {
            self.calculate_sccs(head, body);
        }
    }

    fn minimize(&mut self, priority: Weight, lits: WeightLitSpan<'_>) {
        let tuple_id = self.weight_lit_tuple(lits);
        self.print_step_fact("minimize", vec![priority.to_string(), tuple_id.to_string()]);
    }

    fn output_atom(&mut self, atom: Atom, name: &str) {
        self.print_step_fact("outputAtom", vec![name.to_owned(), atom.to_string()]);
    }

    fn output_term(&mut self, term_id: Id, name: &str) {
        self.print_step_fact("outputTerm", vec![name.to_owned(), term_id.to_string()]);
    }

    fn output(&mut self, term_id: Id, condition: LitSpan<'_>) {
        let tuple_id = self.lit_tuple(condition);
        self.print_step_fact("output", vec![term_id.to_string(), tuple_id.to_string()]);
    }

    fn project(&mut self, atoms: AtomSpan<'_>) {
        for &value in atoms {
            self.print_step_fact("project", vec![value.to_string()]);
        }
    }

    fn external(&mut self, atom: Atom, value: TruthValue) {
        self.print_step_fact(
            "external",
            vec![atom.to_string(), enum_name(value).to_string()],
        );
    }

    fn assume(&mut self, lits: LitSpan<'_>) {
        for &value in lits {
            self.print_step_fact("assume", vec![value.to_string()]);
        }
    }

    fn heuristic(
        &mut self,
        atom: Atom,
        modifier: DomModifier,
        bias: i32,
        priority: u32,
        condition: LitSpan<'_>,
    ) {
        let tuple_id = self.lit_tuple(condition);
        self.print_step_fact(
            "heuristic",
            vec![
                atom.to_string(),
                enum_name(modifier).to_string(),
                bias.to_string(),
                priority.to_string(),
                tuple_id.to_string(),
            ],
        );
    }

    fn acyc_edge(&mut self, source: i32, target: i32, condition: LitSpan<'_>) {
        let tuple_id = self.lit_tuple(condition);
        self.print_step_fact(
            "edge",
            vec![source.to_string(), target.to_string(), tuple_id.to_string()],
        );
    }

    fn theory_term_number(&mut self, term_id: Id, number: i32) {
        self.print_step_fact(
            "theory_number",
            vec![term_id.to_string(), number.to_string()],
        );
    }

    fn theory_term_symbol(&mut self, term_id: Id, name: &str) {
        self.print_step_fact("theory_string", vec![term_id.to_string(), quote(name)]);
    }

    fn theory_term_compound(&mut self, term_id: Id, compound: i32, args: IdSpan<'_>) {
        if compound >= 0 {
            let tuple_id = self.theory_tuple(args);
            self.print_step_fact(
                "theory_function",
                vec![
                    term_id.to_string(),
                    compound.to_string(),
                    tuple_id.to_string(),
                ],
            );
            return;
        }

        let ty = match compound {
            -1 => "tuple",
            -2 => "set",
            -3 => "list",
            _ => panic!("unexpected tuple type"),
        };
        let tuple_id = self.theory_tuple(args);
        self.print_step_fact(
            "theory_sequence",
            vec![term_id.to_string(), ty.to_owned(), tuple_id.to_string()],
        );
    }

    fn theory_element(&mut self, element_id: Id, terms: IdSpan<'_>, cond: LitSpan<'_>) {
        let term_tuple = self.theory_tuple(terms);
        let lit_tuple = self.lit_tuple(cond);
        self.print_step_fact(
            "theory_element",
            vec![
                element_id.to_string(),
                term_tuple.to_string(),
                lit_tuple.to_string(),
            ],
        );
    }

    fn theory_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: IdSpan<'_>) {
        let tuple_id = self.theory_element_tuple(elements);
        self.print_step_fact(
            "theory_atom",
            vec![
                atom_or_zero.to_string(),
                term_id.to_string(),
                tuple_id.to_string(),
            ],
        );
    }

    fn theory_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: IdSpan<'_>,
        op: Id,
        rhs: Id,
    ) {
        let tuple_id = self.theory_element_tuple(elements);
        self.print_step_fact(
            "theory_atom",
            vec![
                atom_or_zero.to_string(),
                term_id.to_string(),
                tuple_id.to_string(),
                op.to_string(),
                rhs.to_string(),
            ],
        );
    }

    fn end_step(&mut self) {
        for (index, scc) in self
            .step_data
            .graph
            .compute_non_trivial_sccs()
            .into_iter()
            .enumerate()
        {
            for atom in scc.into_iter().rev() {
                self.print_step_fact("scc", vec![index.to_string(), atom.to_string()]);
            }
        }
        if self.reify_step {
            self.step_data.clear();
            self.step += 1;
        }
    }
}
