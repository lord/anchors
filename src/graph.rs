use crate::singlethread::graph2::Graph2;

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
}
