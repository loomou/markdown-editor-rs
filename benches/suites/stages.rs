use std::rc::Rc;
use std::time::Duration;

use criterion::{
    BatchSize, Bencher, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};
use md_bench::corpus::{self, Shape};
use md_bench::harness;
use md_core::Px;
use md_core::document::{editor_options, load_markdown};
use md_layout::assembly::assemble_shared;
use md_layout::compose::compose;
use md_layout::flow::HeightState;
use md_layout::island::FallbackSolver;
use md_layout::linebreak::{
    Clusters, ItemCtx, Mode, Opp, OppCtx, Params, Placed, break_lines, build_items, opportunities,
};
use md_layout::spine::FlowSpine;

const SEED: u64 = 0x5eed_0f17;
const EM: f32 = 10.0;
const BREAK_WIDTH: f32 = 800.0;
const VIEWPORT_HEIGHT: Px = 600.0;

const STAGE_SHAPES: [Shape; 6] = [
    Shape::PlainEn,
    Shape::PlainZh,
    Shape::NestedLists,
    Shape::Tables,
    Shape::CodeFences,
    Shape::KitchenSink,
];

const BREAK_SHAPES: [Shape; 3] = [
    Shape::LongParagraph,
    Shape::LongParagraphZh,
    Shape::LongParagraphMixed,
];

const BREAK_SIZES: [usize; 3] = [4 << 10, 32 << 10, 256 << 10];

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(10)
}

fn stage<O>(b: &mut Bencher<'_>, mut routine: impl FnMut() -> O) {
    b.iter_batched_ref(|| (), |()| routine(), BatchSize::PerIteration);
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Ragged => "ragged",
        Mode::Justify => "justify",
        Mode::Balanced => "balanced",
    }
}

fn mono_clusters(text: &str) -> Clusters {
    let mut bytes = Vec::new();
    let mut xs = Vec::new();
    let mut x = 0.0f32;
    for (index, character) in text.char_indices() {
        bytes.push(index as u32);
        xs.push(x);
        x += if character.len_utf8() > 3 { 12.0 } else { 6.0 };
    }
    bytes.push(text.len() as u32);
    xs.push(x);
    Clusters::from_pairs(bytes.into_iter().zip(xs)).expect("monotone byte offsets")
}

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages/parse");
    group.sample_size(10);
    for shape in STAGE_SHAPES {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            harness::assert_fixture(&text, shape.name());
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.name(), corpus::human(size)),
                &text,
                |b, text| stage(b, || load_markdown(text, editor_options())),
            );
        }
    }
    group.finish();
}

fn serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages/serialize");
    group.sample_size(10);
    for shape in STAGE_SHAPES {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            let document = load_markdown(&text, editor_options());
            assert!(
                !document.to_markdown().is_empty(),
                "{}: the parsed document serialized to nothing",
                shape.name()
            );
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.name(), corpus::human(size)),
                &document,
                |b, document| stage(b, || document.to_markdown()),
            );
        }
    }
    group.finish();
}

fn compose_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages/compose");
    group.sample_size(10);
    let theme = harness::layout_theme();
    for shape in STAGE_SHAPES {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            let document = load_markdown(&text, editor_options());
            let tree = compose(&document, &theme);
            assert!(
                !tree.nodes().is_empty(),
                "{}: compose produced an empty box tree",
                shape.name()
            );
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.name(), corpus::human(size)),
                &document,
                |b, document| stage(b, || compose(document, &theme)),
            );
        }
    }
    group.finish();
}

fn assemble(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages/assemble");
    group.sample_size(10);
    let theme = harness::layout_theme();
    let env = harness::environment();
    let measure = harness::StubMeasure;
    for shape in STAGE_SHAPES {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
            let document = load_markdown(&text, editor_options());
            let tree = Rc::new(compose(&document, &theme));
            let assembly = assemble_shared(Rc::clone(&tree), env, &measure, &FallbackSolver);
            assert!(
                !assembly.heights.is_empty(),
                "{}: assemble resolved no box heights",
                shape.name()
            );
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(shape.name(), corpus::human(size)),
                &tree,
                |b, tree| {
                    stage(b, || {
                        assemble_shared(Rc::clone(tree), env, &measure, &FallbackSolver)
                    })
                },
            );
        }
    }
    group.finish();
}

