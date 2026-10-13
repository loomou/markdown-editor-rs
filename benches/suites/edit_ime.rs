use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{self, Shape};
use md_bench::harness;
use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel};

const SEED: u64 = 0x5eed_0f17;
const SIZE: usize = 16 << 10;
const PINYIN: &str = "nihao";

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(50)
}

fn anchor_of(doc: &Doc) -> Caret {
    Caret {
        block: harness::first_leaf(doc),
        offset: 0,
    }
}

fn compose(doc: &mut Doc, letters: usize) -> (Caret, Caret) {
    let anchor = anchor_of(doc);
    let mut head = anchor;
    for index in 1..=letters {
        let sel = Sel { anchor, head };
        head = doc.apply_marked(
            sel,
            Command::Insert {
                text: "n".repeat(index),
            },
        );
    }
    assert!(doc.is_composing(), "the composition did not start");
    (anchor, head)
}

fn plain_zh() -> String {
    let text = corpus::generate(Shape::PlainZh, SIZE, SEED);
    harness::assert_fixture(&text, "plain-zh");
    text
}

fn marked_step(c: &mut Criterion) {
    let text = plain_zh();
    let mut group = c.benchmark_group("edit_ime/marked-step");
    group.throughput(Throughput::Elements(1));
    for letters in [1usize, 4, 8] {
        group.bench_with_input(BenchmarkId::from_parameter(letters), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(text);
                    let (anchor, head) = compose(&mut doc, letters);
                    (doc, anchor, head)
                },
                |(doc, anchor, head)| {
                    doc.apply_marked(
                        Sel {
                            anchor: *anchor,
                            head: *head,
                        },
                        Command::Insert {
                            text: "n".repeat(letters + 1),
                        },
                    )
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn commit(c: &mut Criterion) {
    let text = plain_zh();
    let mut group = c.benchmark_group("edit_ime/commit");
    group.throughput(Throughput::Elements(1));
    for letters in [1usize, 4, 8] {
        group.bench_with_input(BenchmarkId::from_parameter(letters), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(text);
                    let (anchor, head) = compose(&mut doc, letters);
                    (doc, anchor, head)
                },
                |(doc, anchor, head)| {
                    doc.apply_ime_commit(
                        Sel {
                            anchor: *anchor,
                            head: *head,
                        },
                        Command::Insert {
                            text: "好".to_owned(),
                        },
                    )
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn abort(c: &mut Criterion) {
    let text = plain_zh();
    let mut group = c.benchmark_group("edit_ime/abort");
    group.throughput(Throughput::Elements(1));
    for letters in [1usize, 4, 8] {
        group.bench_with_input(BenchmarkId::from_parameter(letters), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(text);
                    let _ = compose(&mut doc, letters);
                    doc
                },
                |doc| doc.abort_compose(),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn sequence(c: &mut Criterion) {
    let text = plain_zh();
    let mut group = c.benchmark_group("edit_ime/sequence");
    group.throughput(Throughput::Elements(1));
    group.bench_function("nihao-commit", |b| {
        b.iter_batched_ref(
            || {
                let doc = harness::doc(&text);
                let anchor = anchor_of(&doc);
                (doc, anchor)
            },
            |(doc, anchor)| {
                let mut head = *anchor;
                for index in 1..=PINYIN.len() {
                    head = doc.apply_marked(
                        Sel {
                            anchor: *anchor,
                            head,
                        },
                        Command::Insert {
                            text: PINYIN[..index].to_owned(),
                        },
                    );
                }
                doc.apply_ime_commit(
                    Sel {
                        anchor: *anchor,
                        head,
                    },
                    Command::Insert {
                        text: "你好".to_owned(),
                    },
                )
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = marked_step, commit, abort, sequence
}
criterion_main!(benches);
