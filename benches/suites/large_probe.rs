use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use md_bench::corpus::{BLOCK_COUNTS, BLOCK_SEED, generate_blocks, human_blocks};
use md_bench::gpui_support::mid_leaf;
use md_bench::harness;
use md_core::Px;
use md_core::document::Command;
use md_layout::box_tree::LayoutBoxId;
use md_layout::flow::HeightState;
use md_layout::spine::FlowSpine;
use md_render::incremental::{Estimator, IncrementalEngine};
use std::time::Duration;

fn estimate(_id: LayoutBoxId, _avail: Px) -> HeightState {
    HeightState::Estimated(20.0)
}

fn sizes() -> Vec<(String, String)> {
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

fn expand_visible(c: &mut Criterion) {
    let mut group = c.benchmark_group("probe/spine-expand-visible");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(1));
    let theme = harness::layout_theme();
    for (label, text) in sizes() {
        let doc = harness::doc(&text);
        let tree = md_layout::compose::compose(&doc.document, &theme);
        {
            let mut probe = FlowSpine::flatten(&tree, 800.0, &estimate);
            let len = probe.len();
            let rounds = probe.expand_visible(&tree, 0.0, 600.0, &estimate);
            eprintln!("[probe spine {label}] spine_len={len} first_rounds={rounds}");
        }
        group.bench_function(&label, |b| {
            b.iter_batched_ref(
                || FlowSpine::flatten(&tree, 800.0, &estimate),
                |spine| {
                    let _ = spine.expand_visible(&tree, 0.0, 600.0, &estimate);
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn apply_changes(c: &mut Criterion) {
    let mut group = c.benchmark_group("probe/apply-changes");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(1));
    let theme = harness::document_theme();
    let env = harness::environment();
    for (label, text) in sizes() {
        let base = harness::doc(&text);
        let spine_len = {
            let tree = md_layout::compose::compose(&base.document, &theme.layout_theme());
            FlowSpine::flatten(&tree, env.viewport_width, &estimate).len()
        };
        eprintln!("[probe apply-changes {label}] spine_len={spine_len}");
        group.bench_function(&label, |b| {
            b.iter_batched_ref(
                || {
                    let mut doc = harness::doc(&text);
                    let leaves = doc.text_leaves();
                    let leaf = leaves.get(leaves.len() / 2).copied().unwrap_or(0);
                    let sel = harness::caret(leaf, harness::leaf_end(&doc, leaf) / 2);
                    doc.apply(
                        sel,
                        Command::Insert {
                            text: "x".to_owned(),
                        },
                    );
                    let changes = doc.take_changes();
                    let engine = IncrementalEngine::new(
                        &base.document,
                        env,
                        Estimator::from_theme(&theme),
                        theme.layout_theme(),
                    );
                    (doc, engine, changes)
                },
                |(doc, engine, changes)| {
                    let _ = engine.apply_changes(&doc.document, changes);
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn leaf_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("probe/mid-leaf");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(1));
    for (label, text) in sizes() {
        let doc = harness::doc(&text);
        group.bench_function(&label, |b| {
            b.iter(|| std::hint::black_box(mid_leaf(&doc)));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = expand_visible, apply_changes, leaf_lookup
}
criterion_main!(benches);
