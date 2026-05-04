use std::panic::{self, AssertUnwindSafe};

use rust_clasp::potassco::basic_types::{
    AbstractProgram, AtomSpan, HeadType, Id, LitSpan, Weight, WeightLitSpan,
};
use rust_clasp::potassco::error::Error;
use rust_clasp::potassco::theory_data::{
    TheoryAtom, TheoryData, TheoryTerm, TheoryTermType, TupleType, VisitMode, Visitor, parens,
    print_atom, print_term,
};

fn catch_error<F>(func: F) -> Error
where
    F: FnOnce(),
{
    let payload = panic::catch_unwind(AssertUnwindSafe(func)).expect_err("expected panic");
    *payload
        .downcast::<Error>()
        .expect("expected potassco error")
}

#[test]
fn tuple_type_metadata_and_parens_match_upstream() {
    assert_eq!(parens(TupleType::Bracket), "[]");
    assert_eq!(parens(TupleType::Brace), "{}");
    assert_eq!(parens(TupleType::Paren), "()");
}

#[test]
fn theory_data_constructor_starts_empty_like_default_cpp_state() {
    let data = TheoryData::new();

    assert!(data.empty());
    assert_eq!(data.num_terms(), 0);
    assert_eq!(data.num_elems(), 0);
    assert_eq!(data.num_atoms(), 0);
    assert!(data.atoms().is_empty());
    assert!(data.curr_atoms().is_empty());
    assert!(!data.has_term(0));
    assert!(!data.has_element(0));
    assert!(!data.is_new_term(0));
    assert!(!data.is_new_element(0));
}

#[test]
fn theory_data_reset_restores_default_constructed_state() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 7);
    data.add_element(0, &[0], TheoryData::COND_DEFERRED);
    data.add_atom(3, 0, &[0]);
    data.update();

    data.reset();

    assert!(data.empty());
    assert_eq!(data.num_terms(), 0);
    assert_eq!(data.num_elems(), 0);
    assert_eq!(data.num_atoms(), 0);
    assert!(data.atoms().is_empty());
    assert!(data.curr_atoms().is_empty());
    assert!(!data.has_term(0));
    assert!(!data.has_element(0));
    assert!(!data.is_new_term(0));
    assert!(!data.is_new_element(0));
    assert_eq!(
        data.get_term(0),
        Err(Error::OutOfRange("Unknown term '0'".to_owned()))
    );
    assert_eq!(
        data.get_element(0),
        Err(Error::OutOfRange("Unknown element '0'".to_owned()))
    );
}

#[test]
fn theory_terms_preserve_number_symbol_function_and_tuple_behavior() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 7);
    data.add_term_symbol(1, "f");
    data.add_term_function(2, 1, &[0]);
    data.add_term_tuple(3, TupleType::Brace, &[0, 2]);

    let number = data.get_term(0).unwrap();
    assert_eq!(number.term_type(), TheoryTermType::Number);
    assert_eq!(number.number().unwrap(), 7);
    assert_eq!(number.size(), 0);
    assert_eq!(number.terms(), &[]);
    assert!(
        matches!(number.symbol(), Err(Error::InvalidArgument(message)) if message == "Term is not a symbol")
    );

    let symbol = data.get_term(1).unwrap();
    assert_eq!(symbol.term_type(), TheoryTermType::Symbol);
    assert_eq!(symbol.symbol().unwrap(), "f");
    assert!(
        matches!(symbol.compound(), Err(Error::InvalidArgument(message)) if message == "Term is not a compound")
    );

    let function = data.get_term(2).unwrap();
    assert_eq!(function.term_type(), TheoryTermType::Compound);
    assert!(function.is_function());
    assert!(!function.is_tuple());
    assert_eq!(function.function().unwrap(), 1);
    assert_eq!(function.compound().unwrap(), 1);
    assert_eq!(function.terms(), &[0]);

    let tuple = data.get_term(3).unwrap();
    assert!(tuple.is_tuple());
    assert_eq!(tuple.tuple().unwrap(), TupleType::Brace);
    assert_eq!(tuple.compound().unwrap(), -2);
    assert_eq!(tuple.terms(), &[0, 2]);
}

