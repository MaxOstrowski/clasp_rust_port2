use std::panic;

use rust_clasp::clasp::claspfwd::Configuration;
use rust_clasp::clasp::literal::{neg_lit, pos_lit, value_false, value_free, value_true};
use rust_clasp::clasp::logic_program_types::{
    AtomState, EdgeType, NodeType, NonHcfSet, PrgAtom, PrgAtomDependency, PrgEdge, PrgHead,
    PrgHeadSimplify, PrgNode, SmallEdgeList, SmallEdgeListTag, value_weak_true,
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
fn prg_head_tracks_support_storage_and_dirty_state() {
    let e1 = PrgEdge::new(1, NodeType::Body, EdgeType::Normal);
    let e2 = PrgEdge::new(2, NodeType::Disj, EdgeType::Choice);
    let mut head = PrgHead::new(7, NodeType::Atom, 19);

    assert_eq!(head.node_type(), NodeType::Atom);
    assert_eq!(head.data(), 19);
    assert_eq!(head.num_supports(), 0);
    assert_eq!(head.support(), PrgEdge::no_edge());
    assert!(!head.in_upper());
    assert!(!head.dirty());

    head.set_in_upper(true);
    head.add_support(e1, PrgHeadSimplify::ForceSimplify);
    assert!(head.in_upper());
    assert_eq!(head.supports(), &[e1]);
    assert!(!head.dirty());

    head.add_support(e2, PrgHeadSimplify::ForceSimplify);
    assert_eq!(head.support(), e1);
    assert_eq!(head.supports(), &[e1, e2]);
    assert!(head.dirty());

    head.mark_dirty();
    head.add_support(e1, PrgHeadSimplify::NoSimplify);
    assert_eq!(head.supports(), &[e1, e2, e1]);

    head.remove_support(e1);
    assert_eq!(head.supports(), &[e2]);
    assert!(head.dirty());

    head.clear_supports();
    assert_eq!(head.num_supports(), 0);
    assert_eq!(head.support(), PrgEdge::no_edge());
    assert!(!head.in_upper());
    assert!(!head.dirty());
}

#[test]
fn prg_head_in_upper_requires_a_relevant_node() {
    let mut head = PrgHead::new(8, NodeType::Disj, 3);

    assert_eq!(head.node_type(), NodeType::Disj);
    head.set_in_upper(true);
    assert!(head.in_upper());

    head.node_mut().mark_removed();
    assert!(!head.in_upper());
}

#[test]
fn prg_atom_tracks_local_state_freeze_and_dependency_lists() {
    let mut atom = PrgAtom::new(12);

    assert_eq!(PrgAtom::node_type(), NodeType::Atom);
    assert_eq!(atom.scc(), PrgNode::SCC_NOT_SET);
    assert!(!atom.has_scc());
    assert!(!atom.in_scc());
    assert!(!atom.frozen());
    assert_eq!(atom.assumption(), rust_clasp::clasp::literal::lit_true);
    assert_eq!(atom.fixed(), value_free);

    atom.node_mut().set_literal(pos_lit(5));
    atom.mark_frozen(value_true);
    assert!(atom.frozen());
    assert_eq!(atom.freeze_value(), value_true);
    assert_eq!(atom.assumption(), pos_lit(5));

    atom.mark_frozen(value_false);
    assert_eq!(atom.freeze_value(), value_false);
    assert_eq!(atom.assumption(), neg_lit(5));

    atom.clear_frozen();
    assert!(!atom.frozen());
    assert!(atom.head().dirty());

    atom.set_fact(true);
    atom.set_dom_var(21);
    assert!(atom.is_fact());
    assert_eq!(atom.dom_var(), 21);
    assert_eq!(atom.fixed(), value_true);

    atom.add_dep(7, true);
    atom.add_dep(8, false);
    atom.add_dep(9, true);
    assert_eq!(atom.deps(), &[pos_lit(7), neg_lit(8), pos_lit(9)]);
    assert!(atom.has_dep(PrgAtomDependency::Pos));
    assert!(atom.has_dep(PrgAtomDependency::Neg));
    assert!(atom.has_dep(PrgAtomDependency::All));

    atom.remove_dep(7, true);
    assert_eq!(atom.deps(), &[neg_lit(8), pos_lit(9)]);

    atom.clear_deps(PrgAtomDependency::Neg);
    assert_eq!(atom.deps(), &[pos_lit(9)]);
    assert!(atom.has_dep(PrgAtomDependency::Pos));
    assert!(!atom.has_dep(PrgAtomDependency::Neg));

    atom.clear_deps(PrgAtomDependency::All);
    assert!(atom.deps().is_empty());
}

#[test]
fn prg_atom_matches_upstream_eq_goal_scc_and_disj_support_rules() {
    let mut atom = PrgAtom::new(14);

    atom.head_mut().add_support(
        PrgEdge::new(4, NodeType::Body, EdgeType::Normal),
        PrgHeadSimplify::ForceSimplify,
    );
    assert!(!atom.in_disj());

    atom.head_mut().add_support(
        PrgEdge::new(6, NodeType::Disj, EdgeType::Choice),
        PrgHeadSimplify::ForceSimplify,
    );
    assert!(atom.in_disj());

    atom.node_mut().set_eq(23);
    assert_eq!(atom.eq_goal(false), pos_lit(23));
    assert_eq!(atom.eq_goal(true), neg_lit(23));

    atom.set_eq_goal(neg_lit(17));
    assert_eq!(atom.eq_goal(false), neg_lit(17));

    atom.set_eq_goal(pos_lit(9));
    assert_eq!(atom.eq_goal(false), pos_lit(23));

    atom.set_scc(PrgNode::SCC_TRIV);
    assert!(atom.has_scc());
    assert!(!atom.in_scc());
    assert!(atom.assign_value(value_weak_true));
    assert_eq!(atom.node().value(), value_true);

    atom.set_scc(7);
    assert!(atom.in_scc());
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
