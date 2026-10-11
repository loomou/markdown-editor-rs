use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use gpui::BenchAppContext;
use md_bench::corpus::{Shape, generate, human};
use md_bench::gpui_support::{FrameCase, empty_media, text_nodes};
use md_bench::harness::{doc, document_theme, environment};
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_layout::box_tree::{BoxTree, LayoutBoxId};

const SEED: u64 = 0x5eed_11a7;
const SIZES: [usize; 3] = [16 << 10, 64 << 10, 256 << 10];
const LANGS: [(&str, &str); 5] = [
    ("no-lang", ""),
    ("rust", "rust"),
    ("python", "python"),
    ("json", "json"),
    ("markdown", "markdown"),
];
const LANG_SIZE: usize = 64 << 10;

fn frame_case(shape: Shape, size: usize) -> FrameCase {
    let text = generate(shape, size, SEED);
    FrameCase {
        label: format!("{}-{}", shape.name(), human(size)),
        doc: doc(&text),
        text,
        theme: document_theme(),
        env: environment(),
    }
}

fn media_with_lang(tree: &BoxTree, nodes: &[LayoutBoxId], lang: &str) -> ShapeMedia {
    let mut langs = HashMap::new();
    for id in nodes {
        let node = tree.get(*id);
        if node.kind() == BlockKind::CodeBlock {
            langs.insert(node.shape_ident().index, lang.to_owned());
        }
    }
    let mut media = empty_media();
    media.block_code_lang = Rc::new(langs);
    media
}

fn code_nodes(tree: &BoxTree, nodes: &[LayoutBoxId]) -> Vec<LayoutBoxId> {
    nodes
        .iter()
        .copied()
        .filter(|id| tree.get(*id).kind() == BlockKind::CodeBlock)
        .collect()
}

struct HighlightCase {
    inner: FrameCase,
    lang: &'static str,
    label: &'static str,
    cold: bool,
}

impl fmt::Display for HighlightCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.label)?;
        f.write_str("/")?;
        f.write_str(self.label)?;
        f.write_str(if self.cold { "/cold" } else { "/hot" })
    }
}

fn size_cases() -> Vec<HighlightCase> {
    let mut out = Vec::new();
    for size in SIZES {
        for cold in [true, false] {
            out.push(HighlightCase {
                inner: frame_case(Shape::CodeFences, size),
                lang: "rust",
                label: "rust",
                cold,
            });
        }
    }
    out
}

fn lang_cases() -> Vec<HighlightCase> {
    LANGS
        .iter()
        .map(|&(label, lang)| HighlightCase {
            inner: frame_case(Shape::CodeFences, LANG_SIZE),
            lang,
            label,
            cold: true,
        })
        .collect()
}

#[gpui::bench(inputs = size_cases(), input_name = "case", group = "content/highlight", sample_size = 10)]
fn highlight(case: &HighlightCase, cx: &mut BenchAppContext<'_, '_>) {
    let fc = &case.inner;
    let (tree, nodes) = text_nodes(&fc.doc, &fc.theme);
    let media = media_with_lang(&tree, &nodes, case.lang);
    let code = code_nodes(&tree, &nodes);
    assert!(
        !code.is_empty(),
        "the code-fence corpus must contain code blocks"
    );
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window
        .update(|window, app| GpuiShaper::new(window, app, &fc.theme, 1.0, cache.clone(), media));
    {
        let node = tree.get(code[0]);
        let probe = shaper.artifact(
            tree.text_of(node),
            tree.runs_of(node),
            tree.content_width(code[0], fc.env.viewport_width),
            node.kind(),
            node.shape_ident(),
        );
        eprintln!(
            "[highlight {}] code-blocks={} first-line-colors={}",
            case,
            code.len(),
            probe.line_colors.len()
        );
        assert!(
            !probe.line_colors.is_empty(),
            "the first code block produced no syntax colors, so highlighting never ran"
        );
    }
    let env = fc.env;
    let cold = case.cold;
    let mut epoch = 0u64;
    cx.bench_iter(move |_cx| {
        if cold {
            epoch += 1;
            cache.begin_frame(epoch);
        }
        for id in &code {
            let node = tree.get(*id);
            let art = shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
            assert!(!art.lines.is_empty(), "a code block shaped to no lines");
        }
        if cold {
            cache.end_frame();
        }
    });
}

#[gpui::bench(inputs = lang_cases(), input_name = "case", group = "content/highlight-lang", sample_size = 10)]
fn highlight_lang(case: &HighlightCase, cx: &mut BenchAppContext<'_, '_>) {
    let fc = &case.inner;
    let (tree, nodes) = text_nodes(&fc.doc, &fc.theme);
    let media = media_with_lang(&tree, &nodes, case.lang);
    let code = code_nodes(&tree, &nodes);
    assert!(
        !code.is_empty(),
        "the code-fence corpus must contain code blocks"
    );
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window
        .update(|window, app| GpuiShaper::new(window, app, &fc.theme, 1.0, cache.clone(), media));
    let env = fc.env;
    let mut epoch = 0u64;
    cx.bench_iter(move |_cx| {
        epoch += 1;
        cache.begin_frame(epoch);
        for id in &code {
            let node = tree.get(*id);
            let art = shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
            assert!(!art.lines.is_empty(), "a code block shaped to no lines");
        }
        cache.end_frame();
    });
}

gpui::bench_group!(benches, highlight, highlight_lang);
gpui::bench_main!(benches);
