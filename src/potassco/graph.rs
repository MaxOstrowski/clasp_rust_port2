//! Rust port of `original_clasp/libpotassco/potassco/graph.h`.

use core::fmt;
use core::marker::PhantomData;

pub type DefaultNodeId = u32;

pub trait GraphId:
    Copy + Eq + Ord + fmt::Debug + From<u8> + TryFrom<usize> + TryInto<usize>
{
}

impl<T> GraphId for T where
    T: Copy + Eq + Ord + fmt::Debug + From<u8> + TryFrom<usize> + TryInto<usize>
{
}

#[derive(Clone, Debug)]
struct Node<Data, Id> {
    edges: Vec<Id>,
    data: Data,
    min: Id,
    off: usize,
}

impl<Data, Id: Copy> Node<Data, Id> {
    fn new(data: Data, min: Id) -> Self {
        Self {
            edges: Vec::new(),
            data,
            min,
            off: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Graph<Data = usize, Id = DefaultNodeId>
where
    Id: GraphId,
{
    nodes: Vec<Node<Data, Id>>,
    open: Id,
    _marker: PhantomData<Id>,
}

impl<Data, Id> Default for Graph<Data, Id>
where
    Id: GraphId,
{
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            open: Id::from(0_u8),
            _marker: PhantomData,
        }
    }
}

impl<Data, Id> Graph<Data, Id>
where
    Id: GraphId,
{
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, data: Data) -> Id {
        let id = Self::id_from_usize(self.nodes.len());
        self.nodes.push(Node::new(data, self.open));
        id
    }

    pub fn add_edge(&mut self, from: Id, to: Id) {
        let from_index = Self::usize_from_id(from);
        self.nodes
            .get_mut(from_index)
            .expect("graph node id out of range")
            .edges
            .push(to);
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    pub fn compute_sccs(&mut self) -> Vec<Vec<Data>>
    where
        Data: Clone,
    {
        self.compute_sccs_impl(false)
    }

    pub fn compute_non_trivial_sccs(&mut self) -> Vec<Vec<Data>>
    where
        Data: Clone,
    {
        self.compute_sccs_impl(true)
    }

    fn compute_sccs_impl(&mut self, skip_trivial: bool) -> Vec<Vec<Data>>
    where
        Data: Clone,
    {
        let mut sccs = Vec::new();
        let mut stack = Vec::<Id>::new();
        let mut trail = Vec::<Id>::new();
        let open = self.open;
        let closed = if open == Id::from(0_u8) {
            Id::from(1_u8)
        } else {
            Id::from(0_u8)
        };

        for x_index in 0..self.nodes.len() {
            if self.nodes[x_index].min != open {
                continue;
            }

            let x_id = Self::id_from_usize(x_index);
            let mut index = Id::from(1_u8);
            Self::push_node(&mut self.nodes, &mut stack, &mut trail, x_id, &mut index);

            while let Some(&y_id) = stack.last() {
                let y_index = Self::usize_from_id(y_id);
                let mut finished = true;
                while self.nodes[y_index].off < self.nodes[y_index].edges.len() {
                    let z_id = self.nodes[y_index].edges[self.nodes[y_index].off];
                    self.nodes[y_index].off += 1;
                    let z_index = Self::usize_from_id(z_id);
                    if self
                        .nodes
                        .get(z_index)
                        .expect("graph node id out of range")
                        .min
                        == open
                    {
                        Self::push_node(&mut self.nodes, &mut stack, &mut trail, z_id, &mut index);
                        finished = false;
                        break;
                    }
                }

                if !finished {
                    continue;
                }

                stack.pop();
                let mut root = true;
                let mut y_min = self.nodes[y_index].min;
                let edges = self.nodes[y_index].edges.clone();
                for z_id in edges {
                    let z_index = Self::usize_from_id(z_id);
                    let z_min = self.nodes[z_index].min;
                    if z_min != closed && z_min < y_min {
                        assert!(z_min != open, "stack invariant broken");
                        root = false;
                        y_min = z_min;
                    }
                }
                self.nodes[y_index].min = y_min;

                if root {
                    let mut scc = Vec::new();
                    loop {
                        let n_id = trail.pop().expect("stack invariant broken");
                        let n_index = Self::usize_from_id(n_id);
                        self.nodes[n_index].min = closed;
                        scc.push(self.nodes[n_index].data.clone());
                        if n_id == y_id {
                            break;
                        }
                    }
                    if !skip_trivial || scc.len() > 1 {
                        sccs.push(scc);
                    }
                }
            }
        }

        self.open = closed;
        sccs
    }

    fn push_node(
        nodes: &mut [Node<Data, Id>],
        stack: &mut Vec<Id>,
        trail: &mut Vec<Id>,
        node_id: Id,
        index: &mut Id,
    ) {
        *index = Self::id_from_usize(Self::usize_from_id(*index) + 1);
        let node = &mut nodes[Self::usize_from_id(node_id)];
        node.min = *index;
        node.off = 0;
        stack.push(node_id);
        trail.push(node_id);
    }

    fn id_from_usize(value: usize) -> Id {
        Id::try_from(value).ok().expect("graph id overflow")
    }

    fn usize_from_id(value: Id) -> usize {
        value.try_into().ok().expect("graph id conversion failed")
    }
}
