use anchors::expert::{Anchor, MultiAnchor, Var};
use anchors::singlethread::Engine;

const NODE_COUNT: u64 = 100;
const ITER_COUNT: u64 = 500000;
const OBSERVED: bool = true;

fn main() {
    let mut engine = Engine::new_with_max_height(128);
    let first_num = Var::new(0u64);
    let mut node = first_num.watch();
    for _ in 0..NODE_COUNT {
        node = node.map(|val| val + 1);
    }
    if OBSERVED {
        engine.mark_observed(&node);
    }
    assert_eq!(engine.get(&node), NODE_COUNT);
    iter(node, engine, first_num);
}

#[inline(never)]
fn iter(node: Anchor<u64, Engine>, mut engine: Engine, set_first_num: Var<u64, Engine>) {
    let mut update_number = 0;
    for i in 0..ITER_COUNT {
        if i % (ITER_COUNT / 100) == 0 {
            println!("{}%", (i * 100) / (ITER_COUNT));
        }
        update_number += 1;
        set_first_num.set(update_number);
        assert_eq!(engine.get(&node), update_number + NODE_COUNT);
    }
}
