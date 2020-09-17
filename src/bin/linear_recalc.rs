use anchors::{singlethread::Engine, AnchorExt, Var};

fn main() {
    let mut engine = Engine::new_with_max_height(1003);
    let (first_num, set_first_num) = Var::new(0u64);
    let mut node = first_num;
    for _ in 0..1000 {
        node = node.map(|val| val + 1);
    }
    assert_eq!(engine.get(&node), 1000);
    let mut update_number = 0;

    for _ in  0..100 {
        update_number += 1;
        set_first_num.set(update_number);
        assert_eq!(engine.get(&node), update_number+1000);
    }
}
