use slotmap::{secondary::SecondaryMap, Key};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeState {
    Necessary,
    Dirty,
    Clean,
}

#[derive(Debug, Clone)]
struct Node<T: Eq + Copy + Debug + Key + Ord> {
    /// These are the nodes that are updated when our value changes. If true, edge is necessary,
    /// if false, edge is clean. A "dirty" edge is just a non-existant edge. Sorted by T.
    parents: Vec<(T, bool)>,
    /// These are the nodes that update us when they change. Sorted by T.
    children: Vec<T>,
    /// `0` if this node has no children, otherwise `max of children's height + 1`. This number
    /// can only ever increase, so that we avoid re-updating the whole graph if the height of some
    /// child element keeps changing.
    height: usize,
    /// Used when setting heights to detect cycles
    visited: bool,

    /// Indicates if a node was either marked dirty manually, or if an inbound edge was set to dirty.
    /// Can be reset back to clean with mark_clean.
    dirty: bool,
}
impl<T: Eq + Copy + Debug + Key + Ord> Default for Node<T> {
    fn default() -> Self {
        Node {
            parents: Vec::new(),
            children: Vec::new(),
            height: 0,
            visited: false,
            dirty: true,
        }
    }
}

pub struct MetadataGraph<T: Eq + Copy + Debug + Key + Ord> {
    nodes: SecondaryMap<T, Node<T>>,
}

impl<T: Eq + Copy + Debug + Key + Ord> MetadataGraph<T> {
    pub fn new() -> Self {
        Self {
            nodes: SecondaryMap::new(),
        }
    }

    pub fn edge(&self, from: T, to: T) -> EdgeState {
        let node = match self.nodes.get(from) {
            None => return EdgeState::Dirty,
            Some(v) => v,
        };
        match node.parents.binary_search_by_key(&to, |v| v.0) {
            Err(_) => EdgeState::Dirty,
            Ok(i) if node.parents[i].1 => EdgeState::Necessary,
            Ok(_) => EdgeState::Clean,
        }
    }

    pub fn set_edge(&mut self, from: T, to: T, state: EdgeState) -> Result<(), Vec<T>> {
        if state == EdgeState::Dirty {
            if let Some(node) = self.nodes.get_mut(to) {
                node.dirty = true;
            }
        }
        let is_necessary = match state {
            EdgeState::Dirty => {
                self.nodes.get_mut(from).map(|v| {
                    if let Ok(i) = v.parents.binary_search_by_key(&to, |v| v.0) {
                        v.parents.remove(i);
                    }
                });
                self.nodes.get_mut(to).map(|v| {
                    if let Ok(i) = v.children.binary_search(&from) {
                        v.children.remove(i);
                    }
                });
                return Ok(());
            }
            EdgeState::Clean => false,
            EdgeState::Necessary => true,
        };
        {
            let node = self.get_mut_or_default(from);
            match node.parents.binary_search_by_key(&to, |v| v.0) {
                Ok(i) => node.parents[i].1 = is_necessary,
                Err(i) => node.parents.insert(i, (to, is_necessary)),
            }
        }
        {
            let node = self.get_mut_or_default(to);
            match node.children.binary_search(&from) {
                Ok(_) => {}
                Err(i) => node.children.insert(i, from),
            }
        }
        self.set_min_height(to, self.height(from) + 1)
    }

    fn get_mut_or_default(&mut self, id: T) -> &mut Node<T> {
        if !self.nodes.contains_key(id) {
            self.nodes.insert(id, Default::default());
        }
        self.nodes.get_mut(id).unwrap()
    }

    pub fn remove(&mut self, node_id: T) {
        let node = match self.nodes.get(node_id) {
            Some(v) => v.clone(),
            None => return,
        };

        for child in node.children {
            // ok to unwrap — not possible to loop when removing an edge
            self.set_edge(child, node_id, EdgeState::Dirty).unwrap();
        }
        for (parent, _) in node.parents {
            // ok to unwrap — not possible to loop when removing an edge
            self.set_edge(node_id, parent, EdgeState::Dirty).unwrap();
        }

        self.nodes.remove(node_id);
    }

