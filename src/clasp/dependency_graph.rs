//! Partial Rust port of `original_clasp/clasp/dependency_graph.h` and
//! `original_clasp/src/dependency_graph.cpp`.
//!
//! This module currently ports the external dependency graph storage layer
//! (`ExtDepGraph`). The positive body-atom dependency graph (`PrgDepGraph`) and
//! the solver-integrated acyclicity checker still depend on unported shared
//! context and solver infrastructure.

use core::cmp::Ordering;

use crate::clasp::literal::{Literal, Var_t};

const INVALID_OFFSET: u32 = u32::MAX;
const FROZEN_SENTINEL_NODE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Arc {
    lit: Literal,
    node: [u32; 2],
}

impl Arc {
    pub const fn create(lit: Literal, start_node: u32, end_node: u32) -> Self {
        Self {
            lit,
            node: [start_node, end_node],
        }
    }

    pub const fn lit(self) -> Literal {
        self.lit
    }

    pub const fn tail(self) -> u32 {
        self.node[0]
    }

    pub const fn head(self) -> u32 {
        self.node[1]
    }

    pub fn next<'a>(&self, arcs: &'a [Arc], index: usize) -> Option<&'a Arc> {
        let current = arcs.get(index)?;
        if current != self {
            return None;
        }
        let next = arcs.get(index + 1)?;
        (self.tail() == next.tail()).then_some(next)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inv {
    lit: Literal,
    rep: u32,
}

impl Inv {
    pub const fn lit(self) -> Literal {
        self.lit
    }

    pub const fn tail(self) -> u32 {
        self.rep >> 1
    }

    pub const fn continues(self) -> bool {
        (self.rep & 1u32) != 0
    }

