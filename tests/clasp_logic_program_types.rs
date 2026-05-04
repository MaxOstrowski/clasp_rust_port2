use std::panic;

use rust_clasp::clasp::claspfwd::Configuration;
use rust_clasp::clasp::literal::{neg_lit, pos_lit, value_false, value_free, value_true};
use rust_clasp::clasp::logic_program_types::{
    AtomState, EdgeType, NodeType, NonHcfSet, PrgEdge, PrgNode, SmallEdgeList, SmallEdgeListTag,
    value_weak_true,
};

#[test]
fn prg_node_tracks_literal_identity_value_and_equivalence_flags() {
    let mut node = PrgNode::new(7, NodeType::Atom);

    assert!(node.is_atom());
    assert_eq!(node.node_type(), NodeType::Atom);
    assert!(node.relevant());
    assert!(!node.eq());
    assert!(!node.removed());
    assert!(!node.has_var());
    assert_eq!(node.value(), value_free);
    assert_eq!(node.true_lit(), rust_clasp::clasp::literal::lit_true);

    node.set_literal(pos_lit(4));
    node.set_value(value_true);
    assert!(node.has_var());
    assert_eq!(node.var(), 4);
    assert_eq!(node.literal(), pos_lit(4));
    assert_eq!(node.true_lit(), pos_lit(4));

    node.set_value(value_false);
    assert_eq!(node.true_lit(), neg_lit(4));

    node.set_eq(11);
    assert!(node.eq());
    assert!(!node.relevant());
    assert!(node.seen());
    assert_eq!(node.id(), 11);

    node.reset_id(12, false);
    assert_eq!(node.id(), 12);
    assert!(node.relevant());
    assert!(!node.seen());

    node.mark_removed();
    assert!(node.removed());
}

#[test]
fn prg_node_assign_value_impl_matches_upstream_weak_truth_rules() {
    let mut node = PrgNode::new(1, NodeType::Body);

    assert!(node.assign_value_impl(value_weak_true, false));
    assert_eq!(node.value(), value_weak_true);
    assert!(node.assign_value_impl(value_true, false));
    assert_eq!(node.value(), value_true);
    assert!(node.assign_value_impl(value_weak_true, false));
    assert_eq!(node.value(), value_true);
    assert!(!node.assign_value_impl(value_false, false));
    assert_eq!(node.value(), value_true);

    let mut no_weak = PrgNode::new(2, NodeType::Body);
    assert!(no_weak.assign_value_impl(value_weak_true, true));
    assert_eq!(no_weak.value(), value_true);
}

#[test]
fn prg_node_rejects_out_of_range_ids() {
    let result = panic::catch_unwind(|| PrgNode::new(PrgNode::NO_NODE, NodeType::Disj));
    assert!(result.is_err());
}

#[test]
fn prg_edge_encodes_node_type_and_edge_semantics() {
    let edge = PrgEdge::new(9, NodeType::Disj, EdgeType::GammaChoice);

    assert_eq!(edge.node(), 9);
    assert_eq!(edge.node_type(), NodeType::Disj);
    assert_eq!(edge.edge_type(), EdgeType::GammaChoice);
    assert!(edge.is_choice());
    assert!(edge.is_gamma());
    assert!(!edge.is_normal());
    assert!(edge.is_disj());
    assert!(!edge.is_atom());
    assert!(!edge.is_body());
    assert!(edge.is_valid());
    assert!(!PrgEdge::no_edge().is_valid());
}

#[test]
fn atom_state_tracks_head_body_and_rule_markers() {
    let head = PrgEdge::new(3, NodeType::Atom, EdgeType::Choice);
    let disj = PrgEdge::new(5, NodeType::Disj, EdgeType::Normal);
    let mut state = AtomState::new();

    state.add_to_head(2);
    state.add_to_head_edge(head);
    state.add_to_head_edge(disj);
    state.add_to_body_slice(&[pos_lit(7), neg_lit(8)]);
    state.set(11, AtomState::SHOWN_FLAG | AtomState::PROJECT_FLAG);

    assert!(state.in_head_atom(2));
    assert!(state.in_head(head));
    assert!(state.in_head(disj));
    assert!(state.in_body(pos_lit(7)));
    assert!(state.in_body(neg_lit(8)));
    assert!(state.is_set(11, AtomState::SHOWN_FLAG));
    assert!(state.is_set(11, AtomState::PROJECT_FLAG));
    assert!(state.all_marked(&[2], AtomState::HEAD_FLAG));
    assert!(state.is_set(3, AtomState::CHOICE_FLAG));
    assert!(state.body_marked(&[pos_lit(7), neg_lit(8)]));

    state.clear_head(head);
    state.clear_body(neg_lit(8));
    state.clear_rule_var(2);

    assert!(!state.in_head(head));
    assert!(!state.in_body(neg_lit(8)));
    assert!(!state.in_head_atom(2));

    state.clear_rule_atoms(&[3_u32, 5_u32]);
    assert!(!state.in_head(disj));

    let mut other = AtomState::new();
    other.add_to_body(pos_lit(1));
    state.swap(&mut other);
    assert!(state.in_body(pos_lit(1)));
    assert!(!other.in_body(pos_lit(1)));
}

