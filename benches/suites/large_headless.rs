use std::rc::Rc;
use std::time::Duration;

use criterion::{
    BatchSize, Bencher, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};
use md_bench::corpus::{BLOCK_COUNTS, BLOCK_SEED, generate_blocks, human_blocks};
use md_bench::harness;
use md_core::Px;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use md_layout::assembly::assemble_shared;
use md_layout::compose::compose;
use md_layout::flow::HeightState;
use md_layout::island::FallbackSolver;
use md_layout::spine::FlowSpine;
use md_render::search::scan_document;

const VIEWPORT_HEIGHT: Px = 600.0;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(10)
}

fn stage<O>(b: &mut Bencher<'_>, mut routine: impl FnMut() -> O) {
    b.iter_batched_ref(|| (), |()| routine(), BatchSize::PerIteration);
}

fn large_texts() -> Vec<(String, String)> {
    BLOCK_COUNTS
        .iter()
        .map(|&blocks| {
            (
                format!("blocks-{}", human_blocks(blocks)),
                generate_blocks(blocks, BLOCK_SEED),
            )
        })
        .collect()
}

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-stages/parse");
    group.sample_size(10);
    for (label, text) in large_texts() {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &text, |b, text| {
            stage(b, || load_markdown(text, editor_options()))
        });
    }
    group.finish();
}

fn serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-stages/serialize");
    group.sample_size(10);
    for (label, text) in large_texts() {
        let document = load_markdown(&text, editor_options());
        assert!(
            !document.to_markdown().is_empty(),
            "{label}: the parsed document serialized to nothing"
        );
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(&label),
            &document,
            |b, document| stage(b, || document.to_markdown()),
        );
    }
    group.finish();
}

fn compose_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-stages/compose");
    group.sample_size(10);
    let theme = harness::layout_theme();
    for (label, text) in large_texts() {
        let document = load_markdown(&text, editor_options());
        assert!(
            !compose(&document, &theme).nodes().is_empty(),
            "{label}: compose produced an empty box tree"
        );
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(&label),
            &document,
            |b, document| stage(b, || compose(document, &theme)),
        );
    }
    group.finish();
}

fn assemble(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-stages/assemble");
    group.sample_size(10);
    let theme = harness::layout_theme();
    let env = harness::environment();
    let measure = harness::StubMeasure;
    for (label, text) in large_texts() {
        let document = load_markdown(&text, editor_options());
        let tree = Rc::new(compose(&document, &theme));
        let assembly = assemble_shared(Rc::clone(&tree), env, &measure, &FallbackSolver);
        assert!(
            !assembly.heights.is_empty(),
            "{label}: assemble resolved no box heights"
        );
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &tree, |b, tree| {
            stage(b, || {
                assemble_shared(Rc::clone(tree), env, &measure, &FallbackSolver)
            })
        });
    }
    group.finish();
}

fn spine(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-stages/spine");
    group.sample_size(10);
    let theme = harness::layout_theme();
    let env = harness::environment();
    let measure = harness::StubMeasure;
    for (label, text) in large_texts() {
        let document = load_markdown(&text, editor_options());
        let tree = Rc::new(compose(&document, &theme));
        let assembly = assemble_shared(Rc::clone(&tree), env, &measure, &FallbackSolver);
        let heights = &assembly.heights;
        let heights_of = |id: md_layout::box_tree::LayoutBoxId, _avail: Px| {
            heights
                .get(&id)
                .copied()
                .unwrap_or(HeightState::Estimated(0.0))
        };
        let flat = FlowSpine::flatten(&tree, env.viewport_width, &heights_of);
        assert_ne!(
            flat.len(),
            0,
            "{label}: the flow spine flattened to no items"
        );
        let top = (flat.total_height() / 2.0).max(0.0);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::new("flatten", &label), &tree, |b, tree| {
            stage(b, || {
                FlowSpine::flatten(tree, env.viewport_width, &heights_of)
            })
        });
        group.bench_with_input(BenchmarkId::new("window", &label), &flat, |b, flat| {
            stage(b, || flat.window(&tree, top, top + VIEWPORT_HEIGHT, &[]))
        });
    }
    group.finish();
}

fn search(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-render/search");
    group.sample_size(10);
    for (label, text) in large_texts() {
        let doc = harness::doc(&text);
        let hit: String = text.chars().take(8).collect();
        for (kind, query) in [("hit", hit), ("miss", "ZZQXJVqq".to_owned())] {
            group.bench_with_input(BenchmarkId::new(kind, &label), &query, |b, query| {
                stage(b, || scan_document(&doc, query))
            });
        }
    }
    group.finish();
}

fn edit_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-edit/insert");
    group.sample_size(10);
    for (label, text) in large_texts() {
        group.bench_with_input(BenchmarkId::from_parameter(&label), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let doc = harness::doc(text);
                    let leaves = doc.text_leaves();
                    let leaf = leaves.get(leaves.len() / 2).copied().unwrap_or(0);
                    let sel = harness::caret(leaf, harness::leaf_end(&doc, leaf) / 2);
                    (doc, sel)
                },
                |(doc, sel)| {
                    doc.apply(
                        *sel,
                        Command::Insert {
                            text: "x".to_owned(),
                        },
                    );
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn edit_ime(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-edit/ime");
    group.sample_size(10);
    for (label, text) in large_texts() {
        group.bench_with_input(BenchmarkId::from_parameter(&label), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(text);
                    let anchor = Caret {
                        block: harness::first_leaf(&doc),
                        offset: 0,
                    };
                    let mut head = anchor;
                    for index in 1..=4 {
                        head = doc.apply_marked(
                            Sel { anchor, head },
                            Command::Insert {
                                text: "n".repeat(index),
                            },
                        );
                    }
                    assert!(doc.is_composing(), "the composition did not start");
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
                    );
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn edit_undo(c: &mut Criterion) {
    let mut group = c.benchmark_group("large-edit/undo");
    group.sample_size(10);
    for (label, text) in large_texts() {
        group.bench_with_input(BenchmarkId::from_parameter(&label), &text, |b, text| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(text);
                    let sel = harness::caret(harness::first_leaf(&doc), 0);
                    doc.apply(
                        sel,
                        Command::Insert {
                            text: "x".to_owned(),
                        },
                    );
                    assert!(doc.can_undo(), "the undo stack must be full");
                    doc
                },
                |doc| {
                    doc.undo();
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
    targets = parse, serialize, compose_tree, assemble, spine, search, edit_insert, edit_ime, edit_undo
}
criterion_main!(benches);
