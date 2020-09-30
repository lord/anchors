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
    nodes: SecondaryMap<T, Node<T>>,
    graph: Graph2<T>,
}

impl<T: Eq + Copy + Debug + Key + Ord> MetadataGraph<T> {
    pub fn new() -> Self {
        Self {
            nodes: SecondaryMap::new(),
            graph: Graph2::new(),
        }
    }

    /// Returns Ok(true) if height was already increasing, Ok(false) if was not already increasing, and Err if there's a cycle.
    /// The error message is the list of node ids in the cycle.
    pub fn ensure_height_increases(&mut self, child: T, parent: T) -> Result<bool, Vec<T>> {
        let parent = self.graph.get_mut_or_default(parent);
        let child = self.graph.get_mut_or_default(child);
        if child.height.get() < parent.height.get() {
            return Ok(true);
        }
        child.visited.set(true);
        let res = set_min_height0(parent, child.height.get() + 1);
        child.visited.set(false);
        res.map(|()| false).map_err(|()| vec![])
    }

    pub fn set_edge_clean(&mut self, child: T, parent: T) {
        let parent = self.graph.get_mut_or_default(parent);
        let child = self.graph.get_mut_or_default(child);
        child.add_clean_parent(parent);
    }

    pub fn set_edge_necessary(&mut self, child: T, parent: T) {
        let parent = self.graph.get_mut_or_default(parent);
        let child = self.graph.get_mut_or_default(child);
        parent.add_necessary_child(child);
    }

    pub fn set_edge_unnecessary(&mut self, child: T, parent: T) {
        let parent = self.graph.get_mut_or_default(parent);
        let child = self.graph.get_mut_or_default(child);
        parent.remove_necessary_child(child);
    }

    pub fn remove(&mut self, node_id: T) {
        self.drain_necessary_children(node_id);
        self.nodes.remove(node_id);
    }

    #[allow(dead_code)]
    pub fn clean_parents<'a>(
        &'a self,
        node_id: T,
    ) -> Option<impl std::iter::Iterator<Item = &'a T>> {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return None,
        };
        Some(
            node.clean_parents.iter()
        )
    }

    pub fn drain_clean_parents<'a>(
        &'a mut self,
        node_id: T,
    ) -> Option<impl std::iter::Iterator<Item = T> + 'a> {
        let node = match self.nodes.get_mut(node_id) {
            Some(v) => v,
            None => return None,
        };
        Some(node.clean_parents.drain(..))
    }

    #[allow(dead_code)]
    pub fn necessary_children<'a>(&'a self, node_id: T) -> Option<impl std::iter::Iterator<Item = &'a T>> {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return None,
        };
        Some(node.necessary_children.iter())
    }

    pub fn drain_necessary_children<'a>(&'a mut self, node_id: T) -> Option<Vec<T>> {
        let node = match self.nodes.get_mut(node_id) {
            Some(v) => v,
            None => return None,
        };
        let necessary_children = std::mem::replace(&mut node.necessary_children, vec![]);
        for child in &necessary_children {
            if let Some(child_node) = self.nodes.get_mut(*child) {
                child_node.necessary_count -= 1;
            }
        }
        Some(necessary_children)
    }

    pub fn is_necessary(&self, node_id: T) -> bool {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return false,
        };

        node.necessary_count > 0
    }

    pub fn height(&self, node_id: T) -> usize {
        self.nodes.get(node_id).map(|node| node.height).unwrap_or(0)
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
        node.clean_parents(|parent| {
            if let Err(mut loop_ids) = set_min_height0(parent, min_height + 1) {
                did_err = true;
            }
        });
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

    fn to_vec<I: std::iter::Iterator>(iter: Option<I>) -> Vec<<I::Item as Deref>::Target> where I::Item: Deref, <I::Item as Deref>::Target : Sized + Copy {
        match iter {
            None => vec![],
            Some(iter) => iter.map(|v| *v).collect(),
        }
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
        let loop_ids = graph
            .ensure_height_increases(k(3), k(2))
            .unwrap_err();
        assert!(&loop_ids == &[k(2), k(3)] || &loop_ids == &[k(3), k(2)]);
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
