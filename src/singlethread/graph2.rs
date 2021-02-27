use super::{AnchorDebugInfo, Generation, GenericAnchor};
use std::cell::{Cell, RefCell, RefMut};
use std::rc::Rc;

use arena_graph::raw as ag;

use std::iter::Iterator;
use std::marker::PhantomData;

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct NodeGuard<'gg>(ag::NodeGuard<'gg, Node>);

type NodePtr = ag::NodePtr<Node>;

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
    nodes: ag::Graph<Node>,
    token: u32,

    still_alive: Rc<Cell<bool>>,

    /// height -> first node in that height's queue
    recalc_queues: RefCell<Vec<Option<NodePtr>>>,
    recalc_min_height: Cell<usize>,
    recalc_max_height: Cell<usize>,

    /// pointer to head of linked list of free nodes
    free_head: Box<Cell<Option<NodePtr>>>,
}

#[derive(Clone, Copy)]
pub struct Graph2Guard<'gg> {
    nodes: ag::GraphGuard<'gg, Node>,
    graph: &'gg Graph2,
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct NodeKey {
    ptr: NodePtr,
    token: u32,
}

impl !Send for NodeKey {}
impl !Sync for NodeKey {}

pub struct NodePtrs {
    /// first parent, remaining parents. unsorted, duplicates may exist
    clean_parent0: Cell<Option<NodePtr>>,
    clean_parents: RefCell<Vec<NodePtr>>,

    graph: *const Graph2,

    /// Next node in either recalc linked list for this height, or if node is in the free list, the free linked list.
    /// If this is the last node, None.
    next: Cell<Option<NodePtr>>,
    /// Prev node in either recalc linked list for this height, or if node is in the free list, the free linked list.
    /// If this is the head node, None.
    prev: Cell<Option<NodePtr>>,
    recalc_state: Cell<RecalcState>,

    /// sorted in pointer order
    necessary_children: RefCell<Vec<NodePtr>>,

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
            let count = &unsafe { self.num.ptr.lookup_unchecked() }.ptrs.handle_count;
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
            let count = &unsafe { self.num.ptr.lookup_unchecked() }.ptrs.handle_count;
            let new_count = count.get() - 1;
            count.set(new_count);
            std::mem::drop(count);
            if new_count == 0 {
                unsafe { free(self.num.ptr) };
            }
        }
    }
}
impl crate::expert::AnchorHandle for AnchorHandle {
    type Token = NodeKey;
    fn token(&self) -> NodeKey {
        self.num
    }
}

impl<'a> std::ops::Deref for NodeGuard<'a> {
    type Target = Node;
    fn deref(&self) -> &Node {
        &*self.0
    }
}

impl<'a> NodeGuard<'a> {
    pub fn key(self) -> NodeKey {
        NodeKey {
            ptr: unsafe { self.0.make_ptr() },
            token: self.token,
        }
    }

    pub fn add_clean_parent(self, parent: NodeGuard<'a>) {
        if self.ptrs.clean_parent0.get().is_none() {
            self.ptrs
                .clean_parent0
                .set(Some(unsafe { parent.0.make_ptr() }))
        } else {
            self.ptrs
                .clean_parents
                .borrow_mut()
                .push(unsafe { parent.0.make_ptr() })
        }
    }

