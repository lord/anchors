use slotmap::{secondary::SecondaryMap, Key};
use std::fmt::Debug;
use crate::singlethread::graph2::{Graph2, NodeGuard};

use crate::singlethread::NodeNum;

#[derive(Debug, Clone)]
struct Node {
    /// These are the parents of this node that are clean
    clean_parents: Vec<NodeNum>,

    /// These are the children of this node that are necessary by this node
    necessary_children: Vec<NodeNum>,

    /// This is the number of nodes that list this node as an necessary child
    necessary_count: usize,

    /// `0` if this node has no children, otherwise `max of children's height + 1`. This number
    /// can only ever increase, so that we avoid re-updating the whole graph if the height of some
    /// child element keeps changing.
    height: usize,
    /// Used when setting heights to detect cycles
    visited: bool,
}
impl Default for Node {
    fn default() -> Self {
        Node {
            height: 0,
            visited: false,
            clean_parents: vec![],
            necessary_children: vec![],
            necessary_count: 0,
        }
    }
}

pub struct MetadataGraph {
    graph: Graph2,
}

impl MetadataGraph {
    pub fn new() -> Self {
        Self {
            graph: Graph2::new(),
        }
    }

    pub fn raw_graph(&self) -> &Graph2 {
        &self.graph
    }

    /// Returns Ok(true) if height was already increasing, Ok(false) if was not already increasing, and Err if there's a cycle.
    /// The error message is the list of node ids in the cycle.
    pub fn ensure_height_increases(&self, child: NodeNum, parent: NodeNum) -> Result<bool, Vec<NodeNum>> {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);

        Self::raw_ensure_height_increases(child, parent).map_err(|()| vec![])
    }

    pub fn raw_ensure_height_increases<'a>(child: NodeGuard<'a>, parent: NodeGuard<'a>) -> Result<bool, ()> {
        if child.height.get() < parent.height.get() {
            return Ok(true);
        }
        child.visited.set(true);
        let res = set_min_height0(parent, child.height.get() + 1);
        child.visited.set(false);
        res.map(|()| false)
    }

    pub fn set_edge_clean(&self, child: NodeNum, parent: NodeNum) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        child.add_clean_parent(parent);
    }

    pub fn set_edge_necessary(&self, child: NodeNum, parent: NodeNum) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        parent.add_necessary_child(child);
    }

    pub fn set_edge_unnecessary(&self, child: NodeNum, parent: NodeNum) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        parent.remove_necessary_child(child);
    }

    pub fn remove(&self, node_id: NodeNum) {
        // TODO implement
    }

    #[allow(dead_code)]
    pub fn clean_parents<'a>(
        &'a self,
        node_id: NodeNum,
    ) -> impl std::iter::Iterator<Item = NodeNum> {
        let node = self.graph.get_or_default(node_id);
        let res: Vec<_> = node.clean_parents().map(|child| child.key.get()).collect();
        res.into_iter()
    }

    pub fn drain_clean_parents<'a>(
        &'a mut self,
        node_id: NodeNum,
    ) -> impl std::iter::Iterator<Item=NodeNum> {
        let node = self.graph.get_or_default(node_id);
        let res: Vec<_> = node.drain_clean_parents().map(|child| child.key.get()).collect();
        res.into_iter()
    }

    #[allow(dead_code)]
    pub fn necessary_children<'a>(&'a self, node_id: NodeNum) -> impl std::iter::Iterator<Item = NodeNum> {
        let node = self.graph.get_or_default(node_id);
        let mut res = vec![];
        for child in node.necessary_children() {
            res.push(child.key.get());
        }
        res.into_iter()
    }

    pub fn drain_necessary_children<'a>(&'a mut self, node_id: NodeNum) -> Option<Vec<NodeNum>> {
        let node = self.graph.get_or_default(node_id);
        let mut res = vec![];
        for child in node.drain_necessary_children() {
            res.push(child.key.get());
        }
        Some(res)
    }

    pub fn is_necessary(&self, node_id: NodeNum) -> bool {
        let node = self.graph.get_or_default(node_id);
        node.necessary_count.get() > 0
    }

    pub fn height(&self, node_id: NodeNum) -> usize {
        let node = self.graph.get_or_default(node_id);
        node.height.get()
    }
}