    pub fn next<'a>(&self, arcs: &'a [Inv], index: usize) -> Option<&'a Inv> {
        let current = arcs.get(index)?;
        if current != self || !self.continues() {
            return None;
        }
        arcs.get(index + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CmpArc<const X: usize>;

impl<const X: usize> CmpArc<X> {
    pub const fn new() -> Self {
        Self
    }

    pub const fn less_arc_node(&self, lhs: &Arc, node: u32) -> bool {
        lhs.node[X] < node
    }

    pub const fn less_node_arc(&self, node: u32, rhs: &Arc) -> bool {
        node < rhs.node[X]
    }

    pub const fn less_arc_arc(&self, lhs: &Arc, rhs: &Arc) -> bool {
        lhs.node[X] < rhs.node[X]
            || (lhs.node[X] == rhs.node[X] && lhs.node[1 - X] < rhs.node[1 - X])
    }

    fn ordering(&self, lhs: &Arc, rhs: &Arc) -> Ordering {
        if self.less_arc_arc(lhs, rhs) {
            Ordering::Less
        } else if self.less_arc_arc(rhs, lhs) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Node {
    fwd_off: u32,
    inv_off: u32,
}

impl Node {
    const fn sentinel() -> Self {
        Self {
            fwd_off: INVALID_OFFSET,
            inv_off: INVALID_OFFSET,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtDepGraphError {
    Frozen,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExtDepGraph {
    fwd_arcs: Vec<Arc>,
    inv_arcs: Vec<Inv>,
    nodes: Vec<Node>,
    max_node: u32,
    committed_edges: u32,
    generation_count: u32,
}

impl ExtDepGraph {
    pub fn new(num_node_guess: u32) -> Self {
        let mut graph = Self::default();
        graph.nodes.reserve(num_node_guess as usize);
        graph
    }

    pub fn add_edge(
        &mut self,
        lit: Literal,
        start_node: u32,
        end_node: u32,
    ) -> Result<(), ExtDepGraphError> {
        if self.frozen() {
            return Err(ExtDepGraphError::Frozen);
        }
        self.fwd_arcs.push(Arc::create(lit, start_node, end_node));
        self.max_node = self
            .max_node
            .max(start_node.max(end_node).saturating_add(1));
        if self.committed_edges != 0 && start_node.min(end_node) < self.nodes.len() as u32 {
            self.inv_arcs.clear();
            self.committed_edges = 0;
            self.generation_count = self.generation_count.wrapping_add(1);
        }
        Ok(())
    }

    pub fn update(&mut self) {
        if self.frozen() {
            let _ = self.fwd_arcs.pop();
        }
    }

    pub fn finalize(&mut self) -> u32 {
        self.finalize_with(|_| {})
    }

    pub fn finalize_with<F>(&mut self, mut freeze_var: F) -> u32
    where
        F: FnMut(Var_t),
    {
        let by_head = CmpArc::<1>::new();
        let by_tail = CmpArc::<0>::new();
        if self.frozen() {
            return self.committed_edges;
        }
        self.fwd_arcs
            .sort_unstable_by(|lhs, rhs| by_head.ordering(lhs, rhs));
        self.inv_arcs.clear();
        self.nodes.clear();
        self.nodes.resize(self.max_node as usize, Node::sentinel());

        let mut index = 0usize;
        while index < self.fwd_arcs.len() {
            let node = self.fwd_arcs[index].head();
            self.nodes[node as usize].inv_off = self.inv_arcs.len() as u32;
            let start = self.inv_arcs.len();
            while index < self.fwd_arcs.len() && self.fwd_arcs[index].head() == node {
                let arc = self.fwd_arcs[index];
                self.inv_arcs.push(Inv {
                    lit: arc.lit(),
                    rep: (arc.tail() << 1) | 1u32,
                });
                freeze_var(arc.lit().var());
                index += 1;
            }
            let end = self.inv_arcs.len();
            if end > start {
                self.inv_arcs[end - 1].rep ^= 1u32;
            }
        }

        self.fwd_arcs
            .sort_unstable_by(|lhs, rhs| by_tail.ordering(lhs, rhs));
        let mut index = 0usize;
        while index < self.fwd_arcs.len() {
            let node = self.fwd_arcs[index].tail();
            self.nodes[node as usize].fwd_off = index as u32;
            index += 1;
            while index < self.fwd_arcs.len() && self.fwd_arcs[index].tail() == node {
                index += 1;
            }
        }

        self.committed_edges = self.fwd_arcs.len() as u32;
        self.fwd_arcs.push(Arc::create(
            Literal::from_rep(0),
            FROZEN_SENTINEL_NODE,
            FROZEN_SENTINEL_NODE,
        ));
        self.committed_edges
    }

    pub fn frozen(&self) -> bool {
        self.fwd_arcs
            .last()
            .is_some_and(|arc| arc.tail() == FROZEN_SENTINEL_NODE)
    }

    pub fn arc(&self, id: u32) -> &Arc {
        &self.fwd_arcs[id as usize]
    }

    pub fn fwd_begin(&self, node: u32) -> Option<&Arc> {
        let offset = self.node(node)?.fwd_off;
        self.valid_off(offset)
            .then(|| &self.fwd_arcs[offset as usize])
    }

    pub fn inv_begin(&self, node: u32) -> Option<&Inv> {
        let offset = self.node(node)?.inv_off;
        self.valid_off(offset)
            .then(|| &self.inv_arcs[offset as usize])
    }

    pub fn fwd_arcs_from(&self, node: u32) -> &[Arc] {
        match self.node(node) {
            Some(node_data) if node_data.fwd_off != INVALID_OFFSET => {
                let start = node_data.fwd_off as usize;
                let end = self.scan_fwd_end(start, node);
                &self.fwd_arcs[start..end]
            }
            _ => &[],
        }
    }

    pub fn inv_arcs_to(&self, node: u32) -> &[Inv] {
        match self.node(node) {
            Some(node_data) if node_data.inv_off != INVALID_OFFSET => {
                let start = node_data.inv_off as usize;
                let end = self.scan_inv_end(start);
                &self.inv_arcs[start..end]
            }
            _ => &[],
        }
    }

    pub const fn nodes(&self) -> u32 {
        self.max_node
    }

    pub const fn edges(&self) -> u32 {
        self.committed_edges
    }

    pub fn valid_node(&self, node: u32) -> bool {
        node < self.max_node
    }

    pub const fn generation_count(&self) -> u32 {
        self.generation_count
    }

    const fn valid_off(&self, offset: u32) -> bool {
        offset != INVALID_OFFSET
    }

    fn node(&self, node: u32) -> Option<&Node> {
        self.nodes.get(node as usize)
    }

    fn scan_fwd_end(&self, start: usize, node: u32) -> usize {
        let mut end = start;
        while end < self.committed_edges as usize && self.fwd_arcs[end].tail() == node {
            end += 1;
        }
        end
    }

    fn scan_inv_end(&self, start: usize) -> usize {
        let mut end = start;
        while end < self.inv_arcs.len() {
            end += 1;
            if !self.inv_arcs[end - 1].continues() {
                break;
            }
        }
        end
    }
}