#[test]
fn small_edge_list_preserves_small_and_large_storage_behaviour() {
    let e1 = PrgEdge::new(1, NodeType::Atom, EdgeType::Normal);
    let e2 = PrgEdge::new(2, NodeType::Body, EdgeType::Gamma);
    let e3 = PrgEdge::new(3, NodeType::Disj, EdgeType::Choice);
    let e4 = PrgEdge::new(4, NodeType::Atom, EdgeType::GammaChoice);
    let mut list = SmallEdgeList::default();

    let mut tag = list.push(SmallEdgeListTag::S0, e1);
    assert_eq!(tag, SmallEdgeListTag::S1);
    assert_eq!(list.span(tag), &[e1]);

    tag = list.push(tag, e2);
    assert_eq!(tag, SmallEdgeListTag::S2);
    assert_eq!(list.span(tag), &[e1, e2]);

    tag = list.push(tag, e3);
    assert_eq!(tag, SmallEdgeListTag::Large);
    assert_eq!(list.span(tag), &[e1, e2, e3]);

    tag = list.push(tag, e4);
    assert_eq!(list.size(tag), 4);
    assert_eq!(list.span(tag), &[e1, e2, e3, e4]);
    assert!(!list.empty(tag));

    let large_ptr = list.data_ptr(tag);
    tag = unsafe { list.shrink_to(tag, large_ptr.wrapping_add(2)) };
    assert_eq!(tag, SmallEdgeListTag::Large);
    assert_eq!(list.span(tag), &[e1, e2]);

    tag = list.pop(tag, 1);
    assert_eq!(tag, SmallEdgeListTag::Large);
    assert_eq!(list.span(tag), &[e1]);

    let cleared = list.clear(tag);
    assert_eq!(cleared, SmallEdgeListTag::S0);
    assert!(list.empty(cleared));

    let mut small = SmallEdgeList::default();
    let mut small_tag = small.push(SmallEdgeListTag::S0, e1);
    small_tag = small.push(small_tag, e2);
    let small_ptr = small.data_ptr(small_tag);
    small_tag = unsafe { small.shrink_to(small_tag, small_ptr.wrapping_add(1)) };
    assert_eq!(small_tag, SmallEdgeListTag::S1);
    assert_eq!(small.span(small_tag), &[e1]);
}

#[test]
fn non_hcf_set_constructor_starts_empty_with_null_config() {
    let set = NonHcfSet::new();

    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert!(set.config.is_null());
}

#[test]
fn non_hcf_set_add_keeps_sorted_unique_members() {
    let mut set = NonHcfSet::new();

    set.add(9);
    set.add(3);
    set.add(7);
    set.add(3);

    assert_eq!(set.as_slice(), &[3, 7, 9]);
}

#[test]
fn non_hcf_set_find_uses_the_sorted_membership_set() {
    let mut set = NonHcfSet::new();
    set.add(2);
    set.add(5);
    set.add(8);

    assert!(set.find(2));
    assert!(set.find(5));
    assert!(set.find(8));
    assert!(!set.find(3));
    assert!(!set.find(9));
}

#[test]
fn non_hcf_set_clone_from_matches_copy_assignment_state() {
    let config = std::ptr::NonNull::<Configuration>::dangling().as_ptr();
    let mut source = NonHcfSet::new();
    source.add(4);
    source.add(6);
    source.config = config;

    let mut target = NonHcfSet::new();
    target.add(9);
    target.clone_from(&source);

    assert_eq!(target.as_slice(), &[4, 6]);
    assert_eq!(target.config, config);
}

#[test]
fn non_hcf_set_view_matches_upstream_drop_semantics() {
    let mut set = NonHcfSet::new();
    set.add(1);
    set.add(4);
    set.add(7);

    assert_eq!(set.view(0), &[1, 4, 7]);
    assert_eq!(set.view(1), &[4, 7]);
    assert_eq!(set.view(3), &[]);

    let panic_result = panic::catch_unwind(|| set.view(4));
    assert!(panic_result.is_err());
}
