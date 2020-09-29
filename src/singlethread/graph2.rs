use typed_arena::Arena;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use super::{GenericAnchor, AnchorDebugInfo, Engine};
use crate::AnchorInner;
use std::marker::PhantomData;

pub struct Graph2 {
    nodes: Arena<Node>,
}

pub struct Node {
    pub observed: Cell<bool>,
    // pub debug_info: AnchorDebugInfo,
    // pub anchor: Rc<RefCell<dyn GenericAnchor>>,
    pub ptrs: NodePtrs,
}
#[derive(Default)]
pub struct NodePtrs {
    parent: Cell<Option<*const Node>>,
}

#[derive(Clone, Copy)]
pub struct NodeGuard<'a> {
    inside: &'a Node,
    // hack to make NodeGuard invariant
    f: PhantomData<&'a mut &'a ()>,
}

impl <'a> std::ops::Deref for NodeGuard<'a> {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.inside
    }
}

impl <'a> NodeGuard<'a> {
    pub fn set_parent(self, parent: Option<NodeGuard<'a>>) {
        self.inside.ptrs.parent.set(parent.map(|r| r.inside as *const Node))
    }

    pub fn parent(self) -> Option<NodeGuard<'a>> {
        self.inside.ptrs.parent.get().map(|ptr| NodeGuard {inside: unsafe {&*ptr}, f: self.f})
    }
}

impl Graph2 {
    fn new() -> Self {
        Self {
            nodes: Arena::new(),
        }
    }

    fn insert<'a>(&'a self, mut node: Node) -> NodeGuard<'a> {
        // SAFETY: ensure ptrs struct is empty on insert
        node.ptrs = NodePtrs::default();
        NodeGuard {inside: self.nodes.alloc(node), f: PhantomData}
    }
}

#[test]
fn test_fails() {
    let graph_a = Graph2::new();
    let node_a = graph_a.insert(Node {
        observed: Cell::new(false),
        ptrs: NodePtrs::default(),
    });

    {
        let graph_b = Graph2::new();
        let node_b = graph_b.insert(Node {
            observed: Cell::new(false),
            ptrs: NodePtrs::default(),
        });
        let node_b2 = graph_b.insert(Node {
            observed: Cell::new(false),
            ptrs: NodePtrs::default(),
        });
        node_b.set_parent(Some(node_b2));
        // set_parent(node_c.unwrap(), Some(node_b));
    }

    println!("{:?}", node_a.observed);

    panic!("end");
}
