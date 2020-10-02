//! Singlethread is Anchors' default execution engine. It's a single threaded engine capable of both
//! Adapton-style pull updates and — if `mark_observed` and `mark_unobserved` are used,
//! Incremental-style push updates.
//!
//! As of Semptember 2020, execution overhead per-node sits at around 100ns on this author's Macbook
//! Air, likely somewhat more if single node has a significant number of parents or children. Hopefully
//! this will significantly improve over the coming months.

pub mod graph2;

use graph2::{Graph2, NodeGuard, RecalcState, NodeKey};

pub use graph2::AnchorHandle;

use crate::refcounter::RefCounter;
use crate::{Anchor, AnchorInner, OutputContext, Poll, UpdateContext};

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
        let gen: u64 = u64::from(self.0) + 1;
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

/// The main execution engine of Singlethread.
pub struct Engine {
    // TODO store Nodes on heap directly?? maybe try for Rc<RefCell<SlotMap>> now
    graph: Rc<Graph2>,
    dirty_marks: Rc<RefCell<Vec<NodeKey>>>,
    refcounter: RefCounter<NodeKey>,

    // tracks the current stabilization generation; incremented on every stabilize
    generation: Generation,
}

struct Mounter {
    graph: Rc<Graph2>,
    refcounter: RefCounter<NodeKey>,
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
            let handle = this.graph.insert(Box::new(inner), debug_info);
            Anchor::new(handle)
        })
    }
}

impl Engine {
    /// Creates a new Engine with maximum height 256.
    pub fn new() -> Self {
        Self::new_with_max_height(256)
    }

    /// Creates a new Engine with a custom maximum height.
    pub fn new_with_max_height(max_height: usize) -> Self {
        let refcounter = RefCounter::new();
        let graph = Rc::new(Graph2::new(max_height));
        let mounter = Mounter {
            refcounter: refcounter.clone(),
            graph: graph.clone(),
        };
        DEFAULT_MOUNTER.with(|v| *v.borrow_mut() = Some(mounter));
        Self {
            graph,
            dirty_marks: Default::default(),
            refcounter,
            generation: Generation::new(),
        }
    }

