use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{Shape, generate, human};
use md_content::math::{key_for as math_key_for, raster as math_raster, spec_from_key};
use md_content::mermaid::{
    MermaidKey, raster as mermaid_raster, spec_from_theme, theme_fingerprint,
};
use md_content::mermaid;
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::inline::InlineMarks;
use md_layout::box_tree::{BoxTree, LayoutBoxId};
use md_theme::{DocumentTheme, ThemeColor};

const SEED: u64 = 0x5eed_0ca5;
const SIZES: [usize; 3] = [16 << 10, 64 << 10, 256 << 10];
const MATH_FONT_SIZE: f32 = 16.0;
const MATH_SAMPLE: usize = 64;
const MERMAID_SAMPLE: usize = 16;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
        .sample_size(10)
}

fn text_nodes(doc: &Doc, theme: &DocumentTheme) -> (BoxTree, Vec<LayoutBoxId>) {
    let tree = md_layout::compose::compose(&doc.document, &theme.layout_theme());
    let mut nodes = Vec::new();
    for (id, node) in tree.nodes() {
        if !tree.text_of(node).is_empty() {
            nodes.push(*id);
        }
    }
    (tree, nodes)
}

fn formulas(shape: Shape, size: usize, theme: &DocumentTheme) -> Vec<(String, bool)> {
    let text = generate(shape, size, SEED);
    let doc = md_bench::harness::doc(&text);
    let (tree, nodes) = text_nodes(&doc, theme);
    let mut out = Vec::new();
    for id in &nodes {
        let node = tree.get(*id);
        let body = tree.text_of(node);
        if node.kind() == BlockKind::Math {
            out.push((body.to_owned(), true));
        }
        for run in tree.runs_of(node) {
            if !run.marks.is_math() {
                continue;
            }
            let span = run.display_range.start as usize..run.display_range.end as usize;
            let Some(latex) = body.get(span) else {
                continue;
            };
            out.push((
                latex.to_owned(),
                run.marks.contains(InlineMarks::MATH_DISPLAY),
            ));
        }
    }
    out
}

fn diagrams(shape: Shape, size: usize, theme: &DocumentTheme) -> Vec<String> {
    let text = generate(shape, size, SEED);
    let doc = md_bench::harness::doc(&text);
    let (tree, nodes) = text_nodes(&doc, theme);
    let mut out = Vec::new();
    for id in &nodes {
        let node = tree.get(*id);
        if node.kind() == BlockKind::Mermaid {
            out.push(tree.text_of(node).to_owned());
        }
    }
    out
}

fn math_raster_bench(c: &mut Criterion) {
    let theme = md_bench::harness::document_theme();
    let color = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
    let mut group = c.benchmark_group("content/math-raster");
    group.sample_size(10);
    for size in SIZES {
        let all = formulas(Shape::Math, size, &theme);
        assert!(
            !all.is_empty(),
            "the math corpus must yield at least one formula"
        );
        let sampled: Vec<(String, md_content::math::RasterSpec)> = all
            .iter()
            .take(MATH_SAMPLE)
            .map(|(latex, display)| {
                let key = math_key_for(latex, *display, MATH_FONT_SIZE, color, 1.0);
                (latex.clone(), spec_from_key(&key, color))
            })
            .collect();
        let rasterised = sampled
            .iter()
            .filter(|(latex, spec)| math_raster(latex, spec).is_ok())
            .count();
        assert!(
            rasterised > 0,
            "the math corpus must rasterise at least one sampled formula"
        );
        eprintln!(
            "[math-raster {}] distinct={} sampled={} rasterised={}",
            human(size),
            all.len(),
            sampled.len(),
            rasterised
        );
        group.throughput(Throughput::Elements(sampled.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(human(size)),
            &sampled,
            |b, sampled| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (latex, spec) in sampled {
                        if math_raster(latex, spec).is_ok() {
                            ok += 1;
                        }
                    }
                    ok
                });
            },
        );
    }
    group.finish();
}

