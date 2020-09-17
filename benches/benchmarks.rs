use anchors::{singlethread::Engine, AnchorExt, Var};
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn stabilize_linear_nodes(c: &mut Criterion) {
    for input in &[10, 100, 1000] {
        c.bench_with_input(BenchmarkId::new("stabilize_linear_nodes", *input), input, |b, i| {
            let mut engine = Engine::new_with_max_height(1003);
            let (first_num, set_first_num) = Var::new(0u64);
            let mut node = first_num;
            for _ in 0..*i {
                node = node.map(|val| val + black_box(1));
            }
            assert_eq!(engine.get(&node), *i);
            let mut update_number = 0;
            b.iter(|| {
                update_number += 1;
                set_first_num.set(update_number);
                assert_eq!(engine.get(&node), update_number+*i);
            });
        });
    }
}

fn stabilize_observed_linear_nodes(c: &mut Criterion) {
    for input in &[10, 100, 1000] {
        c.bench_with_input(BenchmarkId::new("stabilize_linear_nodes", *input), input, |b, i| {
            let mut engine = Engine::new_with_max_height(1003);
            let (first_num, set_first_num) = Var::new(0u64);
            let mut node = first_num;
            for _ in 0..*i {
                node = node.map(|val| val + black_box(1));
            }
            engine.mark_observed(&node);
            assert_eq!(engine.get(&node), *i);
            let mut update_number = 0;
            b.iter(|| {
                update_number += 1;
                set_first_num.set(update_number);
                assert_eq!(engine.get(&node), update_number+*i);
            });
        });
    }
}

criterion_group!{
    name = benches;
    config = Criterion::default();
    targets = stabilize_linear_nodes
}
criterion_main!(benches);
