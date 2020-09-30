use std::fmt::Debug;
use crate::singlethread::graph2::{Graph2, NodeGuard};

use crate::singlethread::NodeNum;

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
        let parent = self.graph.get(parent).unwrap();
        let child = self.graph.get(child).unwrap();

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
        let parent = self.graph.get(parent).unwrap();
        let child = self.graph.get(child).unwrap();
        child.add_clean_parent(parent);
    }

    pub fn set_edge_necessary(&self, child: NodeNum, parent: NodeNum) {
        let parent = self.graph.get(parent).unwrap();
        let child = self.graph.get(child).unwrap();
        parent.add_necessary_child(child);
    }

    pub fn set_edge_unnecessary(&self, child: NodeNum, parent: NodeNum) {
        let parent = self.graph.get(parent).unwrap();
        let child = self.graph.get(child).unwrap();
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
        let node = self.graph.get(node_id).unwrap();
        let res: Vec<_> = node.clean_parents().map(|child| child.key.get()).collect();
        res.into_iter()
    }

    pub fn drain_clean_parents<'a>(
        &'a mut self,
        node_id: NodeNum,
    ) -> impl std::iter::Iterator<Item=NodeNum> {
        let node = self.graph.get(node_id).unwrap();
        let res: Vec<_> = node.drain_clean_parents().map(|child| child.key.get()).collect();
        res.into_iter()
    }

    #[allow(dead_code)]
    pub fn necessary_children<'a>(&'a self, node_id: NodeNum) -> impl std::iter::Iterator<Item = NodeNum> {
        let node = self.graph.get(node_id).unwrap();
        let mut res = vec![];
        for child in node.necessary_children() {
            res.push(child.key.get());
        }
        res.into_iter()
    }

    pub fn drain_necessary_children<'a>(&'a mut self, node_id: NodeNum) -> Option<Vec<NodeNum>> {
        let node = self.graph.get(node_id).unwrap();
        let mut res = vec![];
        for child in node.drain_necessary_children() {
            res.push(child.key.get());
        }
        Some(res)
    }

    pub fn is_necessary(&self, node_id: NodeNum) -> bool {
        let node = self.graph.get(node_id).unwrap();
        node.necessary_count.get() > 0
    }

    pub fn height(&self, node_id: NodeNum) -> usize {
        let node = self.graph.get(node_id).unwrap();
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
    use crate::constant::Constant;

    fn to_vec<I: std::iter::Iterator>(iter: I) -> Vec<I::Item> {
        iter.collect()
    }

    #[test]
    fn set_edge_updates_correctly() {
        let mut graph = MetadataGraph::new();
        let a = graph.raw_graph().insert_testing();
        let b = graph.raw_graph().insert_testing();
        let empty: Vec<NodeNum> = vec![];

        assert_eq!(empty, to_vec(graph.necessary_children(a)));
        assert_eq!(empty, to_vec(graph.clean_parents(a)));
        assert_eq!(empty, to_vec(graph.necessary_children(b)));
        assert_eq!(empty, to_vec(graph.clean_parents(b)));
        assert_eq!(false, graph.is_necessary(a));
        assert_eq!(false, graph.is_necessary(b));

        assert_eq!(Ok(false), graph.ensure_height_increases(a, b));
        assert_eq!(Ok(true), graph.ensure_height_increases(a, b));
        graph.set_edge_clean(a, b);

        assert_eq!(empty, to_vec(graph.necessary_children(a)));
        assert_eq!(vec![b], to_vec(graph.clean_parents(a)));
        assert_eq!(empty, to_vec(graph.necessary_children(b)));
        assert_eq!(empty, to_vec(graph.clean_parents(b)));
        assert_eq!(false, graph.is_necessary(a));
        assert_eq!(false, graph.is_necessary(b));

        assert_eq!(Ok(true), graph.ensure_height_increases(a, b));
        graph.set_edge_necessary(a, b);

        assert_eq!(empty, to_vec(graph.necessary_children(a)));
        assert_eq!(vec![b], to_vec(graph.clean_parents(a)));
        assert_eq!(vec![a], to_vec(graph.necessary_children(b)));
        assert_eq!(empty, to_vec(graph.clean_parents(b)));
        assert_eq!(true, graph.is_necessary(a));
        assert_eq!(false, graph.is_necessary(b));

        graph.drain_clean_parents(a);

        assert_eq!(empty, to_vec(graph.necessary_children(a)));
        assert_eq!(empty, to_vec(graph.clean_parents(a)));
        assert_eq!(vec![a], to_vec(graph.necessary_children(b)));
        assert_eq!(empty, to_vec(graph.clean_parents(b)));
        assert_eq!(true, graph.is_necessary(a));
        assert_eq!(false, graph.is_necessary(b));

        graph.drain_necessary_children(b);

        assert_eq!(empty, to_vec(graph.necessary_children(a)));
        assert_eq!(empty, to_vec(graph.clean_parents(a)));
        assert_eq!(empty, to_vec(graph.necessary_children(b)));
        assert_eq!(empty, to_vec(graph.clean_parents(b)));
        assert_eq!(false, graph.is_necessary(a));
        assert_eq!(false, graph.is_necessary(b));
    }

    #[test]
    fn height_calculated_correctly() {
        let mut graph = MetadataGraph::new();
        let a = graph.raw_graph().insert_testing();
        let b = graph.raw_graph().insert_testing();
        let c = graph.raw_graph().insert_testing();

        assert_eq!(0, graph.height(a));
        assert_eq!(0, graph.height(b));
        assert_eq!(0, graph.height(c));

        assert_eq!(Ok(false), graph.ensure_height_increases(b, c));
        assert_eq!(Ok(true), graph.ensure_height_increases(b, c));
        graph.set_edge_clean(b, c);

        assert_eq!(0, graph.height(a));
        assert_eq!(0, graph.height(b));
        assert_eq!(1, graph.height(c));

        assert_eq!(Ok(false), graph.ensure_height_increases(a, b));
        assert_eq!(Ok(true), graph.ensure_height_increases(a, b));
        graph.set_edge_clean(a, b);

        assert_eq!(0, graph.height(a));
        assert_eq!(1, graph.height(b));
        assert_eq!(2, graph.height(c));

        graph.drain_clean_parents(a);

        assert_eq!(0, graph.height(a));
        assert_eq!(1, graph.height(b));
        assert_eq!(2, graph.height(c));
    }

    #[test]
    fn cycles_cause_error() {
        let mut graph = MetadataGraph::new();
        let b = graph.raw_graph().insert_testing();
        let c = graph.raw_graph().insert_testing();
        graph.ensure_height_increases(b, c).unwrap();
        graph.set_edge_clean(b, c);
        graph
            .ensure_height_increases(c, b)
            .unwrap_err();
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let mut graph = MetadataGraph::new();
        let a = graph.raw_graph().insert_testing();
        let b = graph.raw_graph().insert_testing();
        let c = graph.raw_graph().insert_testing();
        let d = graph.raw_graph().insert_testing();
        let e = graph.raw_graph().insert_testing();

        graph.ensure_height_increases(b, c).unwrap();
        graph.set_edge_clean(b, c);
        graph.ensure_height_increases(c, e).unwrap();
        graph.set_edge_clean(c, e);
        graph.ensure_height_increases(b, d).unwrap();
        graph.set_edge_clean(b, d);
        graph.ensure_height_increases(d, e).unwrap();
        graph.set_edge_clean(d, e);
        graph.ensure_height_increases(a, b).unwrap();
        graph.set_edge_clean(a, b);
    }
}
