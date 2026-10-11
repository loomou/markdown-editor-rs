use std::time::Duration;

use criterion::{
    BatchSize, Bencher, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};
use md_bench::corpus::{self, Shape, generate};
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use md_render::blocks::table::{CellRect, paint_cell_grid};
use md_render::search::{SearchScan, scan_document};
use md_render::snapshot::TableCorners;
use md_theme::DocumentTheme;

const SEED: u64 = 0x5eed_5ca4;
const SEARCH_SHAPES: [Shape; 3] = [Shape::PlainEn, Shape::PlainZh, Shape::KitchenSink];
const SEARCH_SIZES: [usize; 3] = [16 << 10, 256 << 10, 2 << 20];
const GRID_COUNTS: [usize; 5] = [800, 1600, 3200, 6400, 12800];

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(10)
}

fn cell_rects(count: usize) -> Vec<CellRect> {
    (0..count)
        .map(|i| CellRect {
            table: 1,
            rect: (((i % 100) * 30) as f64, ((i / 100) * 24) as f64, 30.0, 24.0),
            is_header: i < 100,
            corners: TableCorners::default(),
        })
        .collect()
}

struct SearchCase {
    label: String,
    size: String,
    doc: Doc,
    query: String,
}

fn search_cases() -> Vec<SearchCase> {
    let mut cases = Vec::new();
    for shape in SEARCH_SHAPES {
        for size in SEARCH_SIZES {
            let text = generate(shape, size, SEED);
            let hit: String = text.chars().take(8).collect();
            let queries = [("hit", hit), ("miss", "ZZQXJVqq".to_owned())];
            for (kind, query) in queries {
                cases.push(SearchCase {
                    label: format!("{}/{}", shape.name(), kind),
                    size: corpus::human(size),
                    doc: Doc::new(load_markdown(&text, editor_options())),
                    query,
                });
            }
        }
    }
    cases
}

fn search(c: &mut Criterion) {
    let cases = search_cases();
    let mut group = c.benchmark_group("render/search");
    group.sample_size(10);
    for case in &cases {
        group.bench_with_input(
            BenchmarkId::new(&case.label, &case.size),
            &case.query,
            |b, query| {
                b.iter_batched_ref(
                    || (),
                    |()| scan_document(&case.doc, query),
                    BatchSize::PerIteration,
                );
            },
        );
    }
    group.finish();
}

fn adversarial(c: &mut Criterion) {
    let mut group = c.benchmark_group("render/search/adversarial");
    group.sample_size(10);
    for hay in [30_000usize, 300_000] {
        let doc = Doc::new(load_markdown(&"a".repeat(hay), editor_options()));
        group.throughput(Throughput::Bytes(hay as u64));
        for prefix in [250usize, 500, 1000] {
            let query = format!("{}b", "a".repeat(prefix));
            group.bench_with_input(
                BenchmarkId::new("no-hit", format!("hay{hay}/q{prefix}")),
                &query,
                |b, query| {
                    b.iter_batched_ref(
                        || (),
                        |()| {
                            let result = scan_document(&doc, query);
                            assert_eq!(result, SearchScan::Empty);
                        },
                        BatchSize::PerIteration,
                    );
                },
            );
        }
    }
    group.finish();
}

fn cell_grid(c: &mut Criterion) {
    let theme = DocumentTheme::one_dark();
    let mut group = c.benchmark_group("render/cell-grid");
    group.sample_size(10);
    for count in GRID_COUNTS {
        let cells = cell_rects(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &cells, |b, cells| {
            stage(b, || {
                let ops = paint_cell_grid(cells, &theme);
                assert!(!ops.is_empty(), "the grid must paint at least one op");
                ops.len()
            });
        });
    }
    group.finish();
}

fn stage<O>(b: &mut Bencher<'_>, mut routine: impl FnMut() -> O) {
    b.iter_batched_ref(|| (), |()| routine(), BatchSize::PerIteration);
}

criterion_group! {
    name = benches;
    config = config();
    targets = search, adversarial, cell_grid
}
criterion_main!(benches);
