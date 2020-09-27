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
}

impl<T: Eq + Copy + Debug + Key + Ord> MetadataGraph<T> {
    pub fn new() -> Self {
        Self {
            nodes: SecondaryMap::new(),
        }
    }

    pub fn edge(&self, from: T, to: T) -> EdgeState {
        unimplemented!()
    }

    pub fn ensure_height_increases(&mut self, from: T, to: T) -> Result<(), Vec<T>> {
        self.set_min_height(to, self.height(from) + 1)
    }

    pub fn set_edge_clean(&mut self, child: T, parent: T, necessary: bool) -> Result<(), Vec<T>> {
        let node = self.get_mut_or_default(child);
        if let Err(i) = node.clean_parents.binary_search(&parent) {
            node.clean_parents.insert(i, parent);
        }
        self.ensure_height_increases(child, parent)
    }

    fn get_mut_or_default(&mut self, id: T) -> &mut Node<T> {
        if !self.nodes.contains_key(id) {
            self.nodes.insert(id, Default::default());
        }
        self.nodes.get_mut(id).unwrap()
    }

    pub fn remove(&mut self, node_id: T) {
        unimplemented!()
    }

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

    /// Increases height of this node to `min_height`, updating its parents as appropriate. If
    /// the node's height is already at `min_height` or above, this will do nothing. If a loop
    /// is detected, returns a vector of the nodes IDs that form a loop; the caller should
    /// consider the graph to be subsequently invalid and either create a fresh one or panic.
    // TODO this still has a bug when reporting loops where it may report more items than just what's available
    pub fn set_min_height(&mut self, node_id: T, min_height: usize) -> Result<(), Vec<T>> {
        // let node = self.get_mut_or_default(node_id);
        // if node.visited {
        //     return Err(vec![node_id]);
        // }
        // node.visited = true;
        // if node.height < min_height {
        //     node.height = min_height;
        //     for (child, _) in node.parents.clone().iter() {
        //         if let Err(mut loop_ids) = self.set_min_height(*child, min_height + 1) {
        //             loop_ids.push(node_id);
        //             return Err(loop_ids);
        //         }
        //     }
        // }
        // self.get_mut_or_default(node_id).visited = false;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use slotmap::{DefaultKey, KeyData};

    fn to_vec<I: std::iter::Iterator>(iter: Option<I>) -> Vec<I::Item> {
        match iter {
            None => vec![],
            Some(iter) => iter.collect(),
        }
    }

    #[test]
    fn set_edge_updates_correctly() {
    }

    fn k(num: u64) -> DefaultKey {
        KeyData::from_ffi(num).into()
    }

    #[test]
    fn height_calculated_correctly() {
    }

    #[test]
    fn cycles_cause_error() {
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
    }
}
