use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::rc::Rc;

use gpui::{App, BenchAppContext};
use md_bench::corpus::{Shape, generate, human};
use md_bench::gpui_support::{FrameCase, empty_media, text_nodes};
use md_bench::harness::{doc, document_theme, environment};
use md_content::images::cache_key;
use md_content::math::{MathCache, MathEm, RasterOut, key_for};
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::inline::InlineMarks;
use md_layout::box_tree::{BoxTree, LayoutBoxId};
use md_theme::ThemeColor;

const SEED: u64 = 0x5eed_0ca5;
const SIZES: [usize; 3] = [16 << 10, 64 << 10, 256 << 10];

struct MediaCase {
    inner: FrameCase,
    hot: bool,
}

impl fmt::Display for MediaCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.label)?;
        f.write_str(if self.hot { "/hot" } else { "/cold" })
    }
}

fn frame_case(shape: Shape, size: usize) -> FrameCase {
    let text = generate(shape, size, SEED);
    FrameCase {
        label: format!("{}-{}", shape.name(), human(size)),
        doc: doc(&text),
        text,
        theme: document_theme(),
        env: environment(),
        path: None,
        job_bytes: None,
    }
}

fn media_cases(shape: Shape) -> Vec<MediaCase> {
    let mut out = Vec::new();
    for size in SIZES {
        for hot in [false, true] {
            out.push(MediaCase {
                inner: frame_case(shape, size),
                hot,
            });
        }
    }
    out
}

fn image_dests(doc: &Doc) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    doc.document.for_each_extra(|index, extra| {
        if let Some(dest_id) = extra.image_dest() {
            let dest = doc.document.link_dest(dest_id).unwrap_or("");
            out.insert(index, cache_key(dest, None::<&Path>));
        }
    });
    out
}

fn math_media(tree: &BoxTree, nodes: &[LayoutBoxId], app: &mut App) -> ShapeMedia {
    let color = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
    let mut cache = MathCache::new();
    let mut latexes: Vec<(String, bool)> = Vec::new();
    for id in nodes {
        let node = tree.get(*id);
        let text = tree.text_of(node);
        if node.kind() == BlockKind::Math {
            latexes.push((text.to_owned(), true));
        }
        for run in tree.runs_of(node) {
            if !run.marks.is_math() {
                continue;
            }
            let span = run.display_range.start as usize..run.display_range.end as usize;
            let Some(latex) = text.get(span) else {
                continue;
            };
            latexes.push((
                latex.to_owned(),
                run.marks.contains(InlineMarks::MATH_DISPLAY),
            ));
        }
    }
    for (latex, display) in &latexes {
        let key = key_for(latex, *display, 16.0, color, 1.0);
        if cache.begin(key.clone()) {
            cache.finish(
                key,
                Ok(RasterOut {
                    em: MathEm {
                        width: 3.0,
                        height: 0.7,
                        depth: 0.25,
                    },
                    image: None,
                }),
                app,
            );
        }
    }
    eprintln!("[math-media] recorded={}", latexes.len());
    let mut media = empty_media();
    media.math_metrics = cache.metrics_snapshot();
    media.math_gen = cache.metrics_gen();
    media
}

fn mermaid_media(tree: &BoxTree, nodes: &[LayoutBoxId]) -> ShapeMedia {
    let mut fitted = HashMap::new();
    for id in nodes {
        let node = tree.get(*id);
        if node.kind() == BlockKind::Mermaid {
            let ident = node.shape_ident();
            fitted.insert((ident.index, ident.generation), (320.0f32, 180.0f32));
        }
    }
    eprintln!("[mermaid-media] fitted={}", fitted.len());
    let mut media = empty_media();
    media.mermaid_fitted = Rc::new(fitted);
    media
}

fn link_maps(doc: &Doc) -> (HashMap<u32, String>, HashMap<u32, (String, String)>) {
    let path = doc.source_path.as_deref();
    let mut dests = HashMap::new();
    let mut raw = HashMap::new();
    for (i, link) in doc.document.links.iter().enumerate() {
        dests.insert(i as u32, cache_key(&link.dest, path));
        raw.insert(i as u32, (link.dest.clone(), link.title.clone()));
    }
    (dests, raw)
}

fn image_media(doc: &Doc, hot: bool) -> ShapeMedia {
    let (links, raw) = link_maps(doc);
    let block = image_dests(doc);
    let mut media = empty_media();
    media.link_dests = Rc::new(links.clone());
    media.link_raw = Rc::new(raw);
    media.block_image_dest = Rc::new(block.clone());
    if hot {
        let sizes: HashMap<md_content::images::SourceKey, (u32, u32)> = links
            .values()
            .chain(block.values())
            .map(|dest| {
                (
                    md_content::images::SourceKey::new(dest.clone()),
                    (320u32, 200u32),
                )
            })
            .collect();
        eprintln!(
            "[image-media] links={} block-images={} sizes={}",
            links.len(),
            block.len(),
            sizes.len()
        );
        media.image_sizes = Rc::new(sizes);
    }
    media
}

fn drive(
    case: &MediaCase,
    cx: &mut BenchAppContext<'_, '_>,
    build: impl FnOnce(&BoxTree, &[LayoutBoxId], &mut App) -> ShapeMedia,
) {
    let fc = &case.inner;
    let (tree, nodes) = text_nodes(&fc.doc, &fc.theme);
    assert!(
        !nodes.is_empty(),
        "the corpus must parse to at least one text node"
    );
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        let media = build(&tree, &nodes, app);
        GpuiShaper::new(window, app, &fc.theme, 1.0, cache.clone(), media)
    });
    let env = fc.env;
    let mut epoch = 0u64;
    cx.bench_iter(move |_cx| {
        epoch += 1;
        cache.begin_frame(epoch);
        for id in &nodes {
            let node = tree.get(*id);
            let art = shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
            assert!(
                art.rows >= 1 && art.height > 0.0,
                "the shaper produced a degenerate artifact"
            );
        }
        cache.end_frame();
    });
}

#[gpui::bench(inputs = media_cases(Shape::Math), input_name = "case", group = "content/math", sample_size = 10)]
fn math_cache(case: &MediaCase, cx: &mut BenchAppContext<'_, '_>) {
    let hot = case.hot;
    drive(case, cx, move |tree, nodes, app| {
        if hot {
            math_media(tree, nodes, app)
        } else {
            empty_media()
        }
    });
}

#[gpui::bench(inputs = media_cases(Shape::Mermaid), input_name = "case", group = "content/mermaid", sample_size = 10)]
fn mermaid_cache(case: &MediaCase, cx: &mut BenchAppContext<'_, '_>) {
    let hot = case.hot;
    drive(case, cx, move |tree, nodes, _app| {
        if hot {
            mermaid_media(tree, nodes)
        } else {
            empty_media()
        }
    });
}

#[gpui::bench(inputs = media_cases(Shape::Images), input_name = "case", group = "content/images", sample_size = 10)]
fn image_cache(case: &MediaCase, cx: &mut BenchAppContext<'_, '_>) {
    let hot = case.hot;
    let (links, _raw) = link_maps(&case.inner.doc);
    let dests = image_dests(&case.inner.doc);
    assert!(
        !dests.is_empty() && !links.is_empty(),
        "the image corpus must declare both block and inline images"
    );
    drive(case, cx, move |_tree, _nodes, _app| {
        image_media(&case.inner.doc, hot)
    });
}

gpui::bench_group!(benches, math_cache, mermaid_cache, image_cache);
gpui::bench_main!(benches);
