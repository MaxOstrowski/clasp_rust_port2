use rust_clasp::clasp::literal::{lit_false, lit_true, neg_lit, pos_lit};
use rust_clasp::clasp::satelite::{
    EVENT_BCE, EVENT_SUBSUMPTION, EVENT_VAR_ELIM, OccurList, SatElite, SatPreClause, less_occ_cost,
};
use rust_clasp::clasp::shared_context::SharedContext;

#[test]
fn occur_list_tracks_occurrences_marks_and_watchers() {
    let mut occur = OccurList::default();

    occur.add(11, false);
    occur.add(12, true);
    occur.add_watch(21);
    occur.add_watch(22);

    assert_eq!(occur.pos(), 1);
    assert_eq!(occur.neg(), 1);
    assert_eq!(occur.num_occ(), 2);
    assert_eq!(occur.cost(), 1);
    assert_eq!(occur.clause_range(), &[pos_lit(11), neg_lit(12)]);
    assert_eq!(occur.watchers().collect::<Vec<_>>(), vec![21, 22]);

    occur.remove_watch(21);
    occur.mark(false);

    assert!(occur.marked(false));
    assert!(!occur.marked(true));
    assert_eq!(occur.watchers().collect::<Vec<_>>(), vec![22]);

    occur.remove(12, true, false);
    assert_eq!(occur.num_occ(), 1);
    assert!(occur.dirty());
    assert_eq!(occur.clause_range(), &[pos_lit(11), neg_lit(12)]);

    occur.remove(11, false, true);
    assert_eq!(occur.num_occ(), 0);
    assert_eq!(occur.clause_range(), &[neg_lit(12)]);

    occur.clear();
    assert_eq!(occur.num_occ(), 0);
    assert!(occur.clause_range().is_empty());
    assert!(occur.watchers().next().is_none());
}

#[test]
fn satelite_resize_and_mark_helpers_preserve_upstream_behavior() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(3);
    satelite.occur_mut(1).add(4, false);
    satelite.occur_mut(2).add(9, true);

    assert_eq!(satelite.num_occ_slots(), 3);

    satelite.resize_occ(5);

    assert_eq!(satelite.num_occ_slots(), 5);
    assert_eq!(satelite.occur(1).clause_range(), &[pos_lit(4)]);
    assert_eq!(satelite.occur(2).clause_range(), &[neg_lit(9)]);

    let clause = SatPreClause::new(vec![pos_lit(1), neg_lit(2), pos_lit(3)]);
    satelite.mark_all(&[pos_lit(1), neg_lit(2)]);

    assert_eq!(satelite.find_unmarked_lit(&clause, 0), 2);
    assert!(satelite.occur(1).marked(false));
    assert!(satelite.occur(2).marked(true));

    satelite.unmark_all(&[pos_lit(1), neg_lit(2)]);
    assert_eq!(satelite.find_unmarked_lit(&clause, 0), 0);

    satelite.cleanup();
    assert_eq!(satelite.num_occ_slots(), 0);
}

#[test]
fn satelite_subsumes_short_and_long_clauses_like_upstream() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(16);

    let base = SatPreClause::new(vec![pos_lit(1), pos_lit(2)]);
    let superset = SatPreClause::new(vec![pos_lit(1), pos_lit(2), neg_lit(3)]);
    let strengthen = SatPreClause::new(vec![neg_lit(1), pos_lit(2)]);
    let mismatch = SatPreClause::new(vec![neg_lit(1), neg_lit(2)]);

    assert_eq!(satelite.subsumes(&base, &superset, lit_true), lit_true);
    assert_eq!(satelite.subsumes(&base, &strengthen, lit_true), pos_lit(1));
    assert_eq!(satelite.subsumes(&base, &mismatch, lit_true), lit_false);

    let long_a = SatPreClause::new(vec![
        pos_lit(1),
        pos_lit(2),
        pos_lit(3),
        pos_lit(4),
        pos_lit(5),
        pos_lit(6),
        pos_lit(7),
        pos_lit(8),
        pos_lit(9),
        pos_lit(10),
    ]);
    let long_b = SatPreClause::new(vec![
        pos_lit(1),
        pos_lit(2),
        pos_lit(3),
        pos_lit(4),
        neg_lit(5),
        pos_lit(6),
        pos_lit(7),
        pos_lit(8),
        pos_lit(9),
        pos_lit(10),
        neg_lit(11),
    ]);

    assert_eq!(satelite.subsumes(&long_a, &long_b, lit_true), pos_lit(5));
    for var in 1..=11 {
        assert_eq!(satelite.occur(var).lit_mark(), 0);
    }
}

#[test]
fn satelite_clause_helpers_and_cost_order_match_header_logic() {
    let mut clause = SatPreClause::new(vec![pos_lit(1), neg_lit(2), pos_lit(3)]);
    let mut occurs = vec![
        OccurList::default(),
        OccurList::default(),
        OccurList::default(),
    ];
    occurs[1].add(10, false);
    occurs[1].add(11, true);
    occurs[2].add(12, false);

    assert_eq!(EVENT_BCE, b'B');
    assert_eq!(EVENT_VAR_ELIM, b'E');
    assert_eq!(EVENT_SUBSUMPTION, b'S');
    assert!(clause.abstraction() & SatPreClause::abstract_lit(pos_lit(1)) != 0);
    assert!(clause.abstraction() & SatPreClause::abstract_lit(neg_lit(2)) != 0);
    assert!(!clause.in_queue());
    assert!(!clause.marked());

    clause.set_in_queue(true);
    clause.set_marked(true);
    clause.strengthen(neg_lit(2));

    assert!(clause.in_queue());
    assert!(clause.marked());
    assert_eq!(clause.lits(), &[pos_lit(1), pos_lit(3)]);
    assert!(less_occ_cost(&occurs, 2, 1));
}

#[test]
fn satelite_clause_simplify_removes_false_literals_and_tracks_satisfaction() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let c = ctx.add_var();
    assert!(ctx.add_unary(pos_lit(a)));
    assert!(ctx.add_unary(neg_lit(b)));

    let mut shrunk = SatPreClause::new(vec![neg_lit(a), pos_lit(b), pos_lit(c)]);
    shrunk.simplify(ctx.master_ref());
    assert_eq!(shrunk.lits(), &[pos_lit(c)]);

    let mut satisfied = SatPreClause::new(vec![neg_lit(a), pos_lit(c), neg_lit(b)]);
    satisfied.simplify(ctx.master_ref());
    assert_eq!(satisfied.lit(0), neg_lit(b));
    assert_eq!(satisfied.size(), 3);
}

#[test]
fn satelite_clause_add_to_creates_a_short_clause_in_solver() {
    let mut ctx = SharedContext::default();
    let a = ctx.add_var();
    let b = ctx.add_var();
    let clause = SatPreClause::new(vec![neg_lit(a), pos_lit(b)]);

    let solver = ctx.start_add_constraints();
    assert!(clause.add_to(solver));
    assert!(ctx.end_init());
    assert_eq!(ctx.num_binary(), 1);
}

#[test]
fn satelite_trivial_resolvent_detects_marked_complements() {
    let mut satelite = SatElite::new();
    satelite.resize_occ(6);
    satelite.occur_mut(2).mark(false);
    satelite.occur_mut(3).mark(true);

    let tautological = SatPreClause::new(vec![neg_lit(2), pos_lit(4)]);
    let non_tautological = SatPreClause::new(vec![pos_lit(2), pos_lit(5)]);

    assert!(satelite.trivial_resolvent(&tautological, 1));
    assert!(!satelite.trivial_resolvent(&non_tautological, 2));
}