#[test]
fn theory_elements_atoms_and_update_tracking_match_cpp_contract() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 1);
    data.add_term_symbol(1, "g");
    data.add_term_function(2, 1, &[0]);
    data.add_element(0, &[2], TheoryData::COND_DEFERRED);
    data.add_atom(42, 2, &[0]);

    assert_eq!(data.num_terms(), 3);
    assert_eq!(data.num_elems(), 3);
    assert_eq!(data.num_atoms(), 1);
    assert!(data.has_term(2));
    assert!(data.is_new_term(2));
    assert!(data.has_element(0));
    assert!(data.is_new_element(0));
    assert_eq!(data.curr_atoms().len(), 1);
    assert_eq!(
        data.get_element(0).unwrap().condition(),
        TheoryData::COND_DEFERRED
    );

    let element = data.get_element(0).unwrap();
    assert_eq!(element.size(), 1);
    assert_eq!(element.terms(), &[2]);
    assert_eq!(element.begin().copied().collect::<Vec<_>>(), vec![2]);
    assert_eq!(element.end().copied().collect::<Vec<_>>(), Vec::<Id>::new());

    let first_atom = &data.atoms()[0];
    assert_eq!(first_atom.atom(), 42);
    assert_eq!(first_atom.term(), 2);
    assert_eq!(first_atom.size(), 1);
    assert_eq!(first_atom.elements(), &[0]);
    assert_eq!(first_atom.begin().copied().collect::<Vec<_>>(), vec![0]);
    assert_eq!(
        first_atom.end().copied().collect::<Vec<_>>(),
        Vec::<Id>::new()
    );
    assert_eq!(first_atom.guard(), None);
    assert_eq!(first_atom.rhs(), None);

    data.update();
    assert!(!data.is_new_term(2));
    assert!(!data.is_new_element(0));
    assert!(data.curr_atoms().is_empty());

    data.set_condition(0, 17);
    assert_eq!(data.get_element(0).unwrap().condition(), 17);

    data.add_atom_guarded(0, 2, &[0], 1, 0);
    assert_eq!(data.curr_atoms().len(), 1);
    let atom = &data.curr_atoms()[0];
    assert_eq!(atom.atom(), 0);
    assert_eq!(atom.term(), 2);
    assert_eq!(atom.size(), 1);
    assert_eq!(atom.elements(), &[0]);
    assert_eq!(atom.begin().copied().collect::<Vec<_>>(), vec![0]);
    assert_eq!(atom.end().copied().collect::<Vec<_>>(), Vec::<Id>::new());
    assert_eq!(atom.guard(), Some(&1));
    assert_eq!(atom.rhs(), Some(&0));
}

#[test]
fn theory_term_accessors_and_iterators_match_cpp_contract() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 7);
    data.add_term_symbol(1, "f");
    data.add_term_function(2, 1, &[0]);
    data.add_term_tuple(3, TupleType::Brace, &[0, 2]);

    let number = data.get_term(0).unwrap();
    assert_eq!(number.term_type(), TheoryTermType::Number);
    assert_eq!(number.number().unwrap(), 7);
    assert_eq!(number.size(), 0);
    assert_eq!(
        number.begin().copied().collect::<Vec<_>>(),
        Vec::<Id>::new()
    );
    assert_eq!(number.end().copied().collect::<Vec<_>>(), Vec::<Id>::new());

    let symbol = data.get_term(1).unwrap();
    assert_eq!(symbol.term_type(), TheoryTermType::Symbol);
    assert_eq!(symbol.symbol().unwrap(), "f");
    assert_eq!(symbol.size(), 0);
    assert_eq!(
        symbol.begin().copied().collect::<Vec<_>>(),
        Vec::<Id>::new()
    );
    assert_eq!(symbol.end().copied().collect::<Vec<_>>(), Vec::<Id>::new());

    let function = data.get_term(2).unwrap();
    assert_eq!(function.term_type(), TheoryTermType::Compound);
    assert!(function.is_function());
    assert!(!function.is_tuple());
    assert_eq!(function.function().unwrap(), 1);
    assert_eq!(function.compound().unwrap(), 1);
    assert_eq!(function.size(), 1);
    assert_eq!(function.terms(), &[0]);
    assert_eq!(function.begin().copied().collect::<Vec<_>>(), vec![0]);
    assert_eq!(
        function.end().copied().collect::<Vec<_>>(),
        Vec::<Id>::new()
    );

    let tuple = data.get_term(3).unwrap();
    assert_eq!(tuple.term_type(), TheoryTermType::Compound);
    assert!(!tuple.is_function());
    assert!(tuple.is_tuple());
    assert_eq!(tuple.tuple().unwrap(), TupleType::Brace);
    assert_eq!(tuple.compound().unwrap(), -2);
    assert_eq!(tuple.size(), 2);
    assert_eq!(tuple.terms(), &[0, 2]);
    assert_eq!(tuple.begin().copied().collect::<Vec<_>>(), vec![0, 2]);
    assert_eq!(tuple.end().copied().collect::<Vec<_>>(), Vec::<Id>::new());
}

