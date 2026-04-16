//! Rust port of original_clasp/libpotassco/potassco/theory_data.h and
//! original_clasp/libpotassco/src/theory_data.cpp.

use crate::potassco::basic_types::{AbstractProgram, Id, IdSpan};
use crate::potassco::enums::{DefaultEnum, EnumMetadata, EnumTag};
use crate::potassco::error::Error;
use crate::{potassco_assert_not_reached, potassco_check_pre};

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TheoryTermType {
    Number = 0,
    Symbol = 1,
    Compound = 2,
}

impl EnumTag for TheoryTermType {
    type Repr = u32;

    fn to_underlying(self) -> Self::Repr {
        self as u32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            0 => Some(Self::Number),
            1 => Some(Self::Symbol),
            2 => Some(Self::Compound),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Default(DefaultEnum::new(3)))
    }
}

#[repr(i32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TupleType {
    Bracket = -3,
    Brace = -2,
    Paren = -1,
}

impl EnumTag for TupleType {
    type Repr = i32;

    fn to_underlying(self) -> Self::Repr {
        self as i32
    }

    fn from_underlying(value: Self::Repr) -> Option<Self> {
        match value {
            -3 => Some(Self::Bracket),
            -2 => Some(Self::Brace),
            -1 => Some(Self::Paren),
            _ => None,
        }
    }

    fn metadata() -> Option<EnumMetadata<Self>> {
        Some(EnumMetadata::Default(DefaultEnum::new_with_first(3, -3)))
    }
}

