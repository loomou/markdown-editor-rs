use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{Shape, generate, human};
use md_content::math::{key_for as math_key_for, raster as math_raster, spec_from_key};
use md_content::mermaid;
use md_content::mermaid::{
    MermaidKey, raster as mermaid_raster, spec_from_theme, theme_fingerprint,
};
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

fn svg_with_paths(paths: usize) -> Vec<u8> {
    let mut svg = String::from(
        "<svg xmlns='http://www.w3.org/2000/svg' width='512' height='512' \
         viewBox='0 0 512 512'><defs><linearGradient id='g'>\
         <stop offset='0' stop-color='#204080'/><stop offset='1' stop-color='#80c0ff'/>\
         </linearGradient></defs><g fill='url(#g)' stroke='#101820' stroke-width='0.5'>",
    );
    for i in 0..paths {
        let x = (i * 7 % 480) as f32;
        let y = (i * 11 % 480) as f32;
        svg.push_str(&format!(
            "<path d='M{x:.1} {y:.1}C{x2:.1} {y2:.1} {x3:.1} {y3:.1} {x4:.1} {y4:.1}\
             L{x5:.1} {y5:.1}Z'/>",
            x2 = x + 12.5,
            y2 = y + 3.25,
            x3 = x + 20.0,
            y3 = y + 17.75,
            x4 = x + 31.5,
            y4 = y + 9.0,
            x5 = x + 5.5,
            y5 = y + 24.0,
        ));
    }
    svg.push_str("</g></svg>");
    svg.into_bytes()
}

