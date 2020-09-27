//! Singlethread is Anchors' default execution engine. It's a single threaded engine capable of both
//! Adapton-style pull updates and — if `mark_observed` and `mark_unobserved` are used,
//! Incremental-style push updates.
//!
//! As of Semptember 2020, execution overhead per-node sits at around 100ns on this author's Macbook
//! Air, likely somewhat more if single node has a significant number of parents or children. Hopefully
//! this will significantly improve over the coming months.

use crate::nodequeue::{NodeQueue, NodeState};
use crate::refcounter::RefCounter;
use crate::{graph, Anchor, AnchorInner, OutputContext, Poll, UpdateContext};
use slotmap::SlotMap;
use std::any::Any;
use std::cell::RefCell;
use std::panic::Location;
use std::rc::Rc;

use std::num::NonZeroU64;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct Generation(NonZeroU64);
impl Generation {
    fn new() -> Generation {
        Generation(NonZeroU64::new(1).unwrap())
    }
    fn increment(&mut self) {
        let gen: u64  = u64::from(self.0) + 1;
        self.0 = NonZeroU64::new(gen).unwrap();
    }
}

thread_local! {
    static DEFAULT_MOUNTER: RefCell<Option<Mounter>> = RefCell::new(None);
}

/// Indicates whether the node is a part of some observed calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedState {
    /// The node has been marked as observed directly via `mark_observed`.
    Observed,

    /// The node is not marked as observed directly.
    /// However, the node has some descendent that is Observed, and this node has
    /// been recalculated since that descendent become Observed.
    Necessary,

    /// The node is not marked as observed directly.
    /// Additionally, this node either has no Observed descendent, or the chain linking
    /// this node to that Observed descendent has not been recalculated since that
    /// dencendent become observed.
    Unnecessary,
}

slotmap::new_key_type! {
    /// The AnchorHandle token of the Singlethread engine.
    pub struct NodeNum;
}

/// The main execution engine of Singlethread.
pub struct Engine {
    // TODO store Nodes on heap directly?? maybe try for Rc<RefCell<SlotMap>> now
    nodes: Rc<RefCell<SlotMap<NodeNum, Node>>>,
    graph: graph::MetadataGraph<NodeNum>,
    to_recalculate: NodeQueue<NodeNum>,
    dirty_marks: Rc<RefCell<Vec<NodeNum>>>,
    refcounter: RefCounter<NodeNum>,

    // used internally by mark_dirty. we persist it so we can allocate less
    queue: Vec<NodeNum>,

    // tracks the current stabilization generation; incremented on every stabilize
    generation: Generation,
}

struct Mounter {
    nodes: Rc<RefCell<SlotMap<NodeNum, Node>>>,
    refcounter: RefCounter<NodeNum>,
}

impl crate::Engine for Engine {
    type AnchorHandle = AnchorHandle;
    type DirtyHandle = DirtyHandle;

    fn mount<I: AnchorInner<Self> + 'static>(inner: I) -> Anchor<I::Output, Self> {
        DEFAULT_MOUNTER.with(|default_mounter| {
            let mut borrow1 = default_mounter.borrow_mut();
            let this = borrow1
                .as_mut()
                .expect("no engine was initialized. did you call `Engine::new()`?");
            let debug_info = inner.debug_info();
            let num = this.nodes.borrow_mut().insert(Node {
                observed: false,
                anchor: Rc::new(RefCell::new(inner)),
                debug_info,
                last_ready: None,
                last_update: None,
            });
            this.refcounter.create(num);
            Anchor::new(AnchorHandle {
                num,
                refcounter: this.refcounter.clone(),
            })
        })
    }
}

struct Node {
    observed: bool,
    debug_info: AnchorDebugInfo,
    anchor: Rc<RefCell<dyn GenericAnchor>>,
    /// tracks the generation when this Node last polled as Updated or Unchanged
    last_ready: Option<Generation>,
    /// tracks the generation when this Node last polled as Updated
    last_update: Option<Generation>,
}

impl Engine {
    /// Creates a new Engine with maximum height 256.
    pub fn new() -> Self {
        Self::new_with_max_height(256)
    }

