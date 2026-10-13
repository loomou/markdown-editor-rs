use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{self, Shape};
use md_bench::harness::{self, Spot};
use md_core::document::{Command, PasteIntent};

const SEED: u64 = 0x5eed_0f17;
const SIZE: usize = 16 << 10;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(50)
}

fn shapes() -> [Shape; 4] {
    [
        Shape::PlainEn,
        Shape::NestedLists,
        Shape::Quotes,
        Shape::Tables,
    ]
}

fn commands() -> Vec<(&'static str, Command)> {
    vec![
        (
            "insert",
            Command::Insert {
                text: "x".to_owned(),
            },
        ),
        ("delete-backward", Command::DeleteBackward),
        ("delete-forward", Command::DeleteForward),
        ("break", Command::Break),
        ("soft-break", Command::SoftBreak),
        ("indent", Command::Indent),
        ("outdent", Command::Outdent),
        ("toggle-task", Command::ToggleTask),
        (
            "wrap-list",
            Command::WrapList {
                ordered: false,
                task: None,
            },
        ),
        (
            "wrap-ordered",
            Command::WrapList {
                ordered: true,
                task: None,
            },
        ),
        (
            "paste-plain",
            Command::Paste {
                text: "pasted text".to_owned(),
                intent: PasteIntent::PlainText,
            },
        ),
        (
            "paste-fragment",
            Command::Paste {
                text: "- one\n- two\n".to_owned(),
                intent: PasteIntent::IndependentFragment,
            },
        ),
    ]
}

fn spot_by_shape(c: &mut Criterion) {
    let mut group = c.benchmark_group("edit_commands/spot");
    group.throughput(Throughput::Elements(1));
    for shape in shapes() {
        let text = corpus::generate(shape, SIZE, SEED);
        harness::assert_fixture(&text, shape.name());
        for spot in Spot::ALL {
            harness::assert_editable(&text, spot);
            group.bench_with_input(
                BenchmarkId::new(shape.name(), spot.name()),
                &text,
                |b, text| {
                    b.iter_batched_ref(
                        || {
                            let doc = harness::doc(text);
                            let sel = spot.sel(&doc);
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

fn command_sweep(c: &mut Criterion) {
    let text = corpus::generate(Shape::KitchenSink, SIZE, SEED);
    harness::assert_fixture(&text, "kitchen-sink");
    let mut group = c.benchmark_group("edit_commands/command");
    group.throughput(Throughput::Elements(1));
    for spot in [Spot::DocStart, Spot::DocMiddle, Spot::DocEnd] {
        for (name, cmd) in commands() {
            group.bench_with_input(BenchmarkId::new(name, spot.name()), &text, |b, text| {
                b.iter_batched_ref(
                    || {
                        let doc = harness::doc(text);
                        let sel = spot.sel(&doc);
                        (doc, sel)
                    },
                    |(doc, sel)| doc.apply(*sel, cmd.clone()),
                    BatchSize::PerIteration,
                );
            });
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = spot_by_shape, command_sweep
}
criterion_main!(benches);