#[test]
fn removing_and_redefining_old_terms_matches_original_semantics() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 1);
    data.update();
    data.remove_term(0);
    assert!(!data.has_term(0));
    assert!(!data.empty());
    assert_eq!(
        data.get_term(0),
        Err(Error::OutOfRange("Unknown term '0'".to_owned()))
    );

    data.add_term_symbol(0, "x");
    assert_eq!(data.get_term(0).unwrap().symbol().unwrap(), "x");
}

#[test]
fn redefining_new_terms_and_elements_panics_with_precondition_errors() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 1);
    let term_error = catch_error(|| data.add_term_symbol(0, "x"));
    assert!(
        matches!(term_error, Error::InvalidArgument(message) if message.contains("Redefinition of theory term '0'"))
    );

    let mut data = TheoryData::new();
    data.add_element(0, &[1], TheoryData::COND_DEFERRED);
    let elem_error = catch_error(|| data.add_element(0, &[2], TheoryData::COND_DEFERRED));
    assert!(
        matches!(elem_error, Error::InvalidArgument(message) if message.contains("Redefinition of theory element '0'"))
    );
}

#[test]
fn set_condition_requires_deferred_marker() {
    let mut data = TheoryData::new();
    data.add_element(0, &[1], 9);
    let error = catch_error(|| data.set_condition(0, 2));
    assert!(
        matches!(error, Error::InvalidArgument(message) if message.contains("condition == Self::COND_DEFERRED"))
    );
}

#[test]
fn set_condition_uses_unknown_element_error_from_get_element() {
    let mut data = TheoryData::new();
    let error = catch_error(|| data.set_condition(4, 2));
    assert_eq!(error, Error::OutOfRange("Unknown element '4'".to_owned()));
}

#[test]
fn filter_only_removes_current_non_directive_atoms() {
    let mut data = TheoryData::new();
    data.add_atom(1, 0, &[]);
    data.update();
    data.add_atom(2, 0, &[]);
    data.add_atom(0, 0, &[]);
    data.add_atom(3, 0, &[]);

    data.filter(|atom| atom.atom() == 2 || atom.atom() == 0);

    let ids: Vec<_> = data.atoms().iter().map(TheoryAtom::atom).collect();
    assert_eq!(ids, vec![1, 0, 3]);
}

#[derive(Default)]
struct RecordingVisitor {
    visited_terms: Vec<Id>,
    visited_elements: Vec<Id>,
    visited_atoms: Vec<Id>,
}

impl Visitor for RecordingVisitor {
    fn visit_term(&mut self, _data: &TheoryData, term_id: Id, _term: &TheoryTerm) {
        self.visited_terms.push(term_id);
    }

    fn visit_element(
        &mut self,
        _data: &TheoryData,
        element_id: Id,
        _element: &rust_clasp::potassco::theory_data::TheoryElement,
    ) {
        self.visited_elements.push(element_id);
    }

    fn visit_atom(&mut self, _data: &TheoryData, atom: &TheoryAtom) {
        self.visited_atoms.push(atom.atom());
    }
}