    /// Creates a new Engine with a custom maximum height.
    pub fn new_with_max_height(max_height: usize) -> Self {
        let refcounter = RefCounter::new();
        let nodes = Rc::new(RefCell::new(SlotMap::with_key()));
        let mounter = Mounter {
            refcounter: refcounter.clone(),
            nodes: nodes.clone(),
        };
        DEFAULT_MOUNTER.with(|v| *v.borrow_mut() = Some(mounter));
        Self {
            nodes,
            graph: graph::MetadataGraph::new(),
            to_recalculate: NodeQueue::new(max_height),
            dirty_marks: Default::default(),
            refcounter,
            queue: vec![],
            generation: Generation::new(),
        }
    }

    /// Marks an Anchor as observed. All observed nodes will always be brought up-to-date
    /// when *any* Anchor in the graph is retrieved. If you get an output value fairly
    /// often, it's best to mark it as Observed so that Anchors can calculate its
    /// dependencies faster.
    pub fn mark_observed<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        self.nodes
            .borrow_mut()
            .get_mut(anchor.data.num)
            .unwrap()
            .observed = true;
        if self.to_recalculate.state(anchor.data.num) != NodeState::Ready {
            self.mark_node_for_recalculation(anchor.data.num);
        }
    }

    /// Marks an Anchor as unobserved. If the `anchor` has parents that are necessary
    /// because `anchor` was previously observed, those parents will be unmarked as
    /// necessary.
    pub fn mark_unobserved<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        self.nodes
            .borrow_mut()
            .get_mut(anchor.data.num)
            .unwrap()
            .observed = false;

        let mut queue = vec![anchor.data.num];

        while let Some(next_id) = queue.pop() {
            if self.check_observed(next_id) != ObservedState::Unnecessary {
                // we have another parent still observed, so skip this
                continue;
            }
            let mut necessary_children = self.graph.necessary_children(next_id);
            for child in &necessary_children {
                // TODO this may need to be dirty in some cases?? may want to better track if a value has been
                // changed or not
                let res = self
                    .graph
                    .set_edge_clean(*child, next_id, false);
                self.panic_if_loop(res);
            }
            queue.append(&mut necessary_children);
            // TODO remove from calculation queue if necessary?
        }
    }

    /// Retrieves the value of an Anchor, recalculating dependencies as necessary to get the
    /// latest value.
    pub fn get<'out, O: Clone + 'static>(&mut self, anchor: &Anchor<O, Engine>) -> O {
        // stabilize once before, since the stabilization process may mark our requested node
        // as dirty
        self.stabilize();
        if self.to_recalculate.state(anchor.data.num) != NodeState::Ready {
            self.to_recalculate
                .queue_recalc(self.graph.height(anchor.data.num), anchor.data.num);
            // stabilize again, to make sure our target node that is now in the queue is up-to-date
            // use stabilize0 because no dirty marks have occured since last stabilization, and we want
            // to make sure we don't unnecessarily increment generation number
            self.stabilize0();
        }
        let target_anchor = &self.nodes.borrow()[anchor.data.num].anchor.clone();
        let borrow = target_anchor.borrow();
        borrow
            .output(&mut EngineContext {
                engine: &self,
                node_num: anchor.data.num,
            })
            .downcast_ref::<O>()
            .unwrap()
            .clone()
    }

    pub(crate) fn update_dirty_marks(&mut self) {
        let dirty_marks = std::mem::replace(&mut *self.dirty_marks.borrow_mut(), Vec::new());
        for dirty in dirty_marks {
            self.mark_dirty(dirty, false);
        }
    }

    /// Ensure any Observed nodes are up-to-date, recalculating dependencies as necessary. You
    /// should rarely need to call this yourself; `Engine::get` calls it automatically.
    pub fn stabilize(&mut self) {
        self.update_dirty_marks();
        self.generation.increment();
        self.stabilize0();
    }

    /// internal function for stabilization. does not update dirty marks or increment the stabilization number
    fn stabilize0(&mut self) {
        while let Some((height, this_node_num)) = self.to_recalculate.pop_next() {
            let calculation_complete = if height == self.graph.height(this_node_num) {
                // this nodes height is current, so we can recalculate
                self.recalculate(this_node_num)
            } else {
                // skip calculation, redo at correct height
                false
            };

            if !calculation_complete {
                self.to_recalculate
                    .queue_recalc(self.graph.height(this_node_num), this_node_num);
            }
        }

        self.garbage_collect();
    }

    /// Returns a debug string containing the current state of the recomputation graph.
    pub fn debug_state(&self) -> String {
        let nodes = self.nodes.borrow();
        let mut debug = "".to_string();
        for (node_id, node) in nodes.iter() {
            let necessary = if self.graph.is_necessary(node_id) {
                "necessary"
            } else {
                "   --    "
            };
            let observed = if node.observed {
                "observed"
            } else {
                "   --   "
            };
            let state = match self.to_recalculate.state(node_id) {
                NodeState::NeedsRecalc => "NeedsRecalc  ",
                NodeState::PendingRecalc => "PendingRecalc",
                NodeState::Ready => "Ready        ",
            };
            debug += &format!(
                "{:>80}  {}  {}  {}\n",
                node.debug_info.to_string(),
                necessary,
                observed,
                state
            );
        }
        debug
    }

    /// Returns whether an Anchor is Observed, Necessary, or Unnecessary.
    pub fn check_observed(&self, id: NodeNum) -> ObservedState {
        if self.nodes.borrow().get(id).as_ref().unwrap().observed {
            return ObservedState::Observed;
        }
        if self.graph.is_necessary(id) {
            ObservedState::Necessary
        } else {
            ObservedState::Unnecessary
        }
    }

    fn garbage_collect(&mut self) {
        let graph = &mut self.graph;
        let mut nodes = self.nodes.borrow_mut();
        self.refcounter.drain(|item| {
            graph.remove(item);
            nodes.remove(item);
        });
    }

    /// returns false if calculation is still pending
    fn recalculate(&mut self, this_node_num: NodeNum) -> bool {
        let this_anchor = self
            .nodes
            .borrow()
            .get(this_node_num)
            .unwrap()
            .anchor
            .clone();
        let mut ecx = EngineContextMut {
            engine: self,
            node_num: this_node_num,
            pending_on_anchor_get: false,
        };
        let poll_result = this_anchor.borrow_mut().poll_updated(&mut ecx);
        let pending_on_anchor_get = ecx.pending_on_anchor_get;
        match poll_result {
            Poll::Pending => {
                if pending_on_anchor_get {
                    // looks like we requested an anchor that isn't yet calculated, so we
                    // reinsert into the graph directly; our height either was higher than this
                    // requested anchor's already, or it was updated so it's higher now.
                    false
                } else {
                    // in the future, this means we polled on some non-anchors future. since
                    // that isn't supported for now, this just means something went wrong
                    panic!("poll_updated return pending without requesting another anchor");
                }
            }
            Poll::Updated => {
                // make sure all parents are marked as dirty, and observed parents are recalculated
                self.mark_dirty(this_node_num, true);
                let mut nodes = self.nodes.borrow_mut();
                let node = nodes.get_mut(this_node_num).unwrap();
                node.last_update = Some(self.generation);
                node.last_ready = Some(self.generation);
                true
            }
            Poll::Unchanged => {
                let mut nodes = self.nodes.borrow_mut();
                let node = nodes.get_mut(this_node_num).unwrap();
                node.last_ready = Some(self.generation);
                true
            },
        }
    }

    fn panic_if_loop(&self, res: Result<(), Vec<NodeNum>>) {
        if let Err(loop_ids) = res {
            let mut debug_str = "".to_string();
            for id in &loop_ids {
                let name = self
                    .nodes
                    .borrow()
                    .get(*id)
                    .map(|node| node.debug_info.to_string())
                    .unwrap_or("(unknown node)".to_string());
                debug_str.push_str("\n-> ");
                debug_str.push_str(&name);
            }
            panic!("loop detected:{}\n", debug_str);
        }
    }

    // skip_self = true indicates output has *definitely* changed, but node has been recalculated
    // skip_self = false indicates node has not yet been recalculated
    fn mark_dirty(&mut self, node_id: NodeNum, skip_self: bool) {
        if skip_self {
            if let Some(parents) = self.graph.empty_clean_parents(node_id) {
                self.queue.reserve(parents.size_hint().0);
                for parent in parents {
                    // TODO still calling dirty twice on observed relationships
                    self.nodes
                        .borrow()
                        .get(parent)
                        .unwrap()
                        .anchor
                        .borrow_mut()
                        .dirty(&node_id);
                    self.queue.push(parent);
                }
            }
        } else {
            self.queue.push(node_id);
        };

        while let Some(next) = self.queue.pop() {
            if self.graph.is_necessary(next) || self.nodes.borrow()[next].observed {
                self.mark_node_for_recalculation(next);
            } else if self.to_recalculate.state(next) == NodeState::Ready {
                self.to_recalculate.needs_recalc(next);
                let start_i = self.queue.len();
                if let Some(parents) = self.graph.empty_clean_parents(next) {
                    self.queue.reserve(parents.size_hint().0);
                    for parent in parents {
                        self.nodes
                            .borrow()
                            .get(parent)
                            .unwrap()
                            .anchor
                            .borrow_mut()
                            .dirty(&next);
                        self.queue.push(parent);
                    }
                }
            };
        }
    }

    fn mark_node_for_recalculation(&mut self, node_id: NodeNum) {
        if self.to_recalculate.state(node_id) != NodeState::PendingRecalc {
            self.to_recalculate
                .queue_recalc(self.graph.height(node_id), node_id);
        }
    }
}

