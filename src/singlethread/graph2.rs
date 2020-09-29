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

#[derive(Clone)]
pub struct NodeGuard<'a> {
    inside: &'a Node,
    f: PhantomData<&'a mut &'a ()>,
}

impl <'a> std::ops::Deref for NodeGuard<'a> {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.inside
    }
}

pub fn set_parent<'a>(me: NodeGuard<'a>, parent: Option<NodeGuard<'a>>) {
    me.inside.ptrs.parent.set(parent.map(|r| r.inside as *const Node))
}

pub fn parent<'a>(me: NodeGuard<'a>) -> Option<NodeGuard<'a>> {
    me.inside.ptrs.parent.get().map(|ptr| NodeGuard {inside: unsafe {&*ptr}, f: PhantomData})
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

impl <'a> Drop for NodeGuard<'a> {
    fn drop(&mut self) {}
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
        let node_d = graph_b.insert(Node {
            observed: Cell::new(false),
            ptrs: NodePtrs::default(),
        });
        let node_c = parent(node_d.clone());
        set_parent(node_b.clone(), node_c.clone());
        // set_parent(node_c.unwrap(), Some(node_b));
    }

    panic!("end");
}
