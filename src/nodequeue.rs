use crate::fakeheap::FakeHeap;
use slotmap::{Key, SecondaryMap};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum NodeState {
    NeedsRecalc,
    PendingRecalc,
    Ready,
}

pub struct NodeQueue<T: Key> {
    heap: FakeHeap<T>,
    states: SecondaryMap<T, NodeState>,
}

impl<T: Key> NodeQueue<T> {
    pub fn new(max_height: usize) -> Self {
        Self {
            heap: FakeHeap::new(max_height),
            states: SecondaryMap::new(),
        }
    }

    pub fn queue_recalc(&mut self, height: usize, node: T) {
        let old = self.states.insert(node.clone(), NodeState::PendingRecalc);
        if old == Some(NodeState::PendingRecalc) {
            return;
        }
        self.heap.insert(height, node);
    }

    pub fn needs_recalc(&mut self, node: T) {
        if self.states.get(node.clone()) != Some(&NodeState::Ready) {
            panic!("node queued for recalc, someone tried to mark it as NeedsRecalc");
        }
        self.states.insert(node, NodeState::NeedsRecalc);
    }

    pub fn pop_next(&mut self) -> Option<(usize, T)> {
        let next = self.heap.pop_min();
        if let Some((_, next)) = next.clone() {
            self.states.insert(next, NodeState::Ready);
        }
        next
    }

    pub fn state(&self, node: T) -> NodeState {
        self.states
            .get(node)
            .cloned()
            .unwrap_or(NodeState::NeedsRecalc)
    }
}