fn svg_source_bench(c: &mut Criterion) {
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
    let corpus: Vec<(usize, Vec<u8>)> = [32usize, 256, 2048]
        .into_iter()
        .map(|paths| (paths, svg_with_paths(paths)))
        .collect();
    let mut group = c.benchmark_group("content/svg-source-validate");
    group.sample_size(10);
    for (paths, bytes) in &corpus {
        assert!(
            md_content::images::validate_svg(bytes).is_ok(),
            "the synthetic svg must pass validation"
        );
        eprintln!("[svg-source {paths}] bytes={}", bytes.len());
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(paths), bytes, |b, bytes| {
            b.iter(|| md_content::images::validate_svg(bytes).is_ok())
        });
    }
    group.finish();

    let mut group = c.benchmark_group("content/svg-source-parse");
    group.sample_size(10);
    for (paths, bytes) in &corpus {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(paths), bytes, |b, bytes| {
            b.iter(|| renderer.parse_svg(bytes).is_ok())
        });
    }
    group.finish();

    let mut group = c.benchmark_group("content/svg-source-raster");
    group.sample_size(10);
    for (paths, bytes) in &corpus {
        let parsed = renderer.parse_svg(bytes).expect("svg parses");
        let target = gpui::Size::new(gpui::DevicePixels(512), gpui::DevicePixels(512));
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(paths), &parsed, |b, parsed| {
            b.iter(|| {
                renderer
                    .render_parsed(parsed, gpui::SvgSize::ExactSize(target))
                    .is_ok()
            })
        });
    }
    group.finish();
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
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
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
            .filter(|(latex, spec)| math_raster(latex, spec, &renderer).is_ok())
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
                        if math_raster(latex, spec, &renderer).is_ok() {
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

fn math_stage_bench(c: &mut Criterion) {
    let theme = md_bench::harness::document_theme();
    let color = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
    let mut group = c.benchmark_group("content/math-stage");
    group.sample_size(10);
    for size in SIZES {
        let all = formulas(Shape::Math, size, &theme);
        let sampled: Vec<(String, md_content::math::RasterSpec)> = all
            .iter()
            .take(MATH_SAMPLE)
            .map(|(latex, display)| {
                let key = math_key_for(latex, *display, MATH_FONT_SIZE, color, 1.0);
                (latex.clone(), spec_from_key(&key, color))
            })
            .collect();
        let prepared: Vec<(String, u32, u32)> = sampled
            .iter()
            .filter_map(|(latex, spec)| md_content::math::prepare_svg(latex, spec))
            .collect();
        assert!(
            !prepared.is_empty(),
            "the math corpus must prepare at least one sampled formula"
        );
        eprintln!(
            "[math-stage {}] distinct={} sampled={} prepared={}",
            human(size),
            all.len(),
            sampled.len(),
            prepared.len()
        );

        group.throughput(Throughput::Elements(sampled.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("svg", human(size)),
            &sampled,
            |b, sampled| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (latex, spec) in sampled {
                        if md_content::math::prepare_svg(latex, spec).is_some() {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );

        group.throughput(Throughput::Elements(prepared.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", human(size)),
            &prepared,
            |b, prepared| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (svg, _, _) in prepared {
                        if renderer.parse_svg(svg.as_bytes()).is_ok() {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );

        let parsed: Vec<_> = prepared
            .iter()
            .filter_map(|(svg, _, _)| renderer.parse_svg(svg.as_bytes()).ok())
            .collect();
        let targets: Vec<gpui::Size<gpui::DevicePixels>> = prepared
            .iter()
            .map(|(_, px_w, px_h)| {
                gpui::Size::new(
                    gpui::DevicePixels(*px_w as i32),
                    gpui::DevicePixels(*px_h as i32),
                )
            })
            .collect();
        group.throughput(Throughput::Elements(parsed.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("raster", human(size)),
            &(&parsed, &targets),
            |b, (parsed, targets)| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (parsed, target) in parsed.iter().zip(targets.iter()) {
                        if renderer
                            .render_parsed(parsed, gpui::SvgSize::ExactSize(*target))
                            .is_ok()
                        {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );

        group.throughput(Throughput::Elements(sampled.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("png", human(size)),
            &sampled,
            |b, sampled| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (latex, spec) in sampled {
                        if md_content::math::raster_png(latex, spec).is_ok() {
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
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
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
            .filter(|source| mermaid_raster(source, &spec, &renderer).is_ok())
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
                        if mermaid_raster(source, &spec, &renderer).is_ok() {
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
    let renderer = gpui::SvgRenderer::new(std::sync::Arc::new(()));
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
        let parsed: Vec<_> = sealed
            .iter()
            .filter_map(|sealed| renderer.parse_svg(sealed.as_str().as_bytes()).ok())
            .collect();
        let targets: Vec<_> = sealed
            .iter()
            .map(|sealed| {
                mermaid::target_for(
                    sealed.as_str(),
                    spec.fit_w as f32,
                    spec.fit_h as f32,
                    spec.dpr,
                )
            })
            .collect();
        assert!(
            !parsed.is_empty(),
            "the gpui parser must accept at least one sampled diagram"
        );
        group.throughput(Throughput::Elements(sealed.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", human(size)),
            &sealed,
            |b, sealed| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for sealed in sealed {
                        if renderer.parse_svg(sealed.as_str().as_bytes()).is_ok() {
                            ok += 1;
                        }
                    }
                    ok
                })
            },
        );
        group.throughput(Throughput::Elements(parsed.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("raster", human(size)),
            &(&parsed, &targets),
            |b, (parsed, targets)| {
                b.iter(|| {
                    let mut ok = 0usize;
                    for (parsed, target) in parsed.iter().zip(targets.iter()) {
                        let size = gpui::Size::new(
                            gpui::DevicePixels(target.px_w as i32),
                            gpui::DevicePixels(target.px_h as i32),
                        );
                        if renderer
                            .render_parsed(parsed, gpui::SvgSize::ExactSize(size))
                            .is_ok()
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
    targets = math_raster_bench,
    math_stage_bench,
    mermaid_render_bench,
    mermaid_stage_bench,
    svg_source_bench
}
criterion_main!(benches);
