use rust_clasp::clasp::constraint::priority_reserved_ufs;
use rust_clasp::clasp::dependency_graph::{
    ACYCLICITY_CHECK_PRIO, AcyclicityCheck, AcyclicityStrategy, CmpArc, ExtDepGraph,
    ExtDepGraphError, SolveTestEvent,
};
use rust_clasp::clasp::literal::{pos_lit, to_lit};
use rust_clasp::clasp::shared_context::SharedContext;

#[test]
fn ext_dep_graph_finalization_orders_forward_and_inverse_arcs() {
    let mut graph = ExtDepGraph::new(3);
    let mut frozen_vars = Vec::new();

    graph.add_edge(pos_lit(3), 2, 1).unwrap();
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.add_edge(pos_lit(2), 0, 2).unwrap();

    assert_eq!(graph.finalize_with(|var| frozen_vars.push(var)), 3);
    assert!(graph.frozen());
    assert_eq!(graph.nodes(), 3);
    assert_eq!(graph.edges(), 3);
    assert_eq!(frozen_vars, vec![1, 3, 2]);

    let forward_from_zero: Vec<_> = graph
        .fwd_arcs_from(0)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(
        forward_from_zero,
        vec![(0, 1, pos_lit(1)), (0, 2, pos_lit(2))]
    );

    let forward_from_two: Vec<_> = graph
        .fwd_arcs_from(2)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(forward_from_two, vec![(2, 1, pos_lit(3))]);

    let inverse_to_one: Vec<_> = graph
        .inv_arcs_to(1)
        .iter()
        .map(|arc| (arc.tail(), arc.lit(), arc.continues()))
        .collect();
    assert_eq!(
        inverse_to_one,
        vec![(0, pos_lit(1), true), (2, pos_lit(3), false)]
    );

    let inverse_to_two: Vec<_> = graph
        .inv_arcs_to(2)
        .iter()
        .map(|arc| (arc.tail(), arc.lit(), arc.continues()))
        .collect();
    assert_eq!(inverse_to_two, vec![(0, pos_lit(2), false)]);
}

#[test]
fn ext_dep_graph_requires_update_before_mutation_after_finalize() {
    let mut graph = ExtDepGraph::new(2);
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.finalize();

    assert_eq!(
        graph.add_edge(pos_lit(2), 1, 0),
        Err(ExtDepGraphError::Frozen)
    );

    graph.update();
    graph.add_edge(pos_lit(2), 1, 0).unwrap();
    assert_eq!(graph.finalize(), 2);
    assert_eq!(graph.edges(), 2);
}

#[test]
fn ext_dep_graph_keeps_existing_and_new_edges_across_updates() {
    let mut graph = ExtDepGraph::new(2);
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.finalize();

    graph.update();
    graph.add_edge(to_lit(-2), 2, 3).unwrap();
    graph.add_edge(pos_lit(3), 1, 2).unwrap();
    assert_eq!(graph.finalize(), 3);

    assert_eq!(graph.nodes(), 4);
    assert_eq!(graph.edges(), 3);
    assert!(graph.valid_node(3));
    assert!(!graph.valid_node(4));

    let forward_from_one: Vec<_> = graph
        .fwd_arcs_from(1)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(forward_from_one, vec![(1, 2, pos_lit(3))]);

    let inverse_to_three: Vec<_> = graph
        .inv_arcs_to(3)
        .iter()
        .map(|arc| (arc.tail(), arc.lit()))
        .collect();
    assert_eq!(inverse_to_three, vec![(2, to_lit(-2))]);
}

#[test]
fn ext_dep_graph_invalid_incremental_updates_reset_committed_edges() {
    let mut graph = ExtDepGraph::new(2);
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    assert_eq!(graph.finalize(), 1);

    graph.update();
    graph.add_edge(to_lit(-2), 2, 3).unwrap();
    assert_eq!(graph.edges(), 1);
    assert_eq!(graph.generation_count(), 0);

    graph.add_edge(pos_lit(3), 1, 2).unwrap();
    assert_eq!(graph.edges(), 0);
    assert_eq!(graph.generation_count(), 1);

    assert_eq!(graph.finalize(), 3);
    assert_eq!(graph.edges(), 3);

    let forward_from_zero: Vec<_> = graph
        .fwd_arcs_from(0)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(forward_from_zero, vec![(0, 1, pos_lit(1))]);

    let forward_from_one: Vec<_> = graph
        .fwd_arcs_from(1)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(forward_from_one, vec![(1, 2, pos_lit(3))]);

    let forward_from_two: Vec<_> = graph
        .fwd_arcs_from(2)
        .iter()
        .map(|arc| (arc.tail(), arc.head(), arc.lit()))
        .collect();
    assert_eq!(forward_from_two, vec![(2, 3, to_lit(-2))]);
}

