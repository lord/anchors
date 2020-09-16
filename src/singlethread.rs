use crate::fakeheap::FakeHeap;
use crate::refcounter::RefCounter;
use crate::{graph, Anchor, AnchorInner, OutputContext, UpdateContext};
use slotmap::SlotMap;
use std::any::Any;
use std::cell::RefCell;
use std::panic::Location;
use std::rc::Rc;
use std::task::Poll;

thread_local! {
    static DEFAULT_MOUNTER: RefCell<Option<Mounter>> = RefCell::new(None);
}

slotmap::new_key_type! { struct NodeNum; }

pub struct Engine {
    // TODO store Nodes on heap directly?? maybe try for Rc<RefCell<SlotMap>> now
    nodes: Rc<RefCell<SlotMap<NodeNum, Node>>>,
    graph: graph::MetadataGraph<NodeNum>,
    to_recalculate: FakeHeap<NodeNum>,
    dirty_marks: Rc<RefCell<Vec<NodeNum>>>,
    refcounter: RefCounter<NodeNum>,
}

struct Mounter {
    nodes: Rc<RefCell<SlotMap<NodeNum, Node>>>,
    refcounter: RefCounter<NodeNum>,
}

impl crate::Engine for Engine {
    type AnchorData = AnchorData;
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
            });
            this.refcounter.create(num);
            Anchor::new(AnchorData {
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
}

impl Engine {
    pub fn new() -> Self {
        Self::new_with_max_height(256)
    }

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
            to_recalculate: FakeHeap::new(max_height),
            dirty_marks: Default::default(),
            refcounter,
        }
    }

    pub fn mark_observed<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        self.nodes
            .borrow_mut()
            .get_mut(anchor.data.num)
            .unwrap()
            .observed = true;
        if self.graph.is_dirty(anchor.data.num) {
            self.mark_node_for_recalculation(anchor.data.num);
        }
    }

    pub fn mark_unobserved<O: 'static>(&mut self, anchor: &Anchor<O, Engine>) {
        self.nodes
            .borrow_mut()
            .get_mut(anchor.data.num)
            .unwrap()
            .observed = false;
        // TODO remove from calculation queue if necessary?
        // TODO need to unobserve child nodes here
    }

    pub fn get<'out, O: Clone + 'static>(&mut self, anchor: &Anchor<O, Engine>) -> O {
        // stabilize once before, since the stabilization process may mark our requested node
        // as dirty
        self.stabilize();
        if self.graph.is_dirty(anchor.data.num) {
            self.to_recalculate
                .insert(self.graph.height(anchor.data.num), anchor.data.num);
            // stabilize again, to make sure our target node that is now in the queue is up-to-date
            self.stabilize();
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

    pub fn stabilize<'a>(&'a mut self) {
        let dirty_marks = std::mem::replace(&mut *self.dirty_marks.borrow_mut(), Vec::new());
        for dirty in dirty_marks {
            self.graph.mark_dirty(dirty);
            self.mark_node_dirty(dirty);
        }

        while let Some((height, this_node_num)) = self.to_recalculate.pop_min() {
            let calculation_complete = if height == self.graph.height(this_node_num) {
                // this nodes height is current, so we can recalculate
                self.recalculate(this_node_num)
            } else {
                // skip calculation, redo at correct height
                false
            };

            if calculation_complete {
                self.graph.mark_clean(this_node_num);
            } else {
                self.to_recalculate
                    .insert(self.graph.height(this_node_num), this_node_num);
            }
        }

        self.garbage_collect();
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
            Poll::Ready(output_changed) => {
                if output_changed {
                    // make sure all parents are marked as dirty, and observed parents are recalculated
                    self.mark_parents_dirty(this_node_num);
                }
                true
            }
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

    fn mark_node_dirty(&mut self, node_id: NodeNum) {
        if self.graph.is_necessary(node_id) || self.nodes.borrow()[node_id].observed {
            self.mark_node_for_recalculation(node_id);
        } else {
            self.mark_parents_dirty(node_id);
        };
    }

    fn mark_node_for_recalculation(&mut self, node_id: NodeNum) {
        if !self.to_recalculate.contains(node_id) {
            self.to_recalculate
                .insert(self.graph.height(node_id), node_id);
        }
    }

    fn mark_parents_dirty(&mut self, node_id: NodeNum) {
        for parent in self.graph.parents(node_id) {
            let edge = self.graph.edge(node_id, parent);
            if edge == graph::EdgeState::Clean {
                // Observed edges remain marked as observed
                let res = self
                    .graph
                    .set_edge(node_id, parent, graph::EdgeState::Dirty);
                self.panic_if_loop(res);
            }
            let anchor = self.nodes.borrow().get(parent).unwrap().anchor.clone();
            let anchor_data = AnchorData {
                num: node_id,
                refcounter: self.refcounter.clone(),
            };
            anchor.borrow_mut().dirty(&anchor_data);
            // mem::forget here so we skip calling AnchorData's Drop; don't want to decrement reference count
            std::mem::forget(anchor_data);
            self.mark_node_dirty(parent);
        }
    }
}

#[derive(Debug)]
pub struct AnchorData {
    num: NodeNum,
    refcounter: RefCounter<NodeNum>,
}

impl PartialEq for AnchorData {
    fn eq(&self, other: &Self) -> bool {
        self.num == other.num
    }
}

impl std::hash::Hash for AnchorData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.num.hash(state);
    }
}

