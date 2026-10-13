use gpui::BenchAppContext;
use md_bench::corpus::{Shape, generate, human};
use md_bench::gpui_support::{PIPE_SIZES, VIEWPORT_H, empty_media, text_nodes, theme_with_mode};
use md_bench::harness::{doc, environment, first_leaf};
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_core::doc::{Cursor, Doc};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{FrameContext, FrameRequest, compose};
use md_render::snap::SnapOperator;
use md_theme::{DocumentTheme, LineBreakMode};
use std::fmt;

const SEED: u64 = 0x5EED_0A11;
const SHAPES: [Shape; 3] = [Shape::PlainEn, Shape::PlainZh, Shape::KitchenSink];
const MODES: [LineBreakMode; 3] = [
    LineBreakMode::Greedy,
    LineBreakMode::Optimal,
    LineBreakMode::Justify,
];

fn mode_name(mode: LineBreakMode) -> &'static str {
    mode.key()
}

struct ModeCase {
    label: String,
    text: String,
    theme: DocumentTheme,
    env: BoxLayoutEnvironment,
}

impl fmt::Display for ModeCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn cases() -> Vec<ModeCase> {
    let mut out = Vec::new();
    for shape in SHAPES {
        for size in PIPE_SIZES {
            let text = generate(shape, size, SEED);
            for mode in MODES {
                out.push(ModeCase {
                    label: format!("{}-{}/{}", shape.name(), human(size), mode_name(mode)),
                    text: text.clone(),
                    theme: theme_with_mode(mode),
                    env: environment(),
                });
            }
        }
    }
    out
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "linebreak/artifact", sample_size = 20)]
fn artifact(case: &ModeCase, cx: &mut BenchAppContext<'_, '_>) {
    let doc = doc(&case.text);
    let (tree, nodes) = text_nodes(&doc, &case.theme);
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(window, app, &case.theme, 1.0, cache.clone(), empty_media())
    });
    let env = case.env;
    let node_count = nodes.len();
    cx.bench_iter(move |_cx| {
        for id in &nodes {
            let node = tree.get(*id);
            shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
        }
    });
    eprintln!(
        "[linebreak/artifact {}] text-nodes={} cache={:?}",
        case.label,
        node_count,
        cache.stats()
    );
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "linebreak/frame", sample_size = 10)]
fn frame(case: &ModeCase, cx: &mut BenchAppContext<'_, '_>) {
    let doc = doc(&case.text);
    let mut window = cx.add_empty_window();
    let (shaper, snap) = window.update(|window, app| {
        (
            GpuiShaper::new(
                window,
                app,
                &case.theme,
                1.0,
                ShapeCache::new(),
                empty_media(),
            ),
            SnapOperator::new(1.0),
        )
    });
    let block = first_leaf(&doc);
    let env = case.env;
    let theme = &case.theme;
    let doc_ref: &Doc = &doc;
    cx.bench_iter(move |_cx| {
        let composed = compose(
            FrameContext {
                doc: doc_ref,
                env,
                shaper: &shaper,
                snap: &snap,
                theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, VIEWPORT_H),
                scroll: 0.0,
                cursor: Cursor { block, offset: 0 },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        assert!(
            !composed.texts.is_empty(),
            "a composed frame must paint at least one text piece"
        );
    });
}

gpui::bench_group!(benches, artifact, frame);
gpui::bench_main!(benches);