    /// Marks an Anchor as observed. All observed nodes will always be brought up-to-date
    /// when *any* Anchor in the graph is retrieved. If you get an output value fairly
    /// often, it's best to mark it as Observed so that Anchors can calculate its
    /// dependencies faster.
    pub fn mark_observed<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        let node = self.graph.get(anchor.token()).unwrap();
        node.observed.set(true);
        if graph2::recalc_state(node) != RecalcState::Ready {
            self.graph.queue_recalc(node);
        }
    }

    /// Marks an Anchor as unobserved. If the `anchor` has parents that are necessary
    /// because `anchor` was previously observed, those parents will be unmarked as
    /// necessary.
    pub fn mark_unobserved<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        let node = self.graph.get(anchor.token()).unwrap();
        node.observed.set(false);
        Self::update_necessary_children(node);
    }

    fn update_necessary_children<'a>(node: NodeGuard<'a>) {
        if Self::check_observed_raw(node) != ObservedState::Unnecessary {
            // we have another parent still observed, so skip this
            return;
        }
        for child in node.drain_necessary_children() {
            // TODO remove from calculation queue if necessary?
            Self::update_necessary_children(child);
        }
    }

    /// Retrieves the value of an Anchor, recalculating dependencies as necessary to get the
    /// latest value.
    pub fn get<'out, O: Clone + 'static>(&mut self, anchor: &Anchor<O, Engine>) -> O {
        // stabilize once before, since the stabilization process may mark our requested node
        // as dirty
        self.stabilize();
        let anchor_node = self.graph.get(anchor.token()).unwrap();
        if graph2::recalc_state(anchor_node) != RecalcState::Ready {
            self.graph.queue_recalc(anchor_node);
            // stabilize again, to make sure our target node that is now in the queue is up-to-date
            // use stabilize0 because no dirty marks have occured since last stabilization, and we want
            // to make sure we don't unnecessarily increment generation number
            self.stabilize0();
        }
        let target_anchor = &self.graph.get(anchor.token()).unwrap().anchor;
        let borrow = target_anchor.borrow();
        borrow
            .as_ref()
            .unwrap()
            .output(&mut EngineContext {
                engine: &self,
            })
            .downcast_ref::<O>()
            .unwrap()
            .clone()
    }

    pub(crate) fn update_dirty_marks(&mut self) {
        let dirty_marks = std::mem::replace(&mut *self.dirty_marks.borrow_mut(), Vec::new());
        for dirty in dirty_marks {
            let node = self.graph.get(dirty).unwrap();
            self.mark_dirty(node, false);
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
        while let Some((height, node)) = self.graph.recalc_pop_next() {
            let calculation_complete = if graph2::height(node) == height {
                // TODO with new graph we can automatically relocate nodes if their height changes
                // this nodes height is current, so we can recalculate
                self.recalculate(node)
            } else {
                // skip calculation, redo at correct height
                false
            };

            if !calculation_complete {
                self.graph.queue_recalc(node);
            }
        }

        self.garbage_collect();
    }

    /// Returns a debug string containing the current state of the recomputation graph.
    pub fn debug_state(&self) -> String {
        let debug = "".to_string();
        // for (node_id, _) in nodes.iter() {
        //     let node = self.graph.get(node_id).unwrap();
        //     let necessary = if self.graph.is_necessary(node_id) {
        //         "necessary"
        //     } else {
        //         "   --    "
        //     };
        //     let observed = if Self::check_observed_raw(node) == ObservedState::Observed {
        //         "observed"
        //     } else {
        //         "   --   "
        //     };
        //     let state = match self.to_recalculate.borrow_mut().state(node_id) {
        //         RecalcState::NeedsRecalc => "NeedsRecalc  ",
        //         RecalcState::PendingRecalc => "PendingRecalc",
        //         RecalcState::Ready => "Ready        ",
        //     };
        //     debug += &format!(
        //         "{:>80}  {}  {}  {}\n",
        //         node.debug_info.get().to_string(),
        //         necessary,
        //         observed,
        //         state
        //     );
        // }
        debug
    }

    pub fn check_observed<T>(&self, anchor: &Anchor<T, Engine>) -> ObservedState {
        let node = self.graph.get(anchor.token()).unwrap();
        Self::check_observed_raw(node)
    }

    /// Returns whether an Anchor is Observed, Necessary, or Unnecessary.
    pub fn check_observed_raw<'a>(node: NodeGuard<'a>) -> ObservedState {
        if node.observed.get() {
            return ObservedState::Observed;
        }
        if node.necessary_count.get() > 0 {
            ObservedState::Necessary
        } else {
            ObservedState::Unnecessary
        }
    }

    fn garbage_collect(&mut self) {
        let _graph = &mut self.graph;
        self.refcounter.drain(|_item| {
            // TODO REMOVE NODES
        });
    }

    /// returns false if calculation is still pending
    fn recalculate<'a>(&'a self, node: NodeGuard<'a>) -> bool {
        let this_anchor = &node.anchor;
        let mut ecx = EngineContextMut {
            engine: self,
            node: node,
            pending_on_anchor_get: false,
        };
        let poll_result = this_anchor.borrow_mut().as_mut().unwrap().poll_updated(&mut ecx);
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
                self.mark_dirty(node, true);
                node.last_update.set(Some(self.generation));
                node.last_ready.set(Some(self.generation));
                true
            }
            Poll::Unchanged => {
                node.last_ready.set(Some(self.generation));
                true
            }
        }
    }

    // skip_self = true indicates output has *definitely* changed, but node has been recalculated
    // skip_self = false indicates node has not yet been recalculated
    fn mark_dirty<'a>(&'a self, node: NodeGuard<'a>, skip_self: bool) {
        if skip_self {
            let parents = node.drain_clean_parents();
            for parent in parents {
                // TODO still calling dirty twice on observed relationships
                parent.anchor.borrow_mut().as_mut().unwrap().dirty(&node.key());
                self.mark_dirty0(parent);
            }
        } else {
            self.mark_dirty0(node);
        }
    }

    fn mark_dirty0<'a>(&'a self, next: NodeGuard<'a>) {
        let id = next.key();
        if Self::check_observed_raw(next) != ObservedState::Unnecessary {
            self.graph.queue_recalc(next);
        } else if graph2::recalc_state(next) == RecalcState::Ready {
            graph2::needs_recalc(next);
            let parents = next.drain_clean_parents();
            for parent in parents {
                parent.anchor.borrow_mut().as_mut().unwrap().dirty(&id);
                self.mark_dirty0(parent);
            }
        }
    }
}