    pub fn clean_parents(self) -> impl Iterator<Item = NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.0.node().ptrs.clean_parents.borrow_mut(),
            next_i: 0,
            first: self.ptrs.clean_parent0.get(),
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_clean_parents(self) -> impl Iterator<Item = NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.0.node().ptrs.clean_parents.borrow_mut(),
            next_i: 0,
            first: self.ptrs.clean_parent0.take(),
            f: PhantomData,
            empty_on_drop: true,
        }
    }

    pub fn add_necessary_child(self, child: NodeGuard<'a>) {
        let mut necessary_children = self.ptrs.necessary_children.borrow_mut();
        let child_ptr = unsafe { child.0.make_ptr() };
        if let Err(i) = necessary_children.binary_search(&child_ptr) {
            necessary_children.insert(i, child_ptr);
            child.necessary_count.set(child.necessary_count.get() + 1)
        }
    }

    pub fn remove_necessary_child(self, child: NodeGuard<'a>) {
        let mut necessary_children = self.ptrs.necessary_children.borrow_mut();
        let child_ptr = unsafe { child.0.make_ptr() };
        if let Ok(i) = necessary_children.binary_search(&child_ptr) {
            necessary_children.remove(i);
            child.necessary_count.set(child.necessary_count.get() - 1)
        }
    }

    pub fn necessary_children(self) -> impl Iterator<Item = NodeGuard<'a>> {
        RefCellVecIterator {
            inside: self.0.node().ptrs.necessary_children.borrow_mut(),
            next_i: 0,
            first: None,
            f: PhantomData,
            empty_on_drop: false,
        }
    }

    pub fn drain_necessary_children(self) -> impl Iterator<Item = NodeGuard<'a>> {
        let necessary_children = self.0.node().ptrs.necessary_children.borrow_mut();
        for child in &*necessary_children {
            let count = &unsafe { self.0.lookup_ptr(*child) }.necessary_count;
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
    inside: RefMut<'a, Vec<NodePtr>>,
    next_i: usize,
    first: Option<NodePtr>,
    // hack to make RefCellVecIterator invariant
    f: PhantomData<&'a mut &'a ()>,
    empty_on_drop: bool,
}

impl<'a> Iterator for RefCellVecIterator<'a> {
    type Item = NodeGuard<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            return Some(NodeGuard(unsafe { first.lookup_unchecked() }));
        }
        let next = self.inside.get(self.next_i)?;
        self.next_i += 1;
        Some(NodeGuard(unsafe { next.lookup_unchecked() }))
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

impl<'gg> Graph2Guard<'gg> {
    pub fn get(&self, key: NodeKey) -> Option<NodeGuard<'gg>> {
        if key.token != self.graph.token {
            return None;
        }
        Some(NodeGuard(unsafe { self.nodes.lookup_ptr(key.ptr) }))
    }

    #[cfg(test)]
    pub fn insert_testing_guard(&self) -> NodeGuard<'gg> {
        let handle = self.graph.insert_testing();
        let guard = self.get(handle.num).unwrap();
        std::mem::forget(handle);
        guard
    }

    pub fn recalc_pop_next(&self) -> Option<(usize, NodeGuard<'gg>)> {
        let mut recalc_queues = self.graph.recalc_queues.borrow_mut();
        while self.graph.recalc_min_height.get() <= self.graph.recalc_max_height.get() {
            if let Some(ptr) = recalc_queues[self.graph.recalc_min_height.get()] {
                let node = unsafe { self.nodes.lookup_ptr(ptr) };
                recalc_queues[self.graph.recalc_min_height.get()] = node.ptrs.next.get();
                if let Some(next_in_queue_ptr) = node.ptrs.next.get() {
                    unsafe { self.nodes.lookup_ptr(next_in_queue_ptr) }
                        .ptrs
                        .prev
                        .set(None);
                }
                node.ptrs.prev.set(None);
                node.ptrs.next.set(None);
                node.ptrs.recalc_state.set(RecalcState::Ready);
                return Some((self.graph.recalc_min_height.get(), NodeGuard(node)));
            } else {
                self.graph
                    .recalc_min_height
                    .set(self.graph.recalc_min_height.get() + 1);
            }
        }
        self.graph.recalc_max_height.set(0);
        None
    }

    pub fn queue_recalc(&self, node: NodeGuard<'gg>) {
        if node.ptrs.recalc_state.get() == RecalcState::Pending {
            // already in recalc queue
            return;
        }
        node.ptrs.recalc_state.set(RecalcState::Pending);
        let node_height = height(node);
        let mut recalc_queues = self.graph.recalc_queues.borrow_mut();
        if node_height >= recalc_queues.len() {
            panic!("too large height error");
        }
        if let Some(old) = recalc_queues[node_height] {
            unsafe { self.nodes.lookup_ptr(old) }
                .ptrs
                .prev
                .set(Some(unsafe { node.0.make_ptr() }));
            node.ptrs.next.set(Some(old));
        } else {
            if self.graph.recalc_min_height.get() > node_height {
                self.graph.recalc_min_height.set(node_height);
            }
            if self.graph.recalc_max_height.get() < node_height {
                self.graph.recalc_max_height.set(node_height);
            }
        }
        recalc_queues[node_height] = Some(unsafe { node.0.make_ptr() });
    }
}

