use anchors::{singlethread::Engine, AnchorExt, Var};

fn main() {
    let mut engine = Engine::new_with_max_height(1003);
    let (first_num, set_first_num) = Var::new(0u64);
    let mut node = first_num.cutoff(|_old_val| false);
    for _ in 0..10 {
        node = node.map(|val| {
            println!("recalc map");
            val + 1
        });
    }
    engine.mark_observed(&node);
    assert_eq!(engine.get(&node), 10);
    let mut update_number = 0;

    for _ in 0..5 {
        update_number += 1;
        println!("== setting ==");
        println!("{}", engine.debug_state());
        set_first_num.set(update_number);
        engine.update_dirty_marks();
        println!("== new stabilize ==");
        println!("{}", engine.debug_state());
        assert_eq!(engine.get(&node), 10);
    }
}
