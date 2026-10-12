use crate::corpus::{
    BLOCK_COUNTS, BLOCK_SEED, RENDER_FRAME_BLOCK_COUNTS, Shape, generate, generate_blocks, human,
    human_blocks,
};
use crate::harness::{caret, doc, document_theme, environment, leaf_end};
use gpui::{
    BenchAppContext, Context, Entity, IntoElement, Render, Styled as _, Window, div, px, size,
};
use md_content::shaper::{GpuiShaper, ShapeMedia};
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::Doc;
use md_core::document::Command;
use md_editor::Shell;
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_theme::{DocumentTheme, LineBreakMode};
use std::fmt;
use std::rc::Rc;

pub const SEED: u64 = 0x5EED_0A11;

pub const VIEWPORT_H: Px = 600.0;

pub const PIPE_SHAPES: [Shape; 4] = [
    Shape::PlainEn,
    Shape::PlainZh,
    Shape::MixedZhEn,
    Shape::KitchenSink,
];

pub const PIPE_SIZES: [usize; 2] = [16 << 10, 256 << 10];

pub const KEEP_SCREENS: Px = 2.0;

pub struct ProbeView;

impl Render for ProbeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().w(px(64.)).h(px(64.))
    }
}

pub struct FrameCase {
    pub label: String,
    pub text: String,
    pub doc: Doc,
    pub theme: DocumentTheme,
    pub env: BoxLayoutEnvironment,
}

impl FrameCase {
    pub fn fresh_doc(&self) -> Doc {
        doc(&self.text)
    }
}

impl fmt::Display for FrameCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn empty_media() -> ShapeMedia {
    ShapeMedia {
        mermaid_fitted: Rc::new(Default::default()),
        math_metrics: Rc::new(Default::default()),
        math_gen: 0,
        image_sizes: Rc::new(Default::default()),
        image_failed: Rc::new(Default::default()),
        image_gen: 0,
        link_dests: Rc::new(Default::default()),
        link_raw: Rc::new(Default::default()),
        block_image_dest: Rc::new(Default::default()),
        block_code_lang: Rc::new(Default::default()),
    }
}

pub fn cases() -> Vec<FrameCase> {
    let mut out = Vec::with_capacity(PIPE_SHAPES.len() * PIPE_SIZES.len());
    for shape in PIPE_SHAPES {
        for size in PIPE_SIZES {
            let text = generate(shape, size, SEED);
            out.push(FrameCase {
                label: format!("{}-{}", shape.name(), human(size)),
                doc: doc(&text),
                text,
                theme: document_theme(),
                env: environment(),
            });
        }
    }
    out
}

pub fn large_cases() -> Vec<FrameCase> {
    cases_for_blocks(&BLOCK_COUNTS)
}

pub fn render_frame_cases() -> Vec<FrameCase> {
    cases_for_blocks(&RENDER_FRAME_BLOCK_COUNTS)
}

fn cases_for_blocks(counts: &[usize]) -> Vec<FrameCase> {
    let mut out = Vec::with_capacity(counts.len());
    for &blocks in counts {
        let text = generate_blocks(blocks, BLOCK_SEED);
        let parsed = doc(&text);
        let leaves = parsed.text_leaves().len();
        assert!(
            leaves >= blocks / 4,
            "the large corpus parsed to too few leaves for its block count"
        );
        out.push(FrameCase {
            label: format!("blocks-{}", human_blocks(blocks)),
            doc: parsed,
            text,
            theme: document_theme(),
            env: environment(),
        });
    }
    out
}

pub fn editor_window_bounds(scroll: Px, viewport_h: Px) -> (Px, Px) {
    let pad = KEEP_SCREENS * viewport_h;
    ((scroll - pad).max(0.0), scroll + viewport_h + pad)
}

pub struct Step {
    pub iterations: u32,
    pub islands: u64,
}

pub fn mid_leaf(doc: &Doc) -> (BlockId, usize) {
    let leaves = doc.text_leaves();
    let leaf = leaves.get(leaves.len() / 2).copied().unwrap_or(0);
    (leaf, leaf_end(doc, leaf) / 2)
}

