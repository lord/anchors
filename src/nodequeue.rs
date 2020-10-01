use crate::fakeheap::FakeHeap;
use slotmap::{Key, SecondaryMap};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum NodeState {
    NeedsRecalc,
    PendingRecalc,
    Ready,
}

impl Default for NodeState {
    fn default() -> Self {
        NodeState::NeedsRecalc
    }
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
}
