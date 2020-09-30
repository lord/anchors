use typed_arena::Arena;
use std::rc::Rc;
use std::cell::{Cell, RefCell, RefMut};
use super::{GenericAnchor, AnchorDebugInfo, Engine};
use crate::AnchorInner;
use std::marker::PhantomData;
use slotmap::{secondary::SecondaryMap, Key};
use std::collections::HashMap;
use std::iter::Iterator;

pub struct Graph2<K: Key> {
    nodes: Arena<Node>,
    mapping: RefCell<SecondaryMap<K, *const Node>>,
    rev_mapping: RefCell<HashMap<*const Node, K>>,
}

pub struct Node {
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
    pub fn add_clean_parent(self, parent: NodeGuard<'a>) {
        if self.inside.ptrs.clean_parent0.get().is_none() {
            self.inside.ptrs.clean_parent0.set(Some(parent.inside as *const Node))
        } else {
            self.inside.ptrs.clean_parents.borrow_mut().push(parent.inside)
        }
    }

    pub fn clean_parents(self) -> impl Iterator<Item=NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.inside.ptrs.clean_parents.borrow_mut(),
            next_i: 0,
            first: self.inside.ptrs.clean_parent0.get(),
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_clean_parents(self) -> impl Iterator<Item=NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.inside.ptrs.clean_parents.borrow_mut(),
            next_i: 0,
            first: self.inside.ptrs.clean_parent0.take(),
            f: PhantomData,
            empty_on_drop: true,
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

    pub fn necessary_children<F: FnMut(NodeGuard<'a>)>(
        self,
        mut func: F,
    ) {
        for parent in self.inside.ptrs.necessary_children.borrow_mut().iter() {
            func(NodeGuard {inside: unsafe {&**parent}, f: self.f});
        }
    }

    pub fn drain_necessary_children<F: FnMut(NodeGuard<'a>)>(
        self,
        mut func: F,
    ) {
        for child in self.inside.ptrs.necessary_children.borrow_mut().drain(..) {
            let child_ref = unsafe {&*child};
            child_ref.necessary_count.set(child_ref.necessary_count.get() - 1);
            func(NodeGuard {inside: child_ref, f: self.f});
        }
    }
}

struct RefCellVecIterator<'a> {
    inside: RefMut<'a, Vec<*const Node>>,
    next_i: usize,
    first: Option<*const Node>,
    // hack to make RefCellVecIterator invariant
    f: PhantomData<&'a mut &'a ()>,
    empty_on_drop: bool,
}

impl <'a> Iterator for RefCellVecIterator<'a> {
    type Item = NodeGuard<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            return Some(NodeGuard {inside: unsafe{&*first},  f: self.f});
        }
        let next = self.inside.get(self.next_i)?;
        self.next_i += 1;
        Some(NodeGuard {inside: unsafe{&**next},  f: self.f})
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut remaining = self.inside.len() - self.next_i;

        if self.first.is_some() {
            remaining += 1;
        }

        (remaining, Some(remaining))
    }
}

impl <'a> Drop for RefCellVecIterator<'a> {
    fn drop(&mut self) {
        if self.empty_on_drop {
            self.inside.clear()
        }
    }
}

impl <K: Key + Copy> Graph2<K> {
    pub fn new() -> Self {
        Self {
            rev_mapping: RefCell::new(HashMap::new()),
            nodes: Arena::new(),
            mapping: RefCell::new(SecondaryMap::new()),
        }
    }

    pub fn insert<'a>(&'a self, mut node: Node) -> NodeGuard<'a> {
        // SAFETY: ensure ptrs struct is empty on insert
        // TODO this probably is not actually necessary if there's no way to take a Node out of the graph
        node.ptrs = NodePtrs::default();
        NodeGuard {inside: self.nodes.alloc(node), f: PhantomData}
    }

    pub fn get_or_default<'a>(&'a self, key: K) -> NodeGuard<'a> {
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
            self.rev_mapping.borrow_mut().insert(guard.inside as *const Node, key);
            guard
        }
    }

    pub fn lookup_key<'a>(&'a self, guard: NodeGuard<'a>) -> K {
        *self.rev_mapping.borrow().get(&(guard.inside as *const Node)).unwrap()
    }
}

#[test]
fn test_fails() {
}
