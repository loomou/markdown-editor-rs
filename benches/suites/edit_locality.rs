use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{self, Shape};
use md_bench::harness::{self, Spot};
use md_core::document::Command;

const SEED: u64 = 0x5eed_0f17;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(10)
}

fn report_counts() {
    println!();
    println!("deterministic cost of one single-character insert at the middle of the document");
    println!(
        "{:<14} {:>6} {:>9} {:>9} {:>7}",
        "shape", "size", "leaves", "changes", "edits"
    );
    for shape in [Shape::PlainEn, Shape::NestedLists, Shape::KitchenSink] {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            let mut doc = harness::doc(&text);
            let leaves = harness::leaf_count(&doc);
            let sel = Spot::DocMiddle.sel(&doc);
            doc.apply(
                sel,
                Command::Insert {
                    text: "x".to_owned(),
                },
            );
            let changes = harness::change_count(&mut doc);
            let edits = doc.edit_gen();
            println!(
                "{:<14} {:>6} {:>9} {:>9} {:>7}",
                shape.name(),
                corpus::human(size),
                leaves,
                changes,
                edits
            );
            assert!(
                changes >= 1,
                "{}: the edit recorded no change at all",
                shape.name()
            );
            assert!(
                edits >= 1,
                "{}: the edit did not advance the generation",
                shape.name()
            );
            assert!(
                changes <= 8,
                "{} at {}: one inserted character recorded {changes} changes, which is no longer a local edit",
                shape.name(),
                corpus::human(size)
            );
            if leaves >= 64 {
                assert!(
                    changes * 8 < leaves,
                    "{} at {}: one inserted character touched {changes} of {leaves} blocks, so incrementality collapsed",
                    shape.name(),
                    corpus::human(size)
                );
            }
        }
    }
    println!();
}

fn locality(c: &mut Criterion) {
    report_counts();
    let mut group = c.benchmark_group("edit_locality/insert-doc-middle");
    group.sample_size(10);
    for shape in [Shape::PlainEn, Shape::KitchenSink] {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            harness::assert_fixture(&text, shape.name());
            harness::assert_editable(&text, Spot::DocMiddle);
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.name(), corpus::human(size)),
                &text,
                |b, text| {
                    b.iter_batched_ref(
                        || {
                            let doc = harness::doc(text);
                            let sel = Spot::DocMiddle.sel(&doc);
                            (doc, sel)
                        },
                        |(doc, sel)| {
                            doc.apply(
                                *sel,
                                Command::Insert {
                                    text: "x".to_owned(),
                                },
                            )
                        },
                        BatchSize::PerIteration,
                    );
                },
            );
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = locality
}
criterion_main!(benches);