#[test]
fn visitor_accept_methods_honor_visit_mode() {
    let mut data = TheoryData::new();
    data.add_term_symbol(0, "f");
    data.add_term_number(1, 7);
    data.add_term_function(2, 0, &[1]);
    data.add_term_symbol(3, "<=");
    data.add_term_number(4, 9);
    data.add_element(0, &[2], TheoryData::COND_DEFERRED);
    data.add_atom_guarded(11, 2, &[0], 3, 4);
    data.update();
    data.add_term_number(5, 13);
    data.add_element(1, &[5], TheoryData::COND_DEFERRED);
    data.add_atom(12, 2, &[1]);

    let mut visitor = RecordingVisitor::default();
    data.accept(&mut visitor, VisitMode::Current);
    assert_eq!(visitor.visited_atoms, vec![12]);

    let guarded = &data.atoms()[0];
    let mut visitor = RecordingVisitor::default();
    data.accept_atom(guarded, &mut visitor, VisitMode::All);
    assert_eq!(visitor.visited_terms, vec![2, 3, 4]);
    assert_eq!(visitor.visited_elements, vec![0]);

    let element = data.get_element(0).unwrap();
    let mut visitor = RecordingVisitor::default();
    data.accept_element(element, &mut visitor, VisitMode::All);
    assert_eq!(visitor.visited_terms, vec![2]);

    let function = data.get_term(2).unwrap();
    let mut visitor = RecordingVisitor::default();
    data.accept_term(&function, &mut visitor, VisitMode::All);
    assert_eq!(visitor.visited_terms, vec![1, 0]);
}

#[derive(Default)]
struct RecordingProgram {
    calls: Vec<String>,
}

impl AbstractProgram for RecordingProgram {
    fn rule(&mut self, _head_type: HeadType, _head: AtomSpan<'_>, _body: LitSpan<'_>) {
        unreachable!()
    }

    fn rule_weighted(
        &mut self,
        _head_type: HeadType,
        _head: AtomSpan<'_>,
        _bound: Weight,
        _body: WeightLitSpan<'_>,
    ) {
        unreachable!()
    }

    fn minimize(&mut self, _priority: Weight, _lits: WeightLitSpan<'_>) {
        unreachable!()
    }

    fn output_atom(&mut self, _atom: rust_clasp::potassco::basic_types::Atom, _name: &str) {
        unreachable!()
    }

    fn theory_term_number(&mut self, term_id: Id, number: i32) {
        self.calls.push(format!("number:{term_id}:{number}"));
    }

    fn theory_term_symbol(&mut self, term_id: Id, name: &str) {
        self.calls.push(format!("symbol:{term_id}:{name}"));
    }

    fn theory_term_compound(&mut self, term_id: Id, compound: i32, args: &[Id]) {
        self.calls
            .push(format!("compound:{term_id}:{compound}:{args:?}"));
    }

    fn theory_atom(&mut self, atom_or_zero: Id, term_id: Id, elements: &[Id]) {
        self.calls
            .push(format!("atom:{atom_or_zero}:{term_id}:{elements:?}"));
    }

    fn theory_atom_guarded(
        &mut self,
        atom_or_zero: Id,
        term_id: Id,
        elements: &[Id],
        op: Id,
        rhs: Id,
    ) {
        self.calls.push(format!(
            "guarded:{atom_or_zero}:{term_id}:{elements:?}:{op}:{rhs}"
        ));
    }
}

#[test]
fn print_helpers_dispatch_theory_callbacks() {
    let mut data = TheoryData::new();
    data.add_term_number(0, 7);
    data.add_term_symbol(1, "f");
    data.add_term_function(2, 1, &[0]);
    data.add_atom(11, 2, &[3]);
    data.add_atom_guarded(12, 2, &[3, 4], 1, 0);

    let mut program = RecordingProgram::default();
    print_term(&mut program, 0, &data.get_term(0).unwrap());
    print_term(&mut program, 1, &data.get_term(1).unwrap());
    print_term(&mut program, 2, &data.get_term(2).unwrap());
    print_atom(&mut program, &data.atoms()[0]);
    print_atom(&mut program, &data.atoms()[1]);

    assert_eq!(
        program.calls,
        vec![
            "number:0:7",
            "symbol:1:f",
            "compound:2:1:[0]",
            "atom:11:2:[3]",
            "guarded:12:2:[3, 4]:1:0",
        ]
    );
}