impl Eq for AnchorData {}

impl Clone for AnchorData {
    fn clone(&self) -> Self {
        self.refcounter.increment(self.num);
        AnchorData {
            num: self.num,
            refcounter: self.refcounter.clone(),
        }
    }
}

impl Drop for AnchorData {
    fn drop(&mut self) {
        self.refcounter.decrement(self.num);
    }
}
impl crate::AnchorData for AnchorData {}

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
        if self.engine.to_recalculate.contains(anchor.data.num) || self.engine.graph.is_dirty(anchor.data.num) || self.engine.graph.edge(anchor.data.num, self.node_num) == graph::EdgeState::Dirty {
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
        if self.engine.to_recalculate.contains(anchor.data.num) || self.engine.graph.is_dirty(anchor.data.num) || self.engine.graph.edge(anchor.data.num, self.node_num) == graph::EdgeState::Dirty {
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
    ) -> Poll<bool> {
        let my_height = self.engine.graph.height(self.node_num);
        let child_height = self.engine.graph.height(anchor.data.num);
        let self_is_necessary = self.engine.graph.is_necessary(self.node_num) || self.engine.nodes.borrow().get(self.node_num).as_ref().unwrap().observed;
        let child_is_clean = !self.engine.graph.is_dirty(anchor.data.num);
        // setting edge updates heights; important to know previous heights before calling this
        let res = self.engine.graph.set_edge(
            anchor.data.num,
            self.node_num,
            if necessary && self_is_necessary {
                graph::EdgeState::Necessary
            } else {
                graph::EdgeState::Clean
            },
        );
        self.engine.panic_if_loop(res);
        if !child_is_clean {
            self.pending_on_anchor_get = true;
            self.engine.mark_node_for_recalculation(anchor.data.num);
            Poll::Pending
        } else if my_height <= child_height {
            self.pending_on_anchor_get = true;
            Poll::Pending
        } else {
            Poll::Ready(true) // TODO FIX
        }
    }

    fn dirty_handle(&mut self) -> DirtyHandle {
        DirtyHandle {
            num: self.node_num,
            dirty_marks: self.engine.dirty_marks.clone(),
        }
    }
}

trait GenericAnchor {
    fn dirty(&mut self, child: &AnchorData);
    fn poll_updated<'eng>(&mut self, ctx: &mut EngineContextMut<'eng>) -> Poll<bool>;
    fn output<'slf, 'out>(&'slf self, ctx: &mut EngineContext<'out>) -> &'out dyn Any
    where
        'slf: 'out;
    fn debug_info(&self) -> AnchorDebugInfo;
}
impl<I: AnchorInner<Engine> + 'static> GenericAnchor for I {
    fn dirty(&mut self, child: &AnchorData) {
        AnchorInner::dirty(self, child)
    }
    fn poll_updated<'eng>(&mut self, ctx: &mut EngineContextMut<'eng>) -> Poll<bool> {
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
