use slotmap::{secondary::SecondaryMap, Key};
use std::fmt::Debug;
use crate::singlethread::graph2::{Graph2, NodeGuard};

#[derive(Debug, Clone)]
struct Node<T: Eq + Copy + Debug + Key + Ord> {
    /// These are the parents of this node that are clean
    clean_parents: Vec<T>,

    /// These are the children of this node that are necessary by this node
    necessary_children: Vec<T>,

    /// This is the number of nodes that list this node as an necessary child
    necessary_count: usize,

    /// `0` if this node has no children, otherwise `max of children's height + 1`. This number
    /// can only ever increase, so that we avoid re-updating the whole graph if the height of some
    /// child element keeps changing.
    height: usize,
    /// Used when setting heights to detect cycles
    visited: bool,
}
impl<T: Eq + Copy + Debug + Key + Ord> Default for Node<T> {
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

pub struct MetadataGraph<T: Eq + Copy + Debug + Key + Ord> {
    graph: Graph2<T>,
}

impl<T: Eq + Copy + Debug + Key + Ord> MetadataGraph<T> {
    pub fn new() -> Self {
        Self {
            graph: Graph2::new(),
        }
    }

    /// Returns Ok(true) if height was already increasing, Ok(false) if was not already increasing, and Err if there's a cycle.
    /// The error message is the list of node ids in the cycle.
    pub fn ensure_height_increases(&mut self, child: T, parent: T) -> Result<bool, Vec<T>> {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        if child.height.get() < parent.height.get() {
            return Ok(true);
        }
        child.visited.set(true);
        let res = set_min_height0(parent, child.height.get() + 1);
        child.visited.set(false);
        res.map(|()| false).map_err(|()| vec![])
    }

    pub fn set_edge_clean(&mut self, child: T, parent: T) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        child.add_clean_parent(parent);
    }

    pub fn set_edge_necessary(&mut self, child: T, parent: T) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        parent.add_necessary_child(child);
    }

    pub fn set_edge_unnecessary(&mut self, child: T, parent: T) {
        let parent = self.graph.get_or_default(parent);
        let child = self.graph.get_or_default(child);
        parent.remove_necessary_child(child);
    }

    pub fn remove(&mut self, node_id: T) {
        // TODO implement
    }

    #[allow(dead_code)]
    pub fn clean_parents<'a>(
        &'a self,
        node_id: T,
    ) -> impl std::iter::Iterator<Item = T> {
        let node = self.graph.get_or_default(node_id);
        let res: Vec<T> = node.clean_parents().map(|child| self.graph.lookup_key(child)).collect();
        res.into_iter()
    }

    pub fn drain_clean_parents<'a>(
        &'a mut self,
        node_id: T,
    ) -> impl std::iter::Iterator<Item=T> {
        let node = self.graph.get_or_default(node_id);
        let res: Vec<T> = node.drain_clean_parents().map(|child| self.graph.lookup_key(child)).collect();
        res.into_iter()
    }

    #[allow(dead_code)]
    pub fn necessary_children<'a>(&'a self, node_id: T) -> impl std::iter::Iterator<Item = T> {
        let node = self.graph.get_or_default(node_id);
        let mut res = vec![];
        for child in node.necessary_children() {
            res.push(self.graph.lookup_key(child));
        }
        res.into_iter()
    }

    pub fn drain_necessary_children<'a>(&'a mut self, node_id: T) -> Option<Vec<T>> {
        let node = self.graph.get_or_default(node_id);
        let mut res = vec![];
        for child in node.drain_necessary_children() {
            res.push(self.graph.lookup_key(child));
        }
        Some(res)
    }

    pub fn is_necessary(&self, node_id: T) -> bool {
        let node = self.graph.get_or_default(node_id);
        node.necessary_count.get() > 0
    }

    pub fn height(&self, node_id: T) -> usize {
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
    use slotmap::{DefaultKey, KeyData};
    use std::ops::Deref;

    fn to_vec<I: std::iter::Iterator>(iter: I) -> Vec<I::Item> {
        iter.collect()
    }

    fn k(num: u64) -> DefaultKey {
        KeyData::from_ffi(num).into()
    }

    #[test]
    fn set_edge_updates_correctly() {
        let mut graph = MetadataGraph::<DefaultKey>::new();
        let empty: Vec<DefaultKey> = vec![];

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
        let mut graph = MetadataGraph::<DefaultKey>::new();

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
        let mut graph = MetadataGraph::<DefaultKey>::new();
        graph.ensure_height_increases(k(2), k(3)).unwrap();
        graph.set_edge_clean(k(2), k(3));
        graph
            .ensure_height_increases(k(3), k(2))
            .unwrap_err();
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let mut graph = MetadataGraph::<DefaultKey>::new();
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