fn mermaid_render_bench(c: &mut Criterion) {
    let theme = md_bench::harness::document_theme();
    let fingerprint = theme_fingerprint(&theme);
    let key = MermaidKey {
        block: 0,
        generation: 0,
        revision: 0,
        theme_fp: fingerprint,
        width_q: 320,
        max_h: 10_000,
        dpr_q: md_content::images::dpr_q(1.0),
    };
    let spec = spec_from_theme(&theme, key);
    let mut group = c.benchmark_group("content/mermaid-render");
    group.sample_size(10);
    for size in SIZES {
        let all = diagrams(Shape::Mermaid, size, &theme);
        assert!(
            !all.is_empty(),
            "the mermaid corpus must yield at least one diagram"
        );
        let sampled: Vec<String> = all.iter().take(MERMAID_SAMPLE).cloned().collect();
        let rendered = sampled
            .iter()
            .filter(|source| mermaid_raster(source, &spec).is_ok())
            .count();
        assert!(
            rendered > 0,
            "the mermaid corpus must render at least one sampled diagram"
        );
        eprintln!(
            "[mermaid-render {}] distinct={} sampled={} rendered={}",
            human(size),
            all.len(),
            sampled.len(),
            rendered
        );
        group.throughput(Throughput::Elements(sampled.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(human(size)),
            &sampled,
            |b, sampled| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for source in sampled {
                        if mermaid_raster(source, &spec).is_ok() {
                            ok += 1;
                        }
                    }
                    ok
                });
            },
        );
    }
    group.finish();
}

fn mermaid_stage_bench(c: &mut Criterion) {
    let theme = md_bench::harness::document_theme();
    let fingerprint = theme_fingerprint(&theme);
    let key = MermaidKey {
        block: 0,
        generation: 0,
        revision: 0,
        theme_fp: fingerprint,
        width_q: 320,
        max_h: 10_000,
        dpr_q: md_content::images::dpr_q(1.0),
    };
    let spec = spec_from_theme(&theme, key);
    let mut group = c.benchmark_group("content/mermaid-stage");
    group.sample_size(10);
    for size in SIZES {
        let all = diagrams(Shape::Mermaid, size, &theme);
        assert!(
            !all.is_empty(),
            "the mermaid corpus must yield at least one diagram"
        );
        let sampled: Vec<String> = all.iter().take(MERMAID_SAMPLE).cloned().collect();
        let sealed: Vec<_> = sampled
            .iter()
            .filter_map(|source| mermaid::render_sealed(source, &spec).ok())
            .collect();
        assert!(
            !sealed.is_empty(),
            "the mermaid corpus must seal at least one sampled diagram"
        );
        eprintln!(
            "[mermaid-stage {}] distinct={} sampled={} sealed={}",
            human(size),
            all.len(),
            sampled.len(),
            sealed.len()
        );
        group.throughput(Throughput::Elements(sampled.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("layout", human(size)),
            &sampled,
            |b, sampled| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for source in sampled {
                        if mermaid::render_sealed(source, &spec).is_ok() {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );
        group.throughput(Throughput::Elements(sealed.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse", human(size)), &sealed, |b, sealed| {
            b.iter(|| {
                let mut ok = 0usize;
                for sealed in sealed {
                    if mermaid::prepare_sealed(sealed, &spec).is_ok() {
                        ok += 1;
                    }
                }
                ok
            })
        });
        group.bench_with_input(
            BenchmarkId::new("parse-and-encode", human(size)),
            &sealed,
            |b, sealed| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for sealed in sealed {
                        if mermaid::prepare_sealed(sealed, &spec)
                            .is_ok_and(|prepared| prepared.encode_png().is_ok())
                        {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = math_raster_bench, mermaid_render_bench, mermaid_stage_bench
}
criterion_main!(benches);