    pub fn mark_dirty(&mut self, node_id: T) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.dirty = true;
        }
    }

    pub fn mark_clean(&mut self, node_id: T) {
        let node = self.get_mut_or_default(node_id);
        node.dirty = false;
    }

    pub fn necessary_parents(&self, node_id: T) -> Vec<T> {
        self.get_parents(node_id, true)
    }

    #[allow(dead_code)]
    pub fn clean_parents(&self, node_id: T) -> Vec<T> {
        self.get_parents(node_id, false)
    }

    pub fn parents(&self, node_id: T) -> Vec<T> {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return vec![],
        };
        node.parents.iter().map(|(v, _)| v.clone()).collect()
    }

    #[allow(dead_code)]
    pub fn necessary_children(&self, node_id: T) -> Vec<T> {
        self.get_children(node_id, true)
    }

    #[allow(dead_code)]
    pub fn clean_children(&self, node_id: T) -> Vec<T> {
        self.get_children(node_id, false)
    }

    pub fn is_necessary(&self, node_id: T) -> bool {
        self.necessary_parents(node_id).len() > 0
    }

    pub fn is_dirty(&self, node_id: T) -> bool {
        self.nodes
            .get(node_id)
            .map(|node| node.dirty)
            .unwrap_or(true)
    }

    pub fn height(&self, node_id: T) -> usize {
        self.nodes.get(node_id).map(|node| node.height).unwrap_or(0)
    }

    /// Increases height of this node to `min_height`, updating its parents as appropriate. If
    /// the node's height is already at `min_height` or above, this will do nothing. If a loop
    /// is detected, returns a vector of the nodes IDs that form a loop; the caller should
    /// consider the graph to be subsequently invalid and either create a fresh one or panic.
    // TODO this still has a bug when reporting loops where it may report more items than just what's available
    pub fn set_min_height(&mut self, node_id: T, min_height: usize) -> Result<(), Vec<T>> {
        let node = self.get_mut_or_default(node_id);
        if node.visited {
            return Err(vec![node_id]);
        }
        node.visited = true;
        if node.height < min_height {
            node.height = min_height;
            for (child, _) in node.parents.clone().iter() {
                if let Err(mut loop_ids) = self.set_min_height(*child, min_height + 1) {
                    loop_ids.push(node_id);
                    return Err(loop_ids);
                }
            }
        }
        self.get_mut_or_default(node_id).visited = false;
        Ok(())
    }

    #[allow(dead_code)]
    fn get_parents(&self, node_id: T, necessary: bool) -> Vec<T> {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return vec![],
        };
        node.parents
            .iter()
            .filter_map(|(id, nec)| if &necessary == nec { Some(*id) } else { None })
            .collect()
    }

    fn get_children(&self, node_id: T, necessary: bool) -> Vec<T> {
        let node = match self.nodes.get(node_id) {
            Some(v) => v,
            None => return vec![],
        };
        node.children
            .iter()
            .filter(|id| {
                let i = self.nodes[**id]
                    .parents
                    .binary_search_by_key(&node_id, |v| v.0)
                    .unwrap();
                necessary == self.nodes[**id].parents[i].1
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use slotmap::{DefaultKey, KeyData};

    #[test]
    fn set_edge_updates_correctly() {
        let mut graph = MetadataGraph::<DefaultKey>::new();
        let empty: Vec<DefaultKey> = vec![];

        assert_eq!(EdgeState::Dirty, graph.edge(k(1), k(2)));
        assert_eq!(EdgeState::Dirty, graph.edge(k(2), k(1)));
        assert_eq!(empty, graph.necessary_parents(k(1)));
        assert_eq!(empty, graph.necessary_children(k(1)));
        assert_eq!(empty, graph.clean_parents(k(1)));
        assert_eq!(empty, graph.clean_children(k(1)));
        assert_eq!(empty, graph.necessary_parents(k(2)));
        assert_eq!(empty, graph.necessary_children(k(2)));
        assert_eq!(empty, graph.clean_parents(k(2)));
        assert_eq!(empty, graph.clean_children(k(2)));
        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        graph.set_edge(k(1), k(2), EdgeState::Clean).unwrap();

        assert_eq!(EdgeState::Clean, graph.edge(k(1), k(2)));
        assert_eq!(EdgeState::Dirty, graph.edge(k(2), k(1)));
        assert_eq!(empty, graph.necessary_parents(k(1)));
        assert_eq!(empty, graph.necessary_children(k(1)));
        assert_eq!(vec![k(2)], graph.clean_parents(k(1)));
        assert_eq!(empty, graph.clean_children(k(1)));
        assert_eq!(empty, graph.necessary_parents(k(2)));
        assert_eq!(empty, graph.necessary_children(k(2)));
        assert_eq!(empty, graph.clean_parents(k(2)));
        assert_eq!(vec![k(1)], graph.clean_children(k(2)));
        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        graph.set_edge(k(1), k(2), EdgeState::Necessary).unwrap();

        assert_eq!(EdgeState::Necessary, graph.edge(k(1), k(2)));
        assert_eq!(EdgeState::Dirty, graph.edge(k(2), k(1)));
        assert_eq!(vec![k(2)], graph.necessary_parents(k(1)));
        assert_eq!(empty, graph.necessary_children(k(1)));
        assert_eq!(empty, graph.clean_parents(k(1)));
        assert_eq!(empty, graph.clean_children(k(1)));
        assert_eq!(empty, graph.necessary_parents(k(2)));
        assert_eq!(vec![k(1)], graph.necessary_children(k(2)));
        assert_eq!(empty, graph.clean_parents(k(2)));
        assert_eq!(empty, graph.clean_children(k(2)));
        assert_eq!(true, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));

        graph.set_edge(k(1), k(2), EdgeState::Dirty).unwrap();

        assert_eq!(EdgeState::Dirty, graph.edge(k(1), k(2)));
        assert_eq!(EdgeState::Dirty, graph.edge(k(2), k(1)));
        assert_eq!(empty, graph.necessary_parents(k(1)));
        assert_eq!(empty, graph.necessary_children(k(1)));
        assert_eq!(empty, graph.clean_parents(k(1)));
        assert_eq!(empty, graph.clean_children(k(1)));
        assert_eq!(empty, graph.necessary_parents(k(2)));
        assert_eq!(empty, graph.necessary_children(k(2)));
        assert_eq!(empty, graph.clean_parents(k(2)));
        assert_eq!(empty, graph.clean_children(k(2)));

        assert_eq!(false, graph.is_necessary(k(1)));
        assert_eq!(false, graph.is_necessary(k(2)));
    }

    fn k(num: u64) -> DefaultKey {
        KeyData::from_ffi(num).into()
    }

    #[test]
    fn height_calculated_correctly() {
        let mut graph = MetadataGraph::<DefaultKey>::new();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(0, graph.height(k(2)));
        assert_eq!(0, graph.height(k(3)));

        graph.set_edge(k(2), k(3), EdgeState::Dirty).unwrap();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(0, graph.height(k(2)));
        assert_eq!(0, graph.height(k(3)));

        graph.set_edge(k(2), k(3), EdgeState::Necessary).unwrap();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(0, graph.height(k(2)));
        assert_eq!(1, graph.height(k(3)));

        graph.set_edge(k(1), k(2), EdgeState::Clean).unwrap();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(1, graph.height(k(2)));
        assert_eq!(2, graph.height(k(3)));

        graph.set_edge(k(1), k(2), EdgeState::Dirty).unwrap();

        assert_eq!(0, graph.height(k(1)));
        assert_eq!(1, graph.height(k(2)));
        assert_eq!(2, graph.height(k(3)));

        graph.set_min_height(k(1), 10).unwrap();

        assert_eq!(10, graph.height(k(1)));
        assert_eq!(1, graph.height(k(2)));
        assert_eq!(2, graph.height(k(3)));

        graph.set_min_height(k(2), 5).unwrap();

        assert_eq!(10, graph.height(k(1)));
        assert_eq!(5, graph.height(k(2)));
        assert_eq!(6, graph.height(k(3)));
    }

    #[test]
    fn cycles_cause_error() {
        let mut graph = MetadataGraph::<DefaultKey>::new();
        graph.set_edge(k(2), k(3), EdgeState::Necessary).unwrap();
        let loop_ids = graph
            .set_edge(k(3), k(2), EdgeState::Necessary)
            .unwrap_err();
        assert!(&loop_ids == &[k(2), k(3), k(2)] || &loop_ids == &[k(3), k(2), k(3)]);
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let mut graph = MetadataGraph::<DefaultKey>::new();
        graph.set_edge(k(10), k(20), EdgeState::Necessary).unwrap();
        graph.set_edge(k(20), k(30), EdgeState::Necessary).unwrap();
        graph.set_edge(k(10), k(21), EdgeState::Necessary).unwrap();
        graph.set_edge(k(21), k(30), EdgeState::Necessary).unwrap();
        graph.set_edge(k(2), k(10), EdgeState::Necessary).unwrap();
    }

    #[test]
    fn dirty_calculated_correctly() {
        let mut graph = MetadataGraph::<DefaultKey>::new();

        assert_eq!(true, graph.is_dirty(k(1)));
        graph.mark_clean(k(1));
        graph.mark_clean(k(2));
        assert_eq!(false, graph.is_dirty(k(1)));
        assert_eq!(false, graph.is_dirty(k(2)));
        graph.set_edge(k(2), k(1), EdgeState::Dirty).unwrap();
        assert_eq!(true, graph.is_dirty(k(1)));
        assert_eq!(false, graph.is_dirty(k(2)));
    }
}
