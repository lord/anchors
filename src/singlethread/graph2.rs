use super::{AnchorDebugInfo, Generation, GenericAnchor};
use std::cell::{Cell, RefCell, RefMut};
use std::rc::Rc;
use typed_arena::Arena;

use std::iter::Iterator;
use std::marker::PhantomData;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RecalcState {
    Needed,
    Pending,
    Ready,
}

impl Default for RecalcState {
    fn default() -> Self {
        RecalcState::Needed
    }
}

thread_local! {
    pub static NEXT_TOKEN: Cell<u32> = Cell::new(0);
}

pub struct Graph2 {
    nodes: Arena<Node>,
    token: u32,

    /// height -> first node in that height's queue
    recalc_queues: RefCell<Vec<Option<*const Node>>>,
    recalc_min_height: Cell<usize>,
    recalc_max_height: Cell<usize>,
}

pub struct Node {
    pub observed: Cell<bool>,

    /// bool used during height incrementing to check for loops
    pub visited: Cell<bool>,

    /// number of nodes that list this node as a necessary child
    pub necessary_count: Cell<usize>,

    pub token: u32,

    pub(super) debug_info: Cell<AnchorDebugInfo>,

    /// tracks the generation when this Node last polled as Updated or Unchanged
    pub(super) last_ready: Cell<Option<Generation>>,
    /// tracks the generation when this Node last polled as Updated
    pub(super) last_update: Cell<Option<Generation>>,

    /// Some() if this node is still active, None otherwise
    pub(super) anchor: RefCell<Option<Box<dyn GenericAnchor>>>,

    pub ptrs: NodePtrs,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct NodeKey {
    ptr: *const Node,
    token: u32,
}

impl !Send for NodeKey{}
impl !Sync for NodeKey{}
impl !Send for NodeGuard<'_>{}
impl !Sync for NodeGuard<'_>{}
impl !Send for Graph2{}
impl !Sync for Graph2{}

#[derive(Default)]
pub struct NodePtrs {
    /// first parent, remaining parents. unsorted, duplicates may exist
    clean_parent0: Cell<Option<*const Node>>,
    clean_parents: RefCell<Vec<*const Node>>,

    /// Next node in recalc linked list for this height. If this is the last node, None.
    recalc_next: Cell<Option<*const Node>>,
    /// Prev node in recalc linked list for this height. IF this is the head node, None.
    recalc_prev: Cell<Option<*const Node>>,
    recalc_state: Cell<RecalcState>,

    /// sorted in pointer order
    necessary_children: RefCell<Vec<*const Node>>,

    height: Cell<usize>,
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
            .field("inside.key", &self.key())
            .finish()
    }
}

impl PartialEq for NodeGuard<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.inside, other.inside)
    }
}

impl<'a> std::ops::Deref for NodeGuard<'a> {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.inside
    }
}

impl<'a> NodeGuard<'a> {
    pub fn key(self) -> NodeKey {
        NodeKey {
            ptr: self.inside as *const Node,
            token: self.token,
        }
    }

    pub fn add_clean_parent(self, parent: NodeGuard<'a>) {
        if self.inside.ptrs.clean_parent0.get().is_none() {
            self.inside
                .ptrs
                .clean_parent0
                .set(Some(parent.inside as *const Node))
        } else {
            self.inside
                .ptrs
                .clean_parents
                .borrow_mut()
                .push(parent.inside)
        }
    }

