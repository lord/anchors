use anchors::{singlethread::Engine, AnchorExt, Var};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn stabilize_linear_nodes(c: &mut Criterion) {
    for node_count in &[10, 100, 1000] {
        for observed in &[true, false] {
            c.bench_with_input(
                BenchmarkId::new(
                    "stabilize_linear_nodes",
                    format!(
                        "{}/{}",
                        node_count,
                        if *observed { "observed" } else { "unobserved" }
                    ),
                ),
                &(*node_count, *observed),
                |b, (node_count, observed)| {
                    let mut engine = Engine::new_with_max_height(1003);
                    let (first_num, set_first_num) = Var::new(0u64);
                    let mut node = first_num;
                    for _ in 0..*node_count {
                        node = node.map(|val| val + black_box(1));
                    }
                    if *observed {
                        engine.mark_observed(&node);
                    }
                    assert_eq!(engine.get(&node), *node_count);
                    let mut update_number = 0;
                    b.iter(|| {
                        update_number += 1;
                        set_first_num.set(update_number);
                        assert_eq!(engine.get(&node), update_number + *node_count);
                    });
                },
            );
        }
    }
}

fn stabilize_linear_nodes_smallheight(c: &mut Criterion) {
    for node_count in &[10, 100] {
        for observed in &[true, false] {
            c.bench_with_input(
                BenchmarkId::new(
                    "stabilize_linear_nodes_smallheight",
                    format!(
                        "{}/{}",
                        node_count,
                        if *observed { "observed" } else { "unobserved" }
                    ),
                ),
                &(*node_count, *observed),
                |b, (node_count, observed)| {
                    let mut engine = Engine::new_with_max_height(128);
                    let (first_num, set_first_num) = Var::new(0u64);
                    let mut node = first_num;
                    for _ in 0..*node_count {
                        node = node.map(|val| val + black_box(1));
                    }
                    if *observed {
                        engine.mark_observed(&node);
                    }
                    assert_eq!(engine.get(&node), *node_count);
                    let mut update_number = 0;
                    b.iter(|| {
                        update_number += 1;
                        set_first_num.set(update_number);
                        assert_eq!(engine.get(&node), update_number + *node_count);
                    });
                },
            );
        }
    }
}

fn stabilize_linear_nodes_cutoff(c: &mut Criterion) {
    for node_count in &[10, 100, 1000] {
        for observed in &[true, false] {
            c.bench_with_input(
                BenchmarkId::new(
                    "stabilize_linear_nodes_cutoff",
                    format!(
                        "{}/{}",
                        node_count,
                        if *observed { "observed" } else { "unobserved" }
                    ),
                ),
                &(*node_count, *observed),
                |b, (node_count, observed)| {
                    let mut engine = Engine::new_with_max_height(1003);
                    let (first_num, set_first_num) = Var::new(0u64);
                    let node = first_num;
                    let node = node.map(|val| black_box(val) - black_box(val) + 1);
                    let mut node = {
                        let mut old_val = None;
                        node.cutoff(move |val| {
                            if Some(*val) != old_val {
                                old_val = Some(*val);
                                true
                            } else {
                                false
                            }
                        })
                    };
                    for i in 0..*node_count {
                        node = node.map(move |val| black_box(val) - black_box(val) + black_box(i));
                    }
                    if *observed {
                        engine.mark_observed(&node);
                    }
                    assert_eq!(engine.get(&node), *node_count - 1);
                    let mut update_number = 0;
                    b.iter(|| {
                        update_number += 1;
                        set_first_num.set(update_number);
                        assert_eq!(engine.get(&node), *node_count - 1);
                    });
                },
            );
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = stabilize_linear_nodes_cutoff, stabilize_linear_nodes_smallheight, stabilize_linear_nodes
}
criterion_main!(benches);