fn set_min_height0<'a>(node: NodeGuard<'a>, min_height: usize) -> Result<(), ()> {
    if node.visited.get() {
        return Err(());
    }
    node.visited.set(true);
    if node.height.get() < min_height {
        node.height.set(min_height);
        let mut did_err = false;
        for parent in node.clean_parents() {
            if let Err(mut loop_ids) = set_min_height0(parent, min_height + 1) {
                did_err = true;
            }
        };
        if did_err {
            return Err(())
        }
    }
    node.visited.set(false);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::ops::Deref;
    use slotmap::KeyData;

    fn to_vec<I: std::iter::Iterator>(iter: I) -> Vec<I::Item> {
        iter.collect()
    }

    fn k(num: u64) -> NodeNum {
        KeyData::from_ffi(num).into()
    }

    #[test]
    fn set_edge_updates_correctly() {
        let mut graph = MetadataGraph::new();
        let empty: Vec<NodeNum> = vec![];

        assert_eq!(empty, to_vec(graph.necessary_children(k(1))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(1))));
        assert_eq!(empty, to_vec(graph.necessary_children(k(2))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(2))));
        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        assert_eq!(Ok(false), graph.ensure_height_increases(k(1), k(2)));
        assert_eq!(Ok(true), graph.ensure_height_increases(k(1), k(2)));
        graph.set_edge_clean(k(1), k(2));

        assert_eq!(empty, to_vec(graph.necessary_children(k(1))));
        assert_eq!(vec![k(2)], to_vec(graph.clean_parents(k(1))));
        assert_eq!(empty, to_vec(graph.necessary_children(k(2))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(2))));
        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        assert_eq!(Ok(true), graph.ensure_height_increases(k(1), k(2)));
        graph.set_edge_necessary(k(1), k(2));

        assert_eq!(empty, to_vec(graph.necessary_children(k(1))));
        assert_eq!(vec![k(2)], to_vec(graph.clean_parents(k(1))));
        assert_eq!(vec![k(1)], to_vec(graph.necessary_children(k(2))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(2))));
        assert_eq!(true, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        graph.drain_clean_parents(k(1));

        assert_eq!(empty, to_vec(graph.necessary_children(k(1))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(1))));
        assert_eq!(vec![k(1)], to_vec(graph.necessary_children(k(2))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(2))));
        assert_eq!(true, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        graph.drain_necessary_children(k(2));

        assert_eq!(empty, to_vec(graph.necessary_children(k(1))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(1))));
        assert_eq!(empty, to_vec(graph.necessary_children(k(2))));
        assert_eq!(empty, to_vec(graph.clean_parents(k(2))));
        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));
    }

    #[test]
    fn height_calculated_correctly() {
        let mut graph = MetadataGraph::new();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(0, graph.height(k(2)));
        assert_eq!(0, graph.height(k(3)));

        assert_eq!(Ok(false), graph.ensure_height_increases(k(2), k(3)));
        assert_eq!(Ok(true), graph.ensure_height_increases(k(2), k(3)));
        graph.set_edge_clean(k(2), k(3));

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(0, graph.height(k(2)));
        assert_eq!(1, graph.height(k(3)));

        assert_eq!(Ok(false), graph.ensure_height_increases(k(1), k(2)));
        assert_eq!(Ok(true), graph.ensure_height_increases(k(1), k(2)));
        graph.set_edge_clean(k(1), k(2));

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(1, graph.height(k(2)));
        assert_eq!(2, graph.height(k(3)));

        graph.drain_clean_parents(k(1));

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(1, graph.height(k(2)));
        assert_eq!(2, graph.height(k(3)));
    }

    #[test]
    fn cycles_cause_error() {
        let mut graph = MetadataGraph::new();
        graph.ensure_height_increases(k(2), k(3)).unwrap();
        graph.set_edge_clean(k(2), k(3));
        graph
            .ensure_height_increases(k(3), k(2))
            .unwrap_err();
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let mut graph = MetadataGraph::new();
        graph.ensure_height_increases(k(10), k(20)).unwrap();
        graph.set_edge_clean(k(10), k(20));
        graph.ensure_height_increases(k(20), k(30)).unwrap();
        graph.set_edge_clean(k(20), k(30));
        graph.ensure_height_increases(k(10), k(21)).unwrap();
        graph.set_edge_clean(k(10), k(21));
        graph.ensure_height_increases(k(21), k(30)).unwrap();
        graph.set_edge_clean(k(21), k(30));
        graph.ensure_height_increases(k(2), k(10)).unwrap();
        graph.set_edge_clean(k(2), k(10));
    }
}
