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

    still_alive: Rc<Cell<bool>>,

    /// height -> first node in that height's queue
    recalc_queues: RefCell<Vec<Option<*const Node>>>,
    recalc_min_height: Cell<usize>,
    recalc_max_height: Cell<usize>,

    /// pointer to head of linked list of free nodes
    free_head: Box<Cell<Option<*const Node>>>,
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

pub struct NodePtrs {
    /// first parent, remaining parents. unsorted, duplicates may exist
    clean_parent0: Cell<Option<*const Node>>,
    clean_parents: RefCell<Vec<*const Node>>,

    free_head: *const Cell<Option<*const Node>>,

    /// Next node in either recalc linked list for this height, or if node is in the free list, the free linked list. If this is the last node, None.
    next: Cell<Option<*const Node>>,
    /// Prev node in either recalc linked list for this height, or if node is in the free list, the free linked list. IF this is the head node, None.
    prev: Cell<Option<*const Node>>,
    recalc_state: Cell<RecalcState>,

    /// sorted in pointer order
    necessary_children: RefCell<Vec<*const Node>>,

    height: Cell<usize>,

    handle_count: Cell<usize>,
}

/// Singlethread's implementation of Anchors' `AnchorHandle`, the engine-specific handle that sits inside an `Anchor`.
#[derive(Debug)]
pub struct AnchorHandle {
    num: NodeKey,
    still_alive: Rc<Cell<bool>>,
}

impl Clone for AnchorHandle {
    fn clone(&self) -> Self {
        if self.still_alive.get() {
            let count = &unsafe {&*self.num.ptr}.ptrs.handle_count;
            count.set(count.get() + 1);
        }
        AnchorHandle {
            num: self.num.clone(),
            still_alive: self.still_alive.clone(),
        }
    }
}

impl Drop for AnchorHandle {
    fn drop(&mut self) {
        if self.still_alive.get() {
            let count = &unsafe {&*self.num.ptr}.ptrs.handle_count;
            let new_count = count.get() - 1;
            count.set(new_count);
            if new_count == 0 {
                unsafe { free(self.num.ptr) };
            }
        }
    }
}
impl crate::AnchorHandle for AnchorHandle {
    type Token = NodeKey;
    fn token(&self) -> NodeKey {
        self.num
    }
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
            still_alive: Rc::new(Cell::new(true)),
            free_head: Box::new(Cell::new(None)),
        }
    }

    #[cfg(test)]
    pub fn insert_testing<'a>(&'a self) -> NodeGuard<'a> {
        let handle = self.insert(
            Box::new(crate::constant::Constant::new_raw_testing(
                123,
            )),
            AnchorDebugInfo {
                location: None,
                type_info: "testing dummy anchor",
            },
        );
        let guard = self.get(handle.num).unwrap();
        // for testing purposes, make sure we never drop the handle
        std::mem::forget(handle);
        guard
    }

    pub(super) fn insert<'a>(
        &'a self,
        anchor: Box<dyn GenericAnchor>,
        debug_info: AnchorDebugInfo,
    ) -> AnchorHandle {
        let ptr = if let Some(free_head) = self.free_head.get() {
            let node = unsafe {&*free_head};
            self.free_head.set(node.ptrs.next.get());
            if let Some(next_ptr) = node.ptrs.next.get() {
                let next_node = unsafe {&*next_ptr};
                next_node.ptrs.prev.set(None);
            }
            node.observed.set(false);
            node.visited.set(false);
            node.necessary_count.set(0);
            node.ptrs.clean_parent0.set(None);
            node.ptrs.clean_parents.replace(vec![]);
            node.ptrs.recalc_state.set(RecalcState::Needed);
            node.ptrs.necessary_children.replace(vec![]);
            node.ptrs.height.set(0);
            node.ptrs.handle_count.set(1);
            node.ptrs.prev.set(None);
            node.ptrs.next.set(None);
            node.debug_info.set(debug_info);
            node.last_ready.set(None);
            node.last_update.set(None);
            node.anchor.replace(Some(anchor));
            node
        } else {
            let mut node = Node {
                observed: Cell::new(false),
                visited: Cell::new(false),
                necessary_count: Cell::new(0),
                token: self.token,
                ptrs: NodePtrs {
                    clean_parent0: Cell::new(None),
                    clean_parents: RefCell::new(vec![]),
                    next: Cell::new(None),
                    prev: Cell::new(None),
                    recalc_state: Cell::new(RecalcState::Needed),
                    necessary_children: RefCell::new(vec![]),
                    height: Cell::new(0),
                    handle_count: Cell::new(1),
                    free_head: &*self.free_head
                },
                debug_info: Cell::new(debug_info),
                last_ready: Cell::new(None),
                last_update: Cell::new(None),
                anchor: RefCell::new(Some(anchor)),
            };
            self.nodes.alloc(node)
        };
        let num = NodeKey {
            ptr: ptr as *const Node,
            token: self.token,
        };
        AnchorHandle {num, still_alive: self.still_alive.clone()}
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
            unsafe {&*old}.ptrs.prev.set(Some(node.inside as *const Node));
            node.ptrs.next.set(Some(old as *const Node));
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
                recalc_queues[self.recalc_min_height.get()] = node.ptrs.next.get();
                if let Some(next_in_queue_ptr) = node.ptrs.next.get() {
                    unsafe {&*next_in_queue_ptr}.ptrs.prev.set(None);
                }
                node.ptrs.prev.set(None);
                node.ptrs.next.set(None);
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

impl Drop for Graph2 {
    fn drop(&mut self) {
        self.still_alive.set(false);
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

fn dequeue_calc<'a>(guard: NodeGuard<'a>) {
    if guard.ptrs.recalc_state.get() != RecalcState::Pending {
        return;
    }
    unimplemented!()
}

unsafe fn free(ptr: *const Node) {
    let guard = NodeGuard {f: PhantomData, inside: &*ptr};
    let _ = guard.drain_necessary_children();
    let _ = guard.drain_clean_parents();
    dequeue_calc(guard);
    // TODO clear out this node with default empty data
    // TODO add node to chain of free nodes
    let free_head = &*guard.inside.ptrs.free_head;
    let old_free = free_head.get();
    if let Some(old_free) = old_free {
        (*old_free).ptrs.prev.set(Some(ptr));
    }
    guard.inside.ptrs.next.set(old_free);
    free_head.set(Some(ptr));

    // "SAFETY": this may cause other nodes to be dropped, so do with care
    *guard.inside.anchor.borrow_mut() = None;
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