/// Singlethread's implementation of Anchors' `AnchorHandle`, the engine-specific handle that sits inside an `Anchor`.
#[derive(Debug)]
pub struct AnchorHandle {
    num: NodeNum,
    refcounter: RefCounter<NodeNum>,
}

impl Clone for AnchorHandle {
    fn clone(&self) -> Self {
        self.refcounter.increment(self.num);
        AnchorHandle {
            num: self.num,
            refcounter: self.refcounter.clone(),
        }
    }
}

impl Drop for AnchorHandle {
    fn drop(&mut self) {
        self.refcounter.decrement(self.num);
    }
}
impl crate::AnchorHandle for AnchorHandle {
    type Token = NodeNum;
    fn token(&self) -> NodeNum {
        self.num
    }
}

/// Singlethread's implementation of Anchors' `DirtyHandle`, which allows a node with non-Anchors inputs to manually mark itself as dirty.
#[derive(Debug, Clone)]
pub struct DirtyHandle {
    num: NodeNum,
    dirty_marks: Rc<RefCell<Vec<NodeNum>>>,
}
impl crate::DirtyHandle for DirtyHandle {
    fn mark_dirty(&self) {
        self.dirty_marks.borrow_mut().push(self.num);
    }
}

struct EngineContext<'eng> {
    engine: &'eng &'eng mut Engine,
    node_num: NodeNum,
}