#[test]
fn ext_dep_graph_finalize_is_idempotent_while_frozen() {
    let mut graph = ExtDepGraph::new(1);
    graph.add_edge(pos_lit(4), 0, 0).unwrap();

    assert_eq!(graph.finalize(), 1);
    assert_eq!(graph.finalize(), 1);
    assert_eq!(graph.edges(), 1);
    assert_eq!(graph.arc(0).lit(), pos_lit(4));
}

#[test]
fn ext_dep_graph_begin_accessors_ignore_invalid_offsets() {
    let mut graph = ExtDepGraph::new(3);
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.finalize();

    assert!(graph.fwd_begin(1).is_none());
    assert!(graph.inv_begin(0).is_none());
    assert!(graph.fwd_begin(2).is_none());
    assert!(graph.inv_begin(2).is_none());
}

#[test]
fn ext_dep_graph_arc_next_stays_within_same_tail_group() {
    let mut graph = ExtDepGraph::new(3);
    graph.add_edge(pos_lit(3), 1, 2).unwrap();
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.add_edge(pos_lit(2), 0, 2).unwrap();
    graph.finalize();

    let arcs = graph.fwd_arcs_from(0);
    assert_eq!(arcs.len(), 2);
    assert_eq!(arcs[0].next(arcs, 0), Some(&arcs[1]));
    assert_eq!(arcs[1].next(arcs, 1), None);
    assert_eq!(arcs[0].next(arcs, 1), None);
}

#[test]
fn ext_dep_graph_inv_next_tracks_continuation_bit() {
    let mut graph = ExtDepGraph::new(3);
    graph.add_edge(pos_lit(3), 2, 1).unwrap();
    graph.add_edge(pos_lit(1), 0, 1).unwrap();
    graph.add_edge(pos_lit(2), 0, 2).unwrap();
    graph.finalize();

    let inverse = graph.inv_arcs_to(1);
    assert_eq!(inverse.len(), 2);
    assert_eq!(inverse[0].next(inverse, 0), Some(&inverse[1]));
    assert_eq!(inverse[1].next(inverse, 1), None);
    assert_eq!(inverse[0].next(inverse, 1), None);
}

#[test]
fn ext_dep_graph_cmp_arc_matches_cpp_ordering_rules() {
    let by_tail = CmpArc::<0>::new();
    let by_head = CmpArc::<1>::new();
    let left = rust_clasp::clasp::dependency_graph::Arc::create(pos_lit(1), 0, 2);
    let right = rust_clasp::clasp::dependency_graph::Arc::create(pos_lit(2), 1, 2);
    let last = rust_clasp::clasp::dependency_graph::Arc::create(pos_lit(3), 1, 3);

    assert!(by_tail.less_arc_node(&left, 1));
    assert!(!by_tail.less_node_arc(1, &right));
    assert!(by_tail.less_arc_arc(&left, &right));
    assert!(by_head.less_arc_arc(&right, &last));
}

#[test]
fn solve_test_event_matches_upstream_default_delta_tracking() {
    let mut ctx = SharedContext::default();
    let _ = ctx.add_var();
    let _ = ctx.start_add_constraints();
    assert!(ctx.end_init());

    let solver = ctx.master_ref();
    let event = SolveTestEvent::new(solver, 7, true);

    assert_eq!(event.result, -1);
    assert_eq!(event.hcc, 7);
    assert!(event.partial);
    assert_eq!(event.time, 0.0);
    assert_eq!(event.conf_delta, solver.stats().core.conflicts);
    assert_eq!(event.choice_delta, solver.stats().core.choices);
    assert_eq!(event.conflicts(), 0);
    assert_eq!(event.choices(), 0);
}

#[test]
fn acyclicity_check_starts_with_empty_structural_state() {
    let mut check = AcyclicityCheck::default();

    assert_eq!(ACYCLICITY_CHECK_PRIO, priority_reserved_ufs + 1);
    assert_eq!(check.priority(), ACYCLICITY_CHECK_PRIO);
    assert_eq!(check.strategy(), AcyclicityStrategy::PropFull);
    assert!(check.graph().is_none());
    assert!(!check.solver_bound());
    assert!(!check.has_reason_store());
    assert_eq!(check.tag_counter(), 0);
    assert_eq!(check.todo_count(), 0);
    assert_eq!(check.tag_slots(), 0);
    assert_eq!(check.parent_slots(), 0);
    assert_eq!(check.node_stack_len(), 0);
    assert_eq!(check.reason_len(), 0);
    assert_eq!(check.generation_id(), 0);

    check.set_strategy(AcyclicityStrategy::PropFwd);
    assert_eq!(check.strategy(), AcyclicityStrategy::PropFwd);

    check.set_strategy(AcyclicityStrategy::PropFullImp);
    assert_eq!(check.strategy(), AcyclicityStrategy::PropFullImp);
}
