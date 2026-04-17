//! Partial port of `original_clasp/tests/satelite_test.cpp`.

use rust_clasp::clasp::literal::{lit_false, lit_true, neg_lit, pos_lit};
use rust_clasp::clasp::satelite::{OccurList, SatElite, SatPreClause, less_occ_cost};

#[test]
fn simple_subsume_matches_upstream_clause_helper_behavior() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(8);

    let base = SatPreClause::new(vec![pos_lit(1), pos_lit(2)]);
    let superset = SatPreClause::new(vec![pos_lit(1), pos_lit(2), pos_lit(3)]);

    assert_eq!(satelite.subsumes(&base, &superset, lit_true), lit_true);
}

#[test]
fn simple_strengthen_matches_upstream_complementary_literal_case() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(8);

    let base = SatPreClause::new(vec![pos_lit(1), pos_lit(2)]);
    let complementary = SatPreClause::new(vec![neg_lit(1), pos_lit(2)]);
    let mismatch = SatPreClause::new(vec![neg_lit(1), neg_lit(2)]);

    assert_eq!(
        satelite.subsumes(&base, &complementary, lit_true),
        pos_lit(1)
    );
    assert_eq!(satelite.subsumes(&base, &mismatch, lit_true), lit_false);
}

#[test]
fn helper_clause_and_occurrence_state_match_current_satelite_port() {
    let mut clause = SatPreClause::new(vec![pos_lit(1), neg_lit(2), pos_lit(3)]);
    let mut occurs = vec![
        OccurList::default(),
        OccurList::default(),
        OccurList::default(),
    ];
    occurs[1].add(10, false);
    occurs[1].add(11, true);
    occurs[2].add(12, false);

    assert!(!clause.in_queue());
    assert!(!clause.marked());
    assert!(clause.abstraction() & SatPreClause::abstract_lit(pos_lit(1)) != 0);
    assert!(clause.abstraction() & SatPreClause::abstract_lit(neg_lit(2)) != 0);

    clause.set_in_queue(true);
    clause.set_marked(true);
    clause.strengthen(neg_lit(2));

    assert!(clause.in_queue());
    assert!(clause.marked());
    assert_eq!(clause.lits(), &[pos_lit(1), pos_lit(3)]);
    assert!(less_occ_cost(&occurs, 2, 1));
}

#[test]
fn mark_helpers_and_trivial_resolvent_cover_current_non_context_logic() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(6);
    satelite.occur_mut(2).add(20, false);
    satelite.occur_mut(3).add(30, true);
    satelite.mark_all(&[pos_lit(2), neg_lit(3)]);

    let clause = SatPreClause::new(vec![pos_lit(2), neg_lit(3), pos_lit(4)]);
    let tautological = SatPreClause::new(vec![neg_lit(2), pos_lit(4)]);
    let non_tautological = SatPreClause::new(vec![pos_lit(2), pos_lit(5)]);

    assert_eq!(satelite.find_unmarked_lit(&clause, 0), 2);
    assert!(satelite.trivial_resolvent(&tautological, 1));
    assert!(!satelite.trivial_resolvent(&non_tautological, 2));

    satelite.unmark_all(&[pos_lit(2), neg_lit(3)]);
    assert_eq!(satelite.find_unmarked_lit(&clause, 0), 0);
}

#[test]
fn subsumed_returns_true_when_a_watched_clause_is_fully_covered() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(8);

    let mut clause_lits = vec![pos_lit(1), pos_lit(2)];
    let mut watched = vec![SatPreClause::new(vec![pos_lit(1), pos_lit(2)])];

    satelite.mark_all(&clause_lits);
    satelite.occur_mut(1).add_watch(0);

    assert!(satelite.subsumed(&mut clause_lits, &mut watched));
    assert_eq!(clause_lits, vec![pos_lit(1), pos_lit(2)]);
    assert_eq!(satelite.occur(1).watchers().collect::<Vec<_>>(), vec![0]);
}

#[test]
fn subsumed_retargets_watches_and_prunes_redundant_marked_literals() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(8);

    let mut clause_lits = vec![pos_lit(1), neg_lit(4)];
    let mut watched = vec![SatPreClause::new(vec![pos_lit(1), pos_lit(4)])];

    satelite.mark_all(&clause_lits);
    satelite.occur_mut(1).add_watch(0);

    assert!(!satelite.subsumed(&mut clause_lits, &mut watched));
    assert_eq!(clause_lits, vec![pos_lit(1)]);
    assert_eq!(watched[0].lits(), &[pos_lit(4), pos_lit(1)]);
    assert_eq!(
        satelite.occur(1).watchers().collect::<Vec<_>>(),
        Vec::<u32>::new()
    );
    assert_eq!(satelite.occur(4).watchers().collect::<Vec<_>>(), vec![0]);
}

#[test]
fn subsumed_drops_clause_literal_once_an_existing_watch_proves_coverage() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(8);

    let mut clause_lits = vec![pos_lit(1), pos_lit(2)];
    let mut watched = vec![SatPreClause::new(vec![pos_lit(3), pos_lit(1), pos_lit(2)])];

    satelite.mark_all(&clause_lits);
    satelite.occur_mut(1).add_watch(0);

    assert!(!satelite.subsumed(&mut clause_lits, &mut watched));
    assert_eq!(clause_lits, vec![pos_lit(2)]);
    assert_eq!(satelite.occur(1).watchers().collect::<Vec<_>>(), vec![0]);
}
