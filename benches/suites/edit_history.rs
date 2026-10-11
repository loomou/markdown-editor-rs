use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{self, Shape};
use md_bench::harness;
use md_core::doc::Doc;
use md_core::document::{Command, Sel};

const SEED: u64 = 0x5eed_0f17;
const SIZE: usize = 16 << 10;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(50)
}

fn typing() -> Command {
    Command::Insert {
        text: "ab cd".to_owned(),
    }
}

fn typed(text: &str, count: usize) -> Doc {
    let mut doc = harness::doc(text);
    let mut sel = harness::caret(harness::first_leaf(&doc), 0);
    for _ in 0..count {
        let caret = doc.apply(sel, typing());
        sel = Sel::collapsed(caret);
    }
    doc
}

fn assert_undo_depth(text: &str, count: usize) {
    let mut doc = typed(text, count);
    for step in 0..count {
        assert!(
            doc.undo().is_some(),
            "expected at least {count} undo steps, stopped at {step}"
        );
    }
    assert!(
        !doc.can_undo(),
        "expected exactly {count} undo steps, but more remain"
    );
}

fn undo_one(c: &mut Criterion) {
    let text = corpus::generate(Shape::KitchenSink, SIZE, SEED);
    harness::assert_fixture(&text, "kitchen-sink");
    let mut group = c.benchmark_group("edit_history/undo-one");
    group.throughput(Throughput::Elements(1));
    for depth in [1usize, 16, 128] {
        assert_undo_depth(&text, depth);
        group.bench_with_input(BenchmarkId::from_parameter(depth), &text, |b, text| {
            b.iter_batched_ref(
                || typed(text, depth),
                |doc| doc.undo(),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn undo_all(c: &mut Criterion) {
    let text = corpus::generate(Shape::KitchenSink, SIZE, SEED);
    harness::assert_fixture(&text, "kitchen-sink");
    let mut group = c.benchmark_group("edit_history/undo-all");
    group.throughput(Throughput::Elements(1));
    for depth in [16usize, 128] {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &text, |b, text| {
            b.iter_batched_ref(
                || typed(text, depth),
                |doc| {
                    while doc.can_undo() {
                        doc.undo();
                    }
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn redo_all(c: &mut Criterion) {
    let text = corpus::generate(Shape::KitchenSink, SIZE, SEED);
    harness::assert_fixture(&text, "kitchen-sink");
    let mut group = c.benchmark_group("edit_history/redo-all");
    group.throughput(Throughput::Elements(1));
    for depth in [16usize, 128] {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = typed(text, depth);
                    while doc.can_undo() {
                        doc.undo();
                    }
                    assert!(doc.can_redo(), "the redo stack must be full");
                    doc
                },
                |doc| {
                    while doc.can_redo() {
                        doc.redo();
                    }
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = undo_one, undo_all, redo_all
}
criterion_main!(benches);