    pub fn clean_parents(self) -> impl Iterator<Item = NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.inside.ptrs.clean_parents.borrow_mut(),
            next_i: 0,
            first: self.inside.ptrs.clean_parent0.get(),
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_clean_parents(self) -> impl Iterator<Item = NodeGuard<'a>> {
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
            child
                .inside
                .necessary_count
                .set(child.inside.necessary_count.get() + 1)
        }
    }

    pub fn remove_necessary_child(self, child: NodeGuard<'a>) {
        let mut necessary_children = self.inside.ptrs.necessary_children.borrow_mut();
        if let Ok(i) = necessary_children.binary_search(&(child.inside as *const Node)) {
            necessary_children.remove(i);
            child
                .inside
                .necessary_count
                .set(child.inside.necessary_count.get() - 1)
        }
    }

    pub fn necessary_children(self) -> impl Iterator<Item = NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.inside.ptrs.necessary_children.borrow_mut(),
            next_i: 0,
            first: None,
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_necessary_children(self) -> impl Iterator<Item = NodeGuard<'a>> {
        let necessary_children = self.inside.ptrs.necessary_children.borrow_mut();
        for child in &*necessary_children {
            let count = &unsafe { &**child }.necessary_count;
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

impl<'a> Iterator for RefCellVecIterator<'a> {
    type Item = NodeGuard<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            return Some(NodeGuard {
                inside: unsafe { &*first },
                f: self.f,
            });
        }
        let next = self.inside.get(self.next_i)?;
        self.next_i += 1;
        Some(NodeGuard {
            inside: unsafe { &**next },
            f: self.f,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut remaining = self.inside.len() - self.next_i;

        if self.first.is_some() {
            remaining += 1;
        }

        (remaining, Some(remaining))
    }
}

impl<'a> Drop for RefCellVecIterator<'a> {
    fn drop(&mut self) {
        if self.empty_on_drop {
            self.inside.clear()
        }
    }
}

impl Graph2 {
    pub fn new(max_height: usize) -> Self {
        Self {
            nodes: Arena::new(),
            token: NEXT_TOKEN.with(|token| {
                let n = token.get();
                token.set(n + 1);
                n
            }),
            recalc_queues: RefCell::new(vec![None; max_height]),
            recalc_min_height: Cell::new(max_height),
            recalc_max_height: Cell::new(0),
        }
    }

    #[cfg(test)]
    pub fn insert_testing<'a>(&'a self) -> NodeGuard<'a> {
        let key = self.insert(
            Box::new(crate::constant::Constant::new_raw_testing(
                123,
            )),
            AnchorDebugInfo {
                location: None,
                type_info: "testing dummy anchor",
            },
        );
        self.get(key).unwrap()
    }

    pub(super) fn insert<'a>(
        &'a self,
        anchor: Box<dyn GenericAnchor>,
        debug_info: AnchorDebugInfo,
    ) -> NodeKey {
        let mut node = Node {
            observed: Cell::new(false),
            visited: Cell::new(false),
            necessary_count: Cell::new(0),
            token: self.token,
            ptrs: NodePtrs::default(),
            debug_info: Cell::new(debug_info),
            last_ready: Cell::new(None),
            last_update: Cell::new(None),
            anchor: RefCell::new(Some(anchor)),
        };
        // SAFETY: ensure ptrs struct is empty on insert
        // TODO this probably is not actually necessary if there's no way to take a Node out of the graph
        node.ptrs = NodePtrs::default();
        let ptr = self.nodes.alloc(node);
        NodeKey {
            ptr: ptr as *const Node,
            token: self.token,
        }
    }

    pub fn get<'a>(&'a self, key: NodeKey) -> Option<NodeGuard<'a>> {
        if key.token != self.token {
            return None;
        }
        Some(NodeGuard {
            inside: unsafe { &*key.ptr },
            f: PhantomData,
        })
    }

    pub fn queue_recalc<'a>(&'a self, node: NodeGuard<'a>) {
        if node.ptrs.recalc_state.get() == RecalcState::Pending {
            // already in recalc queue
            return;
        }
        node.ptrs.recalc_state.set(RecalcState::Pending);
        let node_height = height(node);
        let mut recalc_queues = self.recalc_queues.borrow_mut();
        if node_height >= recalc_queues.len() {
            panic!("too large height error");
        }
        if let Some(old) = recalc_queues[node_height] {
            unsafe {&*old}.ptrs.recalc_prev.set(Some(node.inside as *const Node));
            node.ptrs.recalc_next.set(Some(old as *const Node));
        } else {
            if self.recalc_min_height.get() > node_height {
                self.recalc_min_height.set(node_height);
            }
            if self.recalc_max_height.get() < node_height {
                self.recalc_max_height.set(node_height);
            }
        }
        recalc_queues[node_height] = Some(node.inside as *const Node);
    }

    pub fn recalc_pop_next<'a>(&'a self) -> Option<(usize, NodeGuard<'a>)> {
        let mut recalc_queues = self.recalc_queues.borrow_mut();
        while self.recalc_min_height.get() <= self.recalc_max_height.get() {
            if let Some(ptr) = recalc_queues[self.recalc_min_height.get()] {
                let node = unsafe { &*ptr };
                recalc_queues[self.recalc_min_height.get()] = node.ptrs.recalc_next.get();
                if let Some(next_in_queue_ptr) = node.ptrs.recalc_next.get() {
                    unsafe {&*next_in_queue_ptr}.ptrs.recalc_prev.set(None);
                }
                node.ptrs.recalc_prev.set(None);
                node.ptrs.recalc_next.set(None);
                node.ptrs.recalc_state.set(RecalcState::Ready);
                return Some((self.recalc_min_height.get(), NodeGuard {
                    inside: node,
                    f: PhantomData,
                }));
            } else {
                self.recalc_min_height.set(self.recalc_min_height.get() + 1);
            }
        }
        self.recalc_max_height.set(0);
        None
    }
}

pub fn ensure_height_increases<'a>(
    child: NodeGuard<'a>,
    parent: NodeGuard<'a>,
) -> Result<bool, ()> {
    if height(child) < height(parent) {
        return Ok(true);
    }
    child.visited.set(true);
    let res = set_min_height(parent, height(child) + 1);
    child.visited.set(false);
    res.map(|()| false)
}

fn set_min_height<'a>(node: NodeGuard<'a>, min_height: usize) -> Result<(), ()> {
    if node.visited.get() {
        return Err(());
    }
    node.visited.set(true);
    if height(node) < min_height {
        node.ptrs.height.set(min_height);
        let mut did_err = false;
        for parent in node.clean_parents() {
            if let Err(_loop_ids) = set_min_height(parent, min_height + 1) {
                did_err = true;
            }
        }
        if did_err {
            return Err(());
        }
    }
    node.visited.set(false);
    Ok(())
}

