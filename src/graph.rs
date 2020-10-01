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

