use typed_arena::Arena;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use super::{GenericAnchor, AnchorDebugInfo, Engine};
use crate::AnchorInner;
use std::marker::PhantomData;

pub (super) struct Graph2 {
    nodes: Arena<Node>,
}

pub (super) struct Node {
    pub observed: Cell<bool>,
    pub valid: Cell<bool>,
    pub necessary_count: Cell<u32>,
    pub debug_info: Cell<AnchorDebugInfo>,
    pub anchor: Cell<Option<Box<dyn GenericAnchor>>>,
    pub ptrs: NodePtrs,
}
#[derive(Default)]
pub struct NodePtrs {
    clean_parent0: Cell<Option<*const Node>>,
    clean_parents: RefCell<Vec<*const Node>>,
}

#[derive(Clone, Copy)]
pub (super) struct NodeGuard<'a> {
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
    pub fn add_clean_parent(self, parent: NodeGuard<'a>) {
        if self.inside.ptrs.clean_parent0.get().is_none() {
            self.inside.ptrs.clean_parent0.set(Some(parent.inside as *const Node))
        } else {
            self.inside.ptrs.clean_parents.borrow_mut().push(parent.inside)
        }
    }

    pub fn clean_parents<F: FnMut(NodeGuard<'a>)>(
        self,
        mut func: F,
    ) {
        if let Some(parent) = self.inside.ptrs.clean_parent0.get() {
            func(NodeGuard {inside: unsafe {&*parent}, f: self.f});

            for parent in self.inside.ptrs.clean_parents.borrow_mut().iter() {
                func(NodeGuard {inside: unsafe {&**parent}, f: self.f});
            }
        }
    }

    pub fn drain_clean_parents<F: FnMut(NodeGuard<'a>)>(
        self,
        mut func: F,
    ) {
        if let Some(parent) = self.inside.ptrs.clean_parent0.take() {
            func(NodeGuard {inside: unsafe {&*parent}, f: self.f});

            for parent in self.inside.ptrs.clean_parents.borrow_mut().drain(..) {
                func(NodeGuard {inside: unsafe {&*parent}, f: self.f});
            }
        }
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
        // TODO this probably is not actually necessary if there's no way to take a Node out of the graph
        node.ptrs = NodePtrs::default();
        NodeGuard {inside: self.nodes.alloc(node), f: PhantomData}
    }
}

#[test]
fn test_fails() {
}
