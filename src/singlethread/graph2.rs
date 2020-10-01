use typed_arena::Arena;
use std::rc::Rc;
use std::cell::{Cell, RefCell, RefMut};
use super::{GenericAnchor, AnchorDebugInfo, Engine, Generation};
use crate::AnchorInner;
use std::marker::PhantomData;
use slotmap::{SlotMap, Key};
use std::iter::Iterator;

use crate::singlethread::NodeNum;

pub struct Graph2 {
    nodes: Arena<Node>,
    mapping: RefCell<SlotMap<NodeNum, *const Node>>,
}

pub struct Node {
    pub observed: Cell<bool>,
    pub valid: Cell<bool>,

    /// bool used during height incrementing to check for loops
    pub visited: Cell<bool>,

    /// number of nodes that list this node as a necessary child
    pub necessary_count: Cell<usize>,

    pub height: Cell<usize>,

    pub key: Cell<NodeNum>,

    pub (super) debug_info: Cell<AnchorDebugInfo>,

    /// tracks the generation when this Node last polled as Updated or Unchanged
    pub (super) last_ready: Cell<Option<Generation>>,
    /// tracks the generation when this Node last polled as Updated
    pub (super) last_update: Cell<Option<Generation>>,

    pub (super) anchor: Rc<RefCell<dyn GenericAnchor>>,

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

use std::fmt;
impl fmt::Debug for NodeGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeGuard")
        .field("inside.key", &self.inside.key.get())
        .finish()
    }
}

impl PartialEq for NodeGuard<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.inside, other.inside)
    }
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

    pub fn necessary_children(self) -> impl Iterator<Item=NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.inside.ptrs.necessary_children.borrow_mut(),
            next_i: 0,
            first: None,
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_necessary_children(self) -> impl Iterator<Item=NodeGuard<'a>> {
        let necessary_children = self.inside.ptrs.necessary_children.borrow_mut();
        for child in &*necessary_children {
            let count = &unsafe {&**child}.necessary_count;
            count.set(count.get() - 1);
        }
        RefCellVecIterator {
            inside: necessary_children,
            next_i: 0,
            first: None,
            f: PhantomData,
            empty_on_drop: true,
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

impl Graph2 {
    pub fn new() -> Self {
        Self {
            nodes: Arena::new(),
            mapping: RefCell::new(SlotMap::with_key()),
        }
    }

    #[cfg(test)]
    pub fn insert_testing<'a>(&'a self) -> NodeGuard<'a> {
        let key = self.insert(Rc::new(RefCell::new(crate::constant::Constant::new_raw_testing(123))), AnchorDebugInfo {
            location: None,
            type_info: "testing dummy anchor",
        });
        self.get(key).unwrap()
    }

    pub (super) fn insert<'a>(&'a self, mut anchor: Rc<RefCell<dyn GenericAnchor>>, debug_info: AnchorDebugInfo) -> NodeNum {
        self.mapping.borrow_mut().insert_with_key(|key| {
            let mut node = Node {
                observed: Cell::new(false),
                valid: Cell::new(false),
                visited: Cell::new(false),
                necessary_count: Cell::new(0),
                height: Cell::new(0),
                key: Cell::new(key),
                ptrs: NodePtrs::default(),
                debug_info: Cell::new(debug_info),
                last_ready: Cell::new(None),
                last_update: Cell::new(None),
                anchor,
            };
            // SAFETY: ensure ptrs struct is empty on insert
            // TODO this probably is not actually necessary if there's no way to take a Node out of the graph
            node.ptrs = NodePtrs::default();
            self.nodes.alloc(node)
        })
    }

    pub fn get<'a>(&'a self, key: NodeNum) -> Option<NodeGuard<'a>> {
        self.mapping.borrow().get(key).map(|ptr| NodeGuard {inside: unsafe {&**ptr}, f: PhantomData})
    }
}

pub fn ensure_height_increases<'a>(child: NodeGuard<'a>, parent: NodeGuard<'a>) -> Result<bool, ()> {
    if child.height.get() < parent.height.get() {
        return Ok(true);
    }
    child.visited.set(true);
    let res = set_min_height(parent, child.height.get() + 1);
    child.visited.set(false);
    res.map(|()| false)
}

fn set_min_height<'a>(node: NodeGuard<'a>, min_height: usize) -> Result<(), ()> {
    if node.visited.get() {
        return Err(());
    }
    node.visited.set(true);
    if node.height.get() < min_height {
        node.height.set(min_height);
        let mut did_err = false;
        for parent in node.clean_parents() {
            if let Err(mut loop_ids) = set_min_height(parent, min_height + 1) {
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
        let mut graph = Graph2::new();
        let a = graph.insert_testing();
        let b = graph.insert_testing();
        let empty: Vec<NodeGuard<'_>> = vec![];

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(empty, to_vec(a.clean_parents()));
        assert_eq!(empty, to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(false, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);

        assert_eq!(Ok(false), ensure_height_increases(a, b));
        assert_eq!(Ok(true), ensure_height_increases(a, b));
        a.add_clean_parent(b);

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(vec![b], to_vec(a.clean_parents()));
        assert_eq!(empty, to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(false, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);

        assert_eq!(Ok(true), ensure_height_increases(a, b));
        b.add_necessary_child(a);

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(vec![b], to_vec(a.clean_parents()));
        assert_eq!(vec![a], to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(true, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);

        a.drain_clean_parents();

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(empty, to_vec(a.clean_parents()));
        assert_eq!(vec![a], to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(true, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);

        b.drain_necessary_children();

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(empty, to_vec(a.clean_parents()));
        assert_eq!(empty, to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(false, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);
    }

    #[test]
    fn height_calculated_correctly() {
        let mut graph = Graph2::new();
        let a = graph.insert_testing();
        let b = graph.insert_testing();
        let c = graph.insert_testing();

        assert_eq!(0, a.height.get());
        assert_eq!(0, b.height.get());
        assert_eq!(0, c.height.get());

        assert_eq!(Ok(false), ensure_height_increases(b, c));
        assert_eq!(Ok(true), ensure_height_increases(b, c));
        b.add_clean_parent(c);

        assert_eq!(0, a.height.get());
        assert_eq!(0, b.height.get());
        assert_eq!(1, c.height.get());

        assert_eq!(Ok(false), ensure_height_increases(a, b));
        assert_eq!(Ok(true), ensure_height_increases(a, b));
        a.add_clean_parent(b);

        assert_eq!(0, a.height.get());
        assert_eq!(1, b.height.get());
        assert_eq!(2, c.height.get());

        a.drain_clean_parents();

        assert_eq!(0, a.height.get());
        assert_eq!(1, b.height.get());
        assert_eq!(2, c.height.get());
    }

    #[test]
    fn cycles_cause_error() {
        let mut graph = Graph2::new();
        let b = graph.insert_testing();
        let c = graph.insert_testing();
        ensure_height_increases(b, c).unwrap();
        b.add_clean_parent(c);
        ensure_height_increases(c,b).unwrap_err();
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let mut graph = Graph2::new();
        let a = graph.insert_testing();
        let b = graph.insert_testing();
        let c = graph.insert_testing();
        let d = graph.insert_testing();
        let e = graph.insert_testing();

        ensure_height_increases(b, c).unwrap();
        b.add_clean_parent(c);
        ensure_height_increases(c, e).unwrap();
        c.add_clean_parent(e);
        ensure_height_increases(b, d).unwrap();
        b.add_clean_parent(d);
        ensure_height_increases(d, e).unwrap();
        d.add_clean_parent(e);
        ensure_height_increases(a, b).unwrap();
        a.add_clean_parent(b);
    }
}