pub fn build_engine(case: &FrameCase, top: Px, bottom: Px) -> IncrementalEngine {
    IncrementalEngine::with_window(
        &case.doc.document,
        case.env,
        Estimator::from_theme(&case.theme),
        case.theme.layout_theme(),
        top,
        bottom,
    )
}

pub fn open_engine(
    case: &FrameCase,
    shaper: &GpuiShaper,
) -> (Doc, IncrementalEngine, (BlockId, usize)) {
    let doc = case.fresh_doc();
    let cursor = mid_leaf(&doc);
    let (top, bottom) = editor_window_bounds(0.0, VIEWPORT_H);
    let mut engine = IncrementalEngine::with_window(
        &doc.document,
        case.env,
        Estimator::from_theme(&case.theme),
        case.theme.layout_theme(),
        top,
        bottom,
    );
    engine.assemble_with_doc(
        &doc.document,
        ScrollAnchor::top(),
        VIEWPORT_H,
        shaper,
        &FallbackSolver,
    );
    (doc, engine, cursor)
}

pub fn run_keystroke(
    doc: &mut Doc,
    engine: &mut IncrementalEngine,
    shaper: &GpuiShaper,
    (leaf, offset): (BlockId, usize),
) -> Step {
    let cursor = doc.apply(
        caret(leaf, offset),
        Command::Insert {
            text: "x".to_owned(),
        },
    );
    let changes = doc.take_changes();
    engine.apply_changes(&doc.document, &changes);
    let scroll =
        engine.sync_block_edit_retain_y(&doc.document, cursor.block, 0.0, shaper, &FallbackSolver);
    engine.materialize_pin_block(cursor.block, shaper, &FallbackSolver);
    let anchor = engine.anchor_at_y(scroll, shaper, &FallbackSolver);
    let (assembly, published) =
        engine.assemble_with_doc(&doc.document, anchor, VIEWPORT_H, shaper, &FallbackSolver);
    Step {
        iterations: published.iterations,
        islands: assembly.island_stats.islands_built,
    }
}

pub fn run_scroll(doc: &Doc, engine: &mut IncrementalEngine, shaper: &GpuiShaper) -> Step {
    let anchor = engine.anchor_at_y(VIEWPORT_H, shaper, &FallbackSolver);
    let (assembly, published) =
        engine.assemble_with_doc(&doc.document, anchor, VIEWPORT_H, shaper, &FallbackSolver);
    Step {
        iterations: published.iterations,
        islands: assembly.island_stats.islands_built,
    }
}

pub fn count_hot_path(label: &str, run: impl FnOnce()) -> md_layout::hot_path::HotPathSnapshot {
    md_layout::hot_path::reset();
    run();
    let snapshot = md_layout::hot_path::snapshot();
    eprintln!("[counters {label}] {snapshot}");
    snapshot
}

pub fn theme_with_mode(mode: LineBreakMode) -> DocumentTheme {
    let mut theme = document_theme();
    theme.line_break.mode = mode;
    theme
}

pub fn text_nodes(
    doc: &Doc,
    theme: &DocumentTheme,
) -> (md_layout::box_tree::BoxTree, Vec<LayoutBoxId>) {
    let tree = md_layout::compose::compose(&doc.document, &theme.layout_theme());
    let mut nodes = Vec::new();
    for (id, node) in tree.nodes() {
        if !tree.text_of(node).is_empty() {
            nodes.push(*id);
        }
    }
    (tree, nodes)
}

pub const EDITOR_VIEWPORT: (f32, f32) = (900.0, 600.0);

pub fn mount_shell(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) -> Entity<Shell> {
    let mut window = cx.add_empty_window();
    let shell = window.update(|window, cx| {
        window.resize(size(px(EDITOR_VIEWPORT.0), px(EDITOR_VIEWPORT.1)));
        window.replace_root(cx, |_window, cx| Shell::new(case.fresh_doc(), cx))
    });
    window.update(|window, cx| {
        let focus = shell.read(cx).editor_focus().clone();
        focus.focus(window, cx);
    });
    shell
}