struct EngineContextMut<'eng> {
    engine: &'eng mut Engine,
    node_num: NodeNum,
    pending_on_anchor_get: bool,
}

impl<'eng> OutputContext<'eng> for EngineContext<'eng> {
    type Engine = Engine;

    fn get<'out, O: 'static>(&self, anchor: &Anchor<O, Self::Engine>) -> &'out O
    where
        'eng: 'out,
    {
        let target_node = &self.engine.nodes.borrow()[anchor.data.num];
        if self.engine.to_recalculate.state(anchor.data.num) != NodeState::Ready
        {
            panic!("attempted to get node that was not previously requested")
        }
        // TODO try to wrap all of this in a safe interface?
        let unsafe_borrow = unsafe { target_node.anchor.as_ptr().as_ref().unwrap() };
        let output: &O = unsafe_borrow
            .output(&mut EngineContext {
                engine: self.engine,
                node_num: anchor.data.num,
            })
            .downcast_ref()
            .unwrap();
        output
    }
}

impl<'eng> UpdateContext for EngineContextMut<'eng> {
    type Engine = Engine;

    fn get<'out, 'slf, O: 'static>(&'slf self, anchor: &Anchor<O, Self::Engine>) -> &'out O
    where
        'slf: 'out,
    {
        let target_node = &self.engine.nodes.borrow()[anchor.data.num];
        if self.engine.to_recalculate.state(anchor.data.num) != NodeState::Ready
        {
            panic!("attempted to get node that was not previously requested")
        }

        let unsafe_borrow = unsafe { target_node.anchor.as_ptr().as_ref().unwrap() };
        let output: &O = unsafe_borrow
            .output(&mut EngineContext {
                engine: &self.engine,
                node_num: anchor.data.num,
            })
            .downcast_ref()
            .unwrap();
        output
    }

    fn request<'out, O: 'static>(
        &mut self,
        anchor: &Anchor<O, Self::Engine>,
        necessary: bool,
    ) -> Poll {
        let height_increases =
            self.engine.graph.height(anchor.data.num) < self.engine.graph.height(self.node_num);
        let self_is_necessary =
            self.engine.check_observed(self.node_num) != ObservedState::Unnecessary;
        if !height_increases {
            let res = self
                .engine
                .graph
                .ensure_height_increases(anchor.data.num, self.node_num);
            self.engine.panic_if_loop(res);
        }

        if self.engine.to_recalculate.state(anchor.data.num) != NodeState::Ready {
            self.pending_on_anchor_get = true;
            self.engine.mark_node_for_recalculation(anchor.data.num);
            if necessary && self_is_necessary {
                let res = self.engine.graph.set_edge_clean(
                    anchor.data.num,
                    self.node_num,
                    true,
                );
                self.engine.panic_if_loop(res);
            }
            Poll::Pending
        } else if !height_increases {
            self.pending_on_anchor_get = true;
            Poll::Pending
        } else {
            if graph::EdgeState::Dirty == self.engine.graph.edge(anchor.data.num, self.node_num) {
                let res = self.engine.graph.set_edge_clean(
                    anchor.data.num,
                    self.node_num,
                    necessary && self_is_necessary,
                );
                self.engine.panic_if_loop(res);
            }
            let nodes = self.engine.nodes.borrow();
            if nodes.get(anchor.data.num).unwrap().last_update > nodes.get(self.node_num).unwrap().last_ready {
                Poll::Updated
            } else {
                Poll::Unchanged
            }
        }
    }

    fn unrequest<'out, O: 'static>(&mut self, anchor: &Anchor<O, Self::Engine>) {
        // TODO SHOULD RECURSE
        self.engine
            .graph
            .set_edge_dirty(anchor.data.num, self.node_num);
    }

    fn dirty_handle(&mut self) -> DirtyHandle {
        DirtyHandle {
            num: self.node_num,
            dirty_marks: self.engine.dirty_marks.clone(),
        }
    }
}