impl Graph2 {
    pub fn new(max_height: usize) -> Self {
        Self {
            nodes: ag::Graph::new(),
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

    pub fn with<F: for<'any> FnOnce(Graph2Guard<'any>) -> R, R>(&self, func: F) -> R {
        let nodes = unsafe { self.nodes.with_unchecked() };
        func(Graph2Guard { nodes, graph: self })
    }

    #[cfg(test)]
    pub fn insert_testing<'a>(&'a self) -> AnchorHandle {
        self.insert(
            Box::new(crate::expert::Constant::new_raw_testing(123)),
            AnchorDebugInfo {
                location: None,
                type_info: "testing dummy anchor",
            },
        )
    }

    pub(super) fn insert<'a>(
        &'a self,
        anchor: Box<dyn GenericAnchor>,
        debug_info: AnchorDebugInfo,
    ) -> AnchorHandle {
        self.nodes.with(|nodes| {
            let ptr = if let Some(free_head) = self.free_head.get() {
                let node = unsafe { nodes.lookup_ptr(free_head) };
                self.free_head.set(node.ptrs.next.get());
                if let Some(next_ptr) = node.ptrs.next.get() {
                    let next_node = unsafe { nodes.lookup_ptr(next_ptr) };
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
                let node = Node {
                    observed: Cell::new(false),
                    visited: Cell::new(false),
                    necessary_count: Cell::new(0),
                    token: self.token,
                    ptrs: NodePtrs {
                        clean_parent0: Cell::new(None),
                        clean_parents: RefCell::new(vec![]),
                        graph: &*self,
                        next: Cell::new(None),
                        prev: Cell::new(None),
                        recalc_state: Cell::new(RecalcState::Needed),
                        necessary_children: RefCell::new(vec![]),
                        height: Cell::new(0),
                        handle_count: Cell::new(1),
                    },
                    debug_info: Cell::new(debug_info),
                    last_ready: Cell::new(None),
                    last_update: Cell::new(None),
                    anchor: RefCell::new(Some(anchor)),
                };
                nodes.insert(node)
            };
            let num = NodeKey {
                ptr: unsafe { ptr.make_ptr() },
                token: self.token,
            };
            AnchorHandle {
                num,
                still_alive: self.still_alive.clone(),
            }
        })
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

fn dequeue_calc<'a>(graph: &Graph2, node: NodeGuard<'a>) {
    if node.ptrs.recalc_state.get() != RecalcState::Pending {
        return;
    }
    if let Some(prev) = node.ptrs.prev.get() {
        unsafe { prev.lookup_unchecked() }
            .ptrs
            .next
            .set(node.ptrs.next.get());
    } else {
        // node was first in queue, need to set queue head to next
        let mut recalc_queues = graph.recalc_queues.borrow_mut();
        let height = node.ptrs.height.get();
        let next = node.ptrs.next.get();
        assert_eq!(
            recalc_queues[height].map(|ptr| unsafe { ptr.lookup_unchecked() }),
            Some(node.0)
        );
        recalc_queues[height] = next;
    }

    if let Some(next) = node.ptrs.next.get() {
        unsafe { next.lookup_unchecked() }
            .ptrs
            .next
            .set(node.ptrs.prev.get());
    }

    node.ptrs.prev.set(None);
    node.ptrs.next.set(None);
}

unsafe fn free(ptr: NodePtr) {
    let guard = NodeGuard(ptr.lookup_unchecked());
    let _ = guard.drain_necessary_children();
    let _ = guard.drain_clean_parents();
    let graph = &*(*guard).ptrs.graph;
    dequeue_calc(graph, guard);
    // TODO clear out this node with default empty data
    // TODO add node to chain of free nodes
    let free_head = &graph.free_head;
    let old_free = free_head.get();
    if let Some(old_free) = old_free {
        guard.0.lookup_ptr(old_free).ptrs.prev.set(Some(ptr));
    }
    guard.ptrs.next.set(old_free);
    free_head.set(Some(ptr));

    // "SAFETY": this may cause other nodes to be dropped, so do with care
    *guard.anchor.borrow_mut() = None;
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
        graph.with(|guard| {
            let a = guard.insert_testing_guard();
            let b = guard.insert_testing_guard();
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
        });
    }

    #[test]
    fn height_calculated_correctly() {
        let graph = Graph2::new(256);
        graph.with(|guard| {
            let a = guard.insert_testing_guard();
            let b = guard.insert_testing_guard();
            let c = guard.insert_testing_guard();

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
        })
    }

    #[test]
    fn cycles_cause_error() {
        let graph = Graph2::new(256);
        graph.with(|guard| {
            let b = guard.insert_testing_guard();
            let c = guard.insert_testing_guard();
            ensure_height_increases(b, c).unwrap();
            b.add_clean_parent(c);
            ensure_height_increases(c, b).unwrap_err();
        })
    }

    #[test]
    fn non_cycles_wont_cause_errors() {
        let graph = Graph2::new(256);
        graph.with(|guard| {
            let a = guard.insert_testing_guard();
            let b = guard.insert_testing_guard();
            let c = guard.insert_testing_guard();
            let d = guard.insert_testing_guard();
            let e = guard.insert_testing_guard();

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
        })
    }

    #[test]
    fn test_insert_pop() {
        let graph = Graph2::new(10);
        graph.with(|guard| {
            let a = guard.insert_testing_guard();
            set_min_height(a, 0).unwrap();
            let b = guard.insert_testing_guard();
            set_min_height(b, 5).unwrap();
            let c = guard.insert_testing_guard();
            set_min_height(c, 3).unwrap();
            let d = guard.insert_testing_guard();
            set_min_height(d, 4).unwrap();
            let e = guard.insert_testing_guard();
            set_min_height(e, 1).unwrap();
            let e2 = guard.insert_testing_guard();
            set_min_height(e2, 1).unwrap();
            let e3 = guard.insert_testing_guard();
            set_min_height(e3, 1).unwrap();

            guard.queue_recalc(a);
            guard.queue_recalc(a);
            guard.queue_recalc(a);
            guard.queue_recalc(b);
            guard.queue_recalc(c);
            guard.queue_recalc(d);

            assert_eq!(Some(a), guard.recalc_pop_next().map(|(_, v)| v));
            assert_eq!(Some(c), guard.recalc_pop_next().map(|(_, v)| v));
            assert_eq!(Some(d), guard.recalc_pop_next().map(|(_, v)| v));

            guard.queue_recalc(e);
            guard.queue_recalc(e2);
            guard.queue_recalc(e3);

            assert_eq!(Some(e3), guard.recalc_pop_next().map(|(_, v)| v));
            assert_eq!(Some(e2), guard.recalc_pop_next().map(|(_, v)| v));
            assert_eq!(Some(e), guard.recalc_pop_next().map(|(_, v)| v));
            assert_eq!(Some(b), guard.recalc_pop_next().map(|(_, v)| v));

            assert_eq!(None, guard.recalc_pop_next().map(|(_, v)| v));
        })
    }

    #[test]
    #[should_panic]
    fn test_insert_above_max_height() {
        let graph = Graph2::new(10);
        graph.with(|guard| {
            let a = guard.insert_testing_guard();
            set_min_height(a, 10).unwrap();
            guard.queue_recalc(a);
        })
    }

    #[test]
    fn test_free_list() {
        use crate::expert::AnchorHandle;
        let graph = Graph2::new(10);
        let a = graph.insert_testing();
        let b = graph.insert_testing();
        let c = graph.insert_testing();

        let a_token = a.token();
        let b_token = b.token();
        let c_token = c.token();

        std::mem::drop(a);
        std::mem::drop(b);
        std::mem::drop(c);

        let c = graph.insert_testing();
        let b = graph.insert_testing();
        let a = graph.insert_testing();
        let d = graph.insert_testing();

        assert_eq!(a_token, a.token());
        assert_eq!(b_token, b.token());
        assert_eq!(c_token, c.token());
        let d_token = d.token();

        std::mem::drop(c);
        std::mem::drop(a);
        std::mem::drop(b);
        std::mem::drop(d);

        let d = graph.insert_testing();
        let b = graph.insert_testing();
        let a = graph.insert_testing();
        let c = graph.insert_testing();

        assert_eq!(a_token, a.token());
        assert_eq!(b_token, b.token());
        assert_eq!(c_token, c.token());
        assert_eq!(d_token, d.token());
    }
}
