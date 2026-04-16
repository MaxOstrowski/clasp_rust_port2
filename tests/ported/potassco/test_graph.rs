use rust_clasp::potassco::graph::Graph;

fn to_string(sccs: &[Vec<u32>]) -> String {
    let mut out = String::from("[");
    for (scc_index, scc) in sccs.iter().enumerate() {
        if scc_index != 0 {
            out.push(',');
        }
        out.push('[');
        for (node_index, id) in scc.iter().enumerate() {
            if node_index != 0 {
                out.push(',');
            }
            out.push(char::from_u32(u32::from(b'a') + *id).expect("valid ascii test data"));
        }
        out.push(']');
    }
    out.push(']');
    out
}

#[test]
fn graph_empty() {
    let mut graph = Graph::<u32>::new();
    assert!(graph.compute_sccs().is_empty());
}

#[test]
fn graph_single_node() {
    let mut graph = Graph::<u32>::new();
    graph.add_node(0);

    let sccs = graph.compute_sccs();
    assert_eq!(sccs.len(), 1);
    assert_eq!(to_string(&sccs), "[[a]]");
}

#[test]
fn graph_acyclic() {
    let mut graph = Graph::<u32>::new();
    let id_a = graph.add_node(0);
    let id_b = graph.add_node(1);
    let id_c = graph.add_node(2);

    graph.add_edge(id_a, id_b);
    graph.add_edge(id_b, id_c);

    let sccs = graph.compute_sccs();
    assert_eq!(sccs.len(), 3);
    assert_eq!(to_string(&sccs), "[[c],[b],[a]]");
    assert!(graph.compute_non_trivial_sccs().is_empty());
}

#[test]
fn graph_single_cycle() {
    let mut graph = Graph::<u32>::new();
    let id_a = graph.add_node(0);
    let id_b = graph.add_node(1);
    let id_c = graph.add_node(2);
    graph.add_node(3);

    graph.add_edge(id_a, id_b);
    graph.add_edge(id_b, id_c);
    graph.add_edge(id_c, id_a);

    let sccs = graph.compute_sccs();
    assert_eq!(sccs.len(), 2);
    assert_eq!(to_string(&sccs), "[[c,b,a],[d]]");
}

#[test]
fn graph_multiple_cycles() {
    let mut graph = Graph::<u32>::new();
    let id_a = graph.add_node(0);
    let id_b = graph.add_node(1);
    let id_c = graph.add_node(2);
    let id_d = graph.add_node(3);
    let id_e = graph.add_node(4);
    let id_f = graph.add_node(5);
    let id_g = graph.add_node(6);
    let id_h = graph.add_node(7);
    let id_i = graph.add_node(8);

    graph.add_edge(id_a, id_g);
    graph.add_edge(id_b, id_e);
    graph.add_edge(id_b, id_h);
    graph.add_edge(id_c, id_i);
    graph.add_edge(id_c, id_h);
    graph.add_edge(id_d, id_f);
    graph.add_edge(id_e, id_a);
    graph.add_edge(id_f, id_b);
    graph.add_edge(id_f, id_c);
    graph.add_edge(id_g, id_d);

    assert_eq!(
        to_string(&graph.compute_sccs()),
        "[[h],[i],[c],[e,b,f,d,g,a]]"
    );
}

#[test]
fn graph_preserved_after_repeated_scc_computation() {
    let mut graph = Graph::<u32>::new();
    let id_a = graph.add_node(0);
    let id_b = graph.add_node(1);
    let id_c = graph.add_node(2);
    let id_d = graph.add_node(3);
    let id_e = graph.add_node(4);
    let id_f = graph.add_node(5);
    let id_g = graph.add_node(6);
    let id_h = graph.add_node(7);
    let id_i = graph.add_node(8);

    graph.add_edge(id_a, id_b);
    graph.add_edge(id_b, id_c);
    graph.add_edge(id_c, id_h);
    graph.add_edge(id_c, id_d);
    graph.add_edge(id_d, id_e);
    graph.add_edge(id_e, id_f);
    graph.add_edge(id_e, id_b);
    graph.add_edge(id_e, id_c);
    graph.add_edge(id_f, id_g);
    graph.add_edge(id_g, id_f);
    graph.add_edge(id_h, id_i);
    graph.add_edge(id_i, id_h);

    let expected = "[[i,h],[g,f],[e,d,c,b],[a]]";
    assert_eq!(to_string(&graph.compute_sccs()), expected);
    assert_eq!(to_string(&graph.compute_sccs()), expected);
    assert_eq!(to_string(&graph.compute_sccs()), expected);
}

#[test]
fn clear_keeps_graph_reusable_with_current_open_marker() {
    let mut graph = Graph::<u32>::new();
    let first_a = graph.add_node(0);
    let first_b = graph.add_node(1);
    graph.add_edge(first_a, first_b);
    graph.add_edge(first_b, first_a);
    assert_eq!(to_string(&graph.compute_non_trivial_sccs()), "[[b,a]]");

    graph.clear();
    let second_b = graph.add_node(1);
    let second_c = graph.add_node(2);
    graph.add_edge(second_b, second_c);
    graph.add_edge(second_c, second_b);

    assert_eq!(to_string(&graph.compute_non_trivial_sccs()), "[[c,b]]");
}