/// Singlethread's implementation of Anchors' `DirtyHandle`, which allows a node with non-Anchors inputs to manually mark itself as dirty.
#[derive(Debug, Clone)]
pub struct DirtyHandle {
    num: NodeKey,
    dirty_marks: Rc<RefCell<Vec<NodeKey>>>,
}
impl crate::DirtyHandle for DirtyHandle {
    fn mark_dirty(&self) {
        self.dirty_marks.borrow_mut().push(self.num);
    }
}

struct EngineContext<'eng> {
    engine: &'eng Engine,
}

struct EngineContextMut<'eng> {
    engine: &'eng Engine,
    node: NodeGuard<'eng>,
    pending_on_anchor_get: bool,
}

impl<'eng> OutputContext<'eng> for EngineContext<'eng> {
    type Engine = Engine;

    fn get<'out, O: 'static>(&self, anchor: &Anchor<O, Self::Engine>) -> &'out O
    where
        'eng: 'out,
    {
        let node = self.engine.graph.get(anchor.token()).unwrap();
        if graph2::recalc_state(node) != RecalcState::Ready {
            panic!("attempted to get node that was not previously requested")
        }
        let unsafe_borrow = unsafe { node.anchor.as_ptr().as_ref().unwrap() };
        let output: &O = unsafe_borrow
            .as_ref()
            .unwrap()
            .output(&mut EngineContext {
                engine: self.engine,
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
        let node = self.engine.graph.get(anchor.token()).unwrap();
        if graph2::recalc_state(node) != RecalcState::Ready {
            panic!("attempted to get node that was not previously requested")
        }

        let unsafe_borrow = unsafe { node.anchor.as_ptr().as_ref().unwrap() };
        let output: &O = unsafe_borrow
            .as_ref()
            .unwrap()
            .output(&mut EngineContext {
                engine: self.engine,
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
        let child = self.engine.graph.get(anchor.token()).unwrap();
        let height_already_increased = match graph2::ensure_height_increases(child, self.node) {
            Ok(v) => v,
            Err(()) => {
                panic!("loop detected in anchors!\n");
            }
        };

        let self_is_necessary =
            Engine::check_observed_raw(self.node)
                != ObservedState::Unnecessary;

        if graph2::recalc_state(child) != RecalcState::Ready {
            self.pending_on_anchor_get = true;
            self.engine.graph.queue_recalc(child);
            if necessary && self_is_necessary {
                self.node.add_necessary_child(child);
            }
            Poll::Pending
        } else if !height_already_increased {
            self.pending_on_anchor_get = true;
            Poll::Pending
        } else {
            child.add_clean_parent(self.node);
            if necessary && self_is_necessary {
                self.node.add_necessary_child(child);
            }
            match (child.last_update.get(), self.node.last_ready.get()) {
                (Some(a), Some(b)) if a <= b => Poll::Unchanged,
                _ => Poll::Updated,
            }
        }
    }

    fn unrequest<'out, O: 'static>(&mut self, anchor: &Anchor<O, Self::Engine>) {
        let child = self.engine.graph.get(anchor.token()).unwrap();
        self.node.remove_necessary_child(child);
        Engine::update_necessary_children(child);
    }

    fn dirty_handle(&mut self) -> DirtyHandle {
        DirtyHandle {
            num: self.node.key(),
            dirty_marks: self.engine.dirty_marks.clone(),
        }
    }
}

trait GenericAnchor {
    fn dirty(&mut self, child: &NodeKey);
    fn poll_updated<'eng>(&mut self, ctx: &mut EngineContextMut<'eng>) -> Poll;
    fn output<'slf, 'out>(&'slf self, ctx: &mut EngineContext<'out>) -> &'out dyn Any
    where
        'slf: 'out;
    fn debug_info(&self) -> AnchorDebugInfo;
}
impl<I: AnchorInner<Engine> + 'static> GenericAnchor for I {
    fn dirty(&mut self, child: &NodeKey) {
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

#[derive(Debug, Clone, Copy)]
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