fn spine(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages/spine");
    group.sample_size(10);
    let theme = harness::layout_theme();
    let env = harness::environment();
    let measure = harness::StubMeasure;
    for shape in STAGE_SHAPES {
        for size in corpus::SIZES {
            let text = corpus::generate(shape, size, SEED);
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
                "{}: the flow spine flattened to no items",
                shape.name()
            );
            let top = (flat.total_height() / 2.0).max(0.0);
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("flatten/{}", shape.name()), corpus::human(size)),
                &tree,
                |b, tree| {
                    stage(b, || {
                        FlowSpine::flatten(tree, env.viewport_width, &heights_of)
                    })
                },
            );
            group.bench_with_input(
                BenchmarkId::new(format!("window/{}", shape.name()), corpus::human(size)),
                &flat,
                |b, flat| stage(b, || flat.window(&tree, top, top + VIEWPORT_HEIGHT, &[])),
            );
        }
    }
    group.finish();
}

fn break_items(
    text: &str,
    clusters: &Clusters,
    opps: &[(u32, Opp)],
    mode: Mode,
    params: &Params,
) -> Vec<Placed> {
    build_items(
        text,
        clusters,
        opps,
        &ItemCtx {
            mode,
            em: EM,
            width: BREAK_WIDTH,
            params,
            code_ranges: &[],
            tab_ranges: &[],
        },
    )
}

fn report_paths(params: &Params) {
    println!();
    println!("line-break solver: which path runs for each paragraph length");
    println!(
        "Knuth-Plass is only attempted while the item count stays at or below Params::max_items"
    );
    println!(
        "{:<22} {:>8} {:>9} {:>10} {:>8}",
        "paragraph", "bytes", "items", "path", "lines"
    );
    for shape in BREAK_SHAPES {
        for size in BREAK_SIZES {
            let text = corpus::generate(shape, size, SEED);
            let clusters = mono_clusters(&text);
            let opps = opportunities(&text, &OppCtx { glue_before: &[] });
            let items = break_items(&text, &clusters, &opps, Mode::Ragged, params);
            let plan = break_lines(&text, &items, BREAK_WIDTH, EM, Mode::Ragged, params);
            println!(
                "{:<22} {:>8} {:>9} {:>10} {:>8}",
                shape.name(),
                corpus::human(text.len()),
                items.len(),
                if plan.fallback {
                    "greedy"
                } else {
                    "knuth-plass"
                },
                plan.lines.len()
            );
        }
    }
    println!();
}

struct BreakCase {
    label: String,
    size: String,
    text: String,
    items: Vec<Placed>,
    mode: Mode,
    fallback: bool,
}

fn linebreak(c: &mut Criterion) {
    let params = Params::default();
    report_paths(&params);
    let mut cases = Vec::new();
    for shape in BREAK_SHAPES {
        for size in BREAK_SIZES {
            for mode in [Mode::Ragged, Mode::Justify, Mode::Balanced] {
                let text = corpus::generate(shape, size, SEED);
                let clusters = mono_clusters(&text);
                let opps = opportunities(&text, &OppCtx { glue_before: &[] });
                let items = break_items(&text, &clusters, &opps, mode, &params);
                let plan = break_lines(&text, &items, BREAK_WIDTH, EM, mode, &params);
                assert!(
                    !plan.lines.is_empty(),
                    "{} in {} mode: the line breaker produced no lines",
                    shape.name(),
                    mode_name(mode)
                );
                cases.push(BreakCase {
                    label: format!("{}/{}", shape.name(), mode_name(mode)),
                    size: corpus::human(size),
                    text,
                    items,
                    mode,
                    fallback: plan.fallback,
                });
            }
        }
    }
    for (group_name, fallback) in [
        ("stages/linebreak/knuth-plass", false),
        ("stages/linebreak/greedy-fallback", true),
    ] {
        let mut group = c.benchmark_group(group_name);
        group.sample_size(10);
        for case in cases.iter().filter(|case| case.fallback == fallback) {
            group.throughput(Throughput::Bytes(case.text.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(&case.label, &case.size),
                &case.text,
                |b, text| {
                    stage(b, || {
                        break_lines(text, &case.items, BREAK_WIDTH, EM, case.mode, &params)
                    })
                },
            );
        }
        group.finish();
    }
}

criterion_group! {
    name = benches;
    config = config();
    targets = parse, serialize, compose_tree, assemble, spine, linebreak
}
criterion_main!(benches);