pub fn height<'a>(node: NodeGuard<'a>) -> usize {
    node.ptrs.height.get()
}

pub fn needs_recalc<'a>(node: NodeGuard<'a>) {
    if node.ptrs.recalc_state.get() != RecalcState::Ready {
        // already in recalc queue, or already pending recalc
        return;
    }
    node.ptrs.recalc_state.set(RecalcState::Needed);
}

pub fn recalc_state<'a>(node: NodeGuard<'a>) -> RecalcState {
    node.ptrs.recalc_state.get()
}

#[cfg(test)]
mod test {
    use super::*;

    fn to_vec<I: std::iter::Iterator>(iter: I) -> Vec<I::Item> {
        iter.collect()
    }

    #[test]
    fn set_edge_updates_correctly() {
        let graph = Graph2::new(256);
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

        let _ = a.drain_clean_parents();

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(empty, to_vec(a.clean_parents()));
        assert_eq!(vec![a], to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(true, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);

        let _ = b.drain_necessary_children();

        assert_eq!(empty, to_vec(a.necessary_children()));
        assert_eq!(empty, to_vec(a.clean_parents()));
        assert_eq!(empty, to_vec(b.necessary_children()));
        assert_eq!(empty, to_vec(b.clean_parents()));
        assert_eq!(false, a.necessary_count.get() > 0);
        assert_eq!(false, b.necessary_count.get() > 0);
    }

    #[test]
    fn height_calculated_correctly() {
        let graph = Graph2::new(256);
        let a = graph.insert_testing();
        let b = graph.insert_testing();
        let c = graph.insert_testing();

        assert_eq!(0, height(a));
        assert_eq!(0, height(b));
        assert_eq!(0, height(c));

        assert_eq!(Ok(false), ensure_height_increases(b, c));
        assert_eq!(Ok(true), ensure_height_increases(b, c));
        b.add_clean_parent(c);

        assert_eq!(0, height(a));
        assert_eq!(0, height(b));
        assert_eq!(1, height(c));

        assert_eq!(Ok(false), ensure_height_increases(a, b));
        assert_eq!(Ok(true), ensure_height_increases(a, b));
        a.add_clean_parent(b);

        assert_eq!(0, height(a));
        assert_eq!(1, height(b));
        assert_eq!(2, height(c));

        let _ = a.drain_clean_parents();

        assert_eq!(0, height(a));
        assert_eq!(1, height(b));
        assert_eq!(2, height(c));
    }

    #[test]
    fn cycles_cause_error() {
        let graph = Graph2::new(256);
        let b = graph.insert_testing();
        let c = graph.insert_testing();
        ensure_height_increases(b, c).unwrap();
        b.add_clean_parent(c);
        ensure_height_increases(c, b).unwrap_err();
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let graph = Graph2::new(256);
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

    #[test]
    fn test_insert_pop() {
        let graph = Graph2::new(10);

        let a = graph.insert_testing();
        set_min_height(a, 0).unwrap();
        let b = graph.insert_testing();
        set_min_height(b, 5).unwrap();
        let c = graph.insert_testing();
        set_min_height(c, 3).unwrap();
        let d = graph.insert_testing();
        set_min_height(d, 4).unwrap();
        let e = graph.insert_testing();
        set_min_height(e, 1).unwrap();
        let e2 = graph.insert_testing();
        set_min_height(e2, 1).unwrap();
        let e3 = graph.insert_testing();
        set_min_height(e3, 1).unwrap();

        graph.queue_recalc(a);
        graph.queue_recalc(a);
        graph.queue_recalc(a);
        graph.queue_recalc(b);
        graph.queue_recalc(c);
        graph.queue_recalc(d);

        assert_eq!(Some(a), graph.recalc_pop_next().map(|(_, v)| v));
        assert_eq!(Some(c), graph.recalc_pop_next().map(|(_, v)| v));
        assert_eq!(Some(d), graph.recalc_pop_next().map(|(_, v)| v));

        graph.queue_recalc(e);
        graph.queue_recalc(e2);
        graph.queue_recalc(e3);

        assert_eq!(Some(e3), graph.recalc_pop_next().map(|(_, v)| v));
        assert_eq!(Some(e2), graph.recalc_pop_next().map(|(_, v)| v));
        assert_eq!(Some(e), graph.recalc_pop_next().map(|(_, v)| v));
        assert_eq!(Some(b), graph.recalc_pop_next().map(|(_, v)| v));

        assert_eq!(None, graph.recalc_pop_next().map(|(_, v)| v));
    }

    #[test]
    #[should_panic]
    fn test_insert_above_max_height() {
        let graph = Graph2::new(10);
        let a = graph.insert_testing();
        set_min_height(a, 10).unwrap();
        graph.queue_recalc(a);
    }
}