trait GenericAnchor {
    fn dirty(&mut self, child: &NodeNum);
    fn poll_updated<'eng>(&mut self, ctx: &mut EngineContextMut<'eng>) -> Poll;
    fn output<'slf, 'out>(&'slf self, ctx: &mut EngineContext<'out>) -> &'out dyn Any
    where
        'slf: 'out;
    fn debug_info(&self) -> AnchorDebugInfo;
}
impl<I: AnchorInner<Engine> + 'static> GenericAnchor for I {
    fn dirty(&mut self, child: &NodeNum) {
        AnchorInner::dirty(self, child)
    }
    fn poll_updated<'eng>(&mut self, ctx: &mut EngineContextMut<'eng>) -> Poll {
        AnchorInner::poll_updated(self, ctx)
    }
    fn output<'slf, 'out>(&'slf self, ctx: &mut EngineContext<'out>) -> &'out dyn Any
    where
        'slf: 'out,
    {
        AnchorInner::output(self, ctx)
    }
    fn debug_info(&self) -> AnchorDebugInfo {
        AnchorDebugInfo {
            location: self.debug_location(),
            type_info: std::any::type_name::<I>(),
        }
    }
}

#[derive(Debug)]
struct AnchorDebugInfo {
    location: Option<(&'static str, &'static Location<'static>)>,
    type_info: &'static str,
}

impl AnchorDebugInfo {
    fn to_string(&self) -> String {
        match self.location {
            Some((name, location)) => format!("{} ({})", location, name),
            None => format!("{}", self.type_info),
        }
    }
}
