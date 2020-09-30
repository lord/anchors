use typed_arena::Arena;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use super::{GenericAnchor, AnchorDebugInfo, Engine};
use crate::AnchorInner;
use std::marker::PhantomData;
use slotmap::{secondary::SecondaryMap, Key};

pub (super) struct Graph2<K: Key> {
    nodes: Arena<Node>,
    mapping: RefCell<SecondaryMap<K, *const Node>>,
}

pub (super) struct Node {
    pub observed: Cell<bool>,
    pub valid: Cell<bool>,

    /// bool used during height incrementing to check for loops
    pub visited: Cell<bool>,

    /// number of nodes that list this node as a necessary child
    pub necessary_count: Cell<usize>,

    pub height: Cell<usize>,

    // pub debug_info: Cell<AnchorDebugInfo>,
    // pub anchor: Cell<Option<Box<dyn GenericAnchor>>>,
    pub ptrs: NodePtrs,
}
#[derive(Default)]
pub struct NodePtrs {
    clean_parent0: Cell<Option<*const Node>>,
    clean_parents: RefCell<Vec<*const Node>>,
    /// sorted in pointer order
    necessary_children: RefCell<Vec<*const Node>>,
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

    pub fn add_necessary_child(self, child: NodeGuard<'a>) {
        let mut necessary_children = self.inside.ptrs.necessary_children.borrow_mut();
        if let Err(i) = necessary_children.binary_search(&(child.inside as *const Node)) {
            necessary_children.insert(i, child.inside as *const Node);
            child.inside.necessary_count.set(child.inside.necessary_count.get() + 1)
        }
    }

    pub fn remove_necessary_child(self, child: NodeGuard<'a>) {
        let mut necessary_children = self.inside.ptrs.necessary_children.borrow_mut();
        if let Ok(i) = necessary_children.binary_search(&(child.inside as *const Node)) {
            necessary_children.remove(i);
            child.inside.necessary_count.set(child.inside.necessary_count.get() - 1)
        }
    }

    pub fn drain_necessary_children<F: FnMut(NodeGuard<'a>)>(
        self,
        mut func: F,
    ) {
        for parent in self.inside.ptrs.necessary_children.borrow_mut().drain(..) {
            func(NodeGuard {inside: unsafe {&*parent}, f: self.f});
        }
    }
}

impl <K: Key + Copy> Graph2<K> {
    fn new() -> Self {
        Self {
            nodes: Arena::new(),
            mapping: RefCell::new(SecondaryMap::new()),
        }
    }

    fn insert<'a>(&'a self, mut node: Node) -> NodeGuard<'a> {
        // SAFETY: ensure ptrs struct is empty on insert
        // TODO this probably is not actually necessary if there's no way to take a Node out of the graph
        node.ptrs = NodePtrs::default();
        NodeGuard {inside: self.nodes.alloc(node), f: PhantomData}
    }

    fn get_mut_or_default<'a>(&'a self, key: K) -> NodeGuard<'a> {
        let mut mapping = self.mapping.borrow_mut();
        if mapping.contains_key(key) {
            NodeGuard {inside: unsafe {&**mapping.get_unchecked(key)}, f: PhantomData}
        } else {
            let guard = self.insert(Node {
                observed: Cell::new(false),
                valid: Cell::new(false),
                visited: Cell::new(false),
                necessary_count: Cell::new(0),
                height: Cell::new(0),
                ptrs: NodePtrs::default(),
            });
            mapping.insert(key, guard.inside as *const Node);
            guard
        }
    }
}

#[test]
fn test_fails() {
}