#[must_use]
pub const fn parens(value: TupleType) -> &'static str {
    match value {
        TupleType::Bracket => "[]",
        TupleType::Brace => "{}",
        TupleType::Paren => "()",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TheoryTermData {
    Number(i32),
    Symbol(Box<str>),
    Compound { base: i32, args: Vec<Id> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheoryTerm {
    data: TheoryTermData,
}

impl TheoryTerm {
    fn number_term(value: i32) -> Self {
        Self {
            data: TheoryTermData::Number(value),
        }
    }

    fn symbol_term(value: &str) -> Self {
        Self {
            data: TheoryTermData::Symbol(value.into()),
        }
    }

    fn compound_term(base: i32, args: IdSpan<'_>) -> Self {
        Self {
            data: TheoryTermData::Compound {
                base,
                args: args.to_vec(),
            },
        }
    }

    #[must_use]
    pub fn term_type(&self) -> TheoryTermType {
        match self.data {
            TheoryTermData::Number(_) => TheoryTermType::Number,
            TheoryTermData::Symbol(_) => TheoryTermType::Symbol,
            TheoryTermData::Compound { .. } => TheoryTermType::Compound,
        }
    }

    pub fn number(&self) -> Result<i32, Error> {
        match self.data {
            TheoryTermData::Number(value) => Ok(value),
            _ => Err(Error::InvalidArgument("Term is not a number".to_owned())),
        }
    }

    pub fn symbol(&self) -> Result<&str, Error> {
        match &self.data {
            TheoryTermData::Symbol(value) => Ok(value),
            _ => Err(Error::InvalidArgument("Term is not a symbol".to_owned())),
        }
    }

    pub fn compound(&self) -> Result<i32, Error> {
        match self.data {
            TheoryTermData::Compound { base, .. } => Ok(base),
            _ => Err(Error::InvalidArgument("Term is not a compound".to_owned())),
        }
    }

    #[must_use]
    pub fn is_function(&self) -> bool {
        matches!(self.data, TheoryTermData::Compound { base, .. } if base >= 0)
    }

    pub fn function(&self) -> Result<Id, Error> {
        match self.data {
            TheoryTermData::Compound { base, .. } if base >= 0 => Ok(base as Id),
            _ => Err(Error::InvalidArgument("Term is not a function".to_owned())),
        }
    }

    #[must_use]
    pub fn is_tuple(&self) -> bool {
        matches!(self.data, TheoryTermData::Compound { base, .. } if base < 0)
    }

    pub fn tuple(&self) -> Result<TupleType, Error> {
        match self.data {
            TheoryTermData::Compound { base, .. } if base < 0 => TupleType::from_underlying(base)
                .ok_or_else(|| Error::InvalidArgument("Term is not a tuple".to_owned())),
            _ => Err(Error::InvalidArgument("Term is not a tuple".to_owned())),
        }
    }

    #[must_use]
    pub fn size(&self) -> u32 {
        match &self.data {
            TheoryTermData::Compound { args, .. } => args.len() as u32,
            _ => 0,
        }
    }

    #[must_use]
    pub fn terms(&self) -> &[Id] {
        match &self.data {
            TheoryTermData::Compound { args, .. } => args,
            _ => &[],
        }
    }

    pub fn begin(&self) -> core::slice::Iter<'_, Id> {
        self.terms().iter()
    }

    pub fn end(&self) -> core::slice::Iter<'_, Id> {
        self.terms()[self.terms().len()..].iter()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheoryElement {
    terms: Vec<Id>,
    condition: Option<Id>,
}

impl TheoryElement {
    fn new(terms: IdSpan<'_>, condition: Option<Id>) -> Self {
        Self {
            terms: terms.to_vec(),
            condition,
        }
    }

    #[must_use]
    pub fn size(&self) -> u32 {
        self.terms.len() as u32
    }

    #[must_use]
    pub fn terms(&self) -> &[Id] {
        &self.terms
    }

    pub fn begin(&self) -> core::slice::Iter<'_, Id> {
        self.terms.iter()
    }

    pub fn end(&self) -> core::slice::Iter<'_, Id> {
        self.terms[self.terms.len()..].iter()
    }

    #[must_use]
    pub fn condition(&self) -> Id {
        self.condition.unwrap_or(0)
    }

    fn set_condition(&mut self, condition: Id) {
        self.condition = Some(condition);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheoryAtom {
    atom: Id,
    term_id: Id,
    elements: Vec<Id>,
    guard: Option<Id>,
    rhs: Option<Id>,
}

impl TheoryAtom {
    fn new(
        atom: Id,
        term_id: Id,
        elements: IdSpan<'_>,
        guard: Option<Id>,
        rhs: Option<Id>,
    ) -> Self {
        Self {
            atom,
            term_id,
            elements: elements.to_vec(),
            guard,
            rhs,
        }
    }

    #[must_use]
    pub fn atom(&self) -> Id {
        self.atom
    }

    #[must_use]
    pub fn term(&self) -> Id {
        self.term_id
    }

    #[must_use]
    pub fn size(&self) -> u32 {
        self.elements.len() as u32
    }

    #[must_use]
    pub fn elements(&self) -> &[Id] {
        &self.elements
    }

    pub fn begin(&self) -> core::slice::Iter<'_, Id> {
        self.elements.iter()
    }

    pub fn end(&self) -> core::slice::Iter<'_, Id> {
        self.elements[self.elements.len()..].iter()
    }

    #[must_use]
    pub fn guard(&self) -> Option<&Id> {
        self.guard.as_ref()
    }

    #[must_use]
    pub fn rhs(&self) -> Option<&Id> {
        self.rhs.as_ref()
    }
}

pub type AtomView<'a> = &'a [TheoryAtom];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VisitMode {
    All,
    Current,
}

pub trait Visitor {
    fn visit_term(&mut self, data: &TheoryData, term_id: Id, term: &TheoryTerm);
    fn visit_element(&mut self, data: &TheoryData, element_id: Id, element: &TheoryElement);
    fn visit_atom(&mut self, data: &TheoryData, atom: &TheoryAtom);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UpdateFrame {
    atom: usize,
    term: usize,
    element: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TheoryData {
    atoms: Vec<TheoryAtom>,
    elements: Vec<Option<TheoryElement>>,
    terms: Vec<Option<TheoryTerm>>,
    frame: UpdateFrame,
}

impl TheoryData {
    pub const COND_DEFERRED: Id = Id::MAX;

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.atoms.clear();
        self.elements.clear();
        self.terms.clear();
        self.frame = UpdateFrame::default();
    }

    pub fn update(&mut self) {
        self.frame.atom = self.atoms.len();
        self.frame.term = self.terms.len();
        self.frame.element = self.elements.len();
    }

    pub fn add_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: IdSpan<'_>) {
        self.atoms
            .push(TheoryAtom::new(atom_or_zero, term_id, elements, None, None));
    }

    pub fn add_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: IdSpan<'_>,
        op: Id,
        rhs: Id,
    ) {
        self.atoms.push(TheoryAtom::new(
            atom_or_zero,
            term_id,
            elements,
            Some(op),
            Some(rhs),
        ));
    }

    pub fn add_element(&mut self, element_id: Id, terms: IdSpan<'_>, condition: Id) {
        let index = element_id as usize;
        if index >= self.elements.len() {
            self.elements.resize(index + 1, None);
        } else if self.elements[index].is_some() {
            potassco_check_pre!(
                !self.is_new_element(element_id),
                "Redefinition of theory element '{}'",
                element_id
            );
        }
        let condition = if condition == 0 {
            None
        } else {
            Some(condition)
        };
        self.elements[index] = Some(TheoryElement::new(terms, condition));
    }

    pub fn set_condition(&mut self, element_id: Id, new_cond: Id) {
        let element = self
            .elements
            .get_mut(element_id as usize)
            .and_then(Option::as_mut)
            .expect("element existence checked by get_element");
        potassco_check_pre!(element.condition() == Self::COND_DEFERRED);
        element.set_condition(new_cond);
    }

    pub fn add_term_number(&mut self, term_id: Id, number: i32) {
        self.set_term(term_id, TheoryTerm::number_term(number));
    }

    pub fn add_term_symbol(&mut self, term_id: Id, name: &str) {
        self.set_term(term_id, TheoryTerm::symbol_term(name));
    }

    pub fn add_term_function(&mut self, term_id: Id, function_id: Id, args: IdSpan<'_>) {
        self.set_term(term_id, TheoryTerm::compound_term(function_id as i32, args));
    }

    pub fn add_term_tuple(&mut self, term_id: Id, tuple_type: TupleType, args: IdSpan<'_>) {
        self.set_term(term_id, TheoryTerm::compound_term(tuple_type as i32, args));
    }

    pub fn remove_term(&mut self, term_id: Id) {
        if self.has_term(term_id) {
            self.terms[term_id as usize] = None;
        }
    }

    #[must_use]
    pub fn empty(&self) -> bool {
        self.terms.iter().all(Option::is_none)
            && self.elements.iter().all(Option::is_none)
            && self.atoms.is_empty()
    }

    #[must_use]
    pub fn num_terms(&self) -> u32 {
        self.terms.len() as u32
    }

    #[must_use]
    pub fn num_elems(&self) -> u32 {
        self.elements.len() as u32
    }

    #[must_use]
    pub fn num_atoms(&self) -> u32 {
        self.atoms.len() as u32
    }

    #[must_use]
    pub fn atoms(&self) -> AtomView<'_> {
        &self.atoms
    }

    #[must_use]
    pub fn curr_atoms(&self) -> AtomView<'_> {
        &self.atoms[self.frame.atom.min(self.atoms.len())..]
    }

    #[must_use]
    pub fn has_term(&self, id: Id) -> bool {
        self.terms.get(id as usize).is_some_and(Option::is_some)
    }

    #[must_use]
    pub fn is_new_term(&self, id: Id) -> bool {
        self.has_term(id) && (id as usize) >= self.frame.term
    }

    #[must_use]
    pub fn has_element(&self, id: Id) -> bool {
        self.elements.get(id as usize).is_some_and(Option::is_some)
    }

    #[must_use]
    pub fn is_new_element(&self, id: Id) -> bool {
        self.has_element(id) && (id as usize) >= self.frame.element
    }

    pub fn get_term(&self, id: Id) -> Result<TheoryTerm, Error> {
        self.terms
            .get(id as usize)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| Error::OutOfRange(format!("Unknown term '{}'", id)))
    }

    pub fn get_element(&self, id: Id) -> Result<&TheoryElement, Error> {
        self.elements
            .get(id as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::OutOfRange(format!("Unknown element '{}'", id)))
    }

    pub fn filter<F>(&mut self, predicate: F)
    where
        F: Fn(&TheoryAtom) -> bool,
    {
        let split = self.frame.atom.min(self.atoms.len());
        let current = self.atoms.split_off(split);
        self.atoms.extend(
            current
                .into_iter()
                .filter(|atom| atom.atom() == 0 || !predicate(atom)),
        );
    }

    pub fn accept<V>(&self, visitor: &mut V, mode: VisitMode)
    where
        V: Visitor,
    {
        for atom in self.atom_iter(mode) {
            visitor.visit_atom(self, atom);
        }
    }

    pub fn accept_atom<V>(&self, atom: &TheoryAtom, visitor: &mut V, mode: VisitMode)
    where
        V: Visitor,
    {
        if self.do_visit_term(mode, atom.term()) {
            let term = self.get_term(atom.term()).expect("validated by caller");
            visitor.visit_term(self, atom.term(), &term);
        }
        for &element_id in atom.elements() {
            if self.do_visit_elem(mode, element_id) {
                let element = self.get_element(element_id).expect("validated by caller");
                visitor.visit_element(self, element_id, element);
            }
        }
        if let Some(&guard) = atom.guard() {
            if self.do_visit_term(mode, guard) {
                let term = self.get_term(guard).expect("validated by caller");
                visitor.visit_term(self, guard, &term);
            }
        }
        if let Some(&rhs) = atom.rhs() {
            if self.do_visit_term(mode, rhs) {
                let term = self.get_term(rhs).expect("validated by caller");
                visitor.visit_term(self, rhs, &term);
            }
        }
    }

    pub fn accept_element<V>(&self, element: &TheoryElement, visitor: &mut V, mode: VisitMode)
    where
        V: Visitor,
    {
        for &term_id in element.terms() {
            if self.do_visit_term(mode, term_id) {
                let term = self.get_term(term_id).expect("validated by caller");
                visitor.visit_term(self, term_id, &term);
            }
        }
    }

    pub fn accept_term<V>(&self, term: &TheoryTerm, visitor: &mut V, mode: VisitMode)
    where
        V: Visitor,
    {
        if term.term_type() == TheoryTermType::Compound {
            for &term_id in term.terms() {
                if self.do_visit_term(mode, term_id) {
                    let sub_term = self.get_term(term_id).expect("validated by caller");
                    visitor.visit_term(self, term_id, &sub_term);
                }
            }
            if term.is_function() {
                let function_id = term.function().expect("compound function checked");
                if self.do_visit_term(mode, function_id) {
                    let function_term = self.get_term(function_id).expect("validated by caller");
                    visitor.visit_term(self, function_id, &function_term);
                }
            }
        }
    }

    fn set_term(&mut self, term_id: Id, value: TheoryTerm) {
        let index = term_id as usize;
        if index >= self.terms.len() {
            self.terms.resize(index + 1, None);
        } else if self.terms[index].is_some() {
            potassco_check_pre!(
                !self.is_new_term(term_id),
                "Redefinition of theory term '{}'",
                term_id
            );
        }
        self.terms[index] = Some(value);
    }

    fn atom_iter(&self, mode: VisitMode) -> &[TheoryAtom] {
        match mode {
            VisitMode::All => self.atoms(),
            VisitMode::Current => self.curr_atoms(),
        }
    }

    fn do_visit_term(&self, mode: VisitMode, id: Id) -> bool {
        mode == VisitMode::All || self.is_new_term(id)
    }

    fn do_visit_elem(&self, mode: VisitMode, id: Id) -> bool {
        mode == VisitMode::All || self.is_new_element(id)
    }
}

impl Default for TheoryTerm {
    fn default() -> Self {
        Self::number_term(0)
    }
}

pub fn print_term(out: &mut dyn AbstractProgram, term_id: Id, term: &TheoryTerm) {
    match term.term_type() {
        TheoryTermType::Number => {
            out.theory_term_number(term_id, term.number().expect("number term"))
        }
        TheoryTermType::Symbol => {
            out.theory_term_symbol(term_id, term.symbol().expect("symbol term"))
        }
        TheoryTermType::Compound => out.theory_term_compound(
            term_id,
            term.compound().expect("compound term"),
            term.terms(),
        ),
    }
}

pub fn print_atom(out: &mut dyn AbstractProgram, atom: &TheoryAtom) {
    match (atom.guard(), atom.rhs()) {
        (Some(&guard), Some(&rhs)) => {
            out.theory_atom_guarded(atom.atom(), atom.term(), atom.elements(), guard, rhs)
        }
        (None, None) => out.theory_atom(atom.atom(), atom.term(), atom.elements()),
        _ => potassco_assert_not_reached!("guarded theory atom must have both guard and rhs"),
    }
}
