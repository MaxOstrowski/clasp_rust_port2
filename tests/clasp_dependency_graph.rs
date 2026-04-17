use rust_clasp::clasp::dependency_graph::{ExtDepGraph, ExtDepGraphError};
use rust_clasp::clasp::literal::{pos_lit, to_lit};

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
