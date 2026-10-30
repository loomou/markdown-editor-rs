use crate::corpus::{
    self, generate, generate_blocks, human, human_blocks, Shape, BLOCK_COUNTS, BLOCK_SEED,
    GIANT_IMAGE, MEDIA_IMAGES, MEDIA_LARGE, MEDIA_MERMAID, RENDER_FRAME_BLOCK_COUNTS,
};
use crate::harness::{caret, doc, doc_with_path, document_theme, environment, leaf_end};
use gpui::{
    div, px, size, BenchAppContext, Context, Entity, IntoElement, Render, Styled as _, Window,
};
use md_content::shaper::{GpuiShaper, ShapeMedia};
use md_core::block::BlockId;
use md_core::doc::Doc;
use md_core::document::Command;
use md_core::Px;
use md_editor::Shell;
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_theme::{DocumentTheme, LineBreakMode};
use std::fmt;
use std::path::PathBuf;
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
    pub path: Option<PathBuf>,
    /// Job budget this case must be measured under. `None` keeps the editor's
    /// default, so a case can compare one document at more than one ceiling.
    pub job_bytes: Option<usize>,
    /// Resident ceiling this case must be measured under. `None` follows the
    /// viewport, which is what a real window does. A case that sets it asks
    /// what happens when the caches cannot hold everything the document has
    /// shown, which is the only situation a warm cache can help with.
    pub resident_bytes: Option<usize>,
    /// Frames rendered after each wrap without scrolling. A cooling candidate
    /// needs to stay off screen for the grace, and a pass that wraps every
    /// fourteen frames never gives it that long.
    pub dwell_ms: u64,
}

impl FrameCase {
    pub fn fresh_doc(&self) -> Doc {
        match &self.path {
            Some(path) => doc_with_path(&self.text, path.clone()),
            None => doc(&self.text),
        }
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
                path: None,
                job_bytes: None,
                resident_bytes: None,
                dwell_ms: 0,
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

pub fn cases_for_blocks(counts: &[usize]) -> Vec<FrameCase> {
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
            path: None,
            job_bytes: None,
            resident_bytes: None,
            dwell_ms: 0,
        });
    }
    out
}

pub fn editor_window_bounds(scroll: Px, viewport_h: Px) -> (Px, Px) {
    let pad = KEEP_SCREENS * viewport_h;
    ((scroll - pad).max(0.0), scroll + viewport_h + pad)
}

pub fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

pub fn media_root() -> PathBuf {
    corpus_root().join("media")
}

fn media_case(label: &str, text: String, path: PathBuf) -> FrameCase {
    FrameCase {
        label: label.to_owned(),
        doc: doc_with_path(&text, path.clone()),
        text,
        theme: document_theme(),
        env: environment(),
        path: Some(path),
        job_bytes: None,
        resident_bytes: None,
        dwell_ms: 0,
    }
}

fn media_case_under(label: &str, text: String, path: PathBuf, job_bytes: usize) -> FrameCase {
    FrameCase {
        job_bytes: Some(job_bytes),
        ..media_case(label, text, path)
    }
}

/// A case whose caches cannot hold every image the document has shown. The
/// ceiling is set from what the full-corpus cases actually hold, so the slice
/// is smaller than the content rather than arbitrarily small.
fn media_case_tight(
    label: &str,
    text: String,
    path: PathBuf,
    resident_bytes: usize,
    dwell_ms: u64,
) -> FrameCase {
    FrameCase {
        resident_bytes: Some(resident_bytes),
        dwell_ms,
        ..media_case(label, text, path)
    }
}

pub fn media_cases() -> Vec<FrameCase> {
    let root = media_root();
    vec![
        media_case(
            "media-images-12",
            corpus::media_images_doc(&MEDIA_IMAGES, true),
            root.join("images.md"),
        ),
        media_case_tight(
            "media-images-12-tight",
            corpus::media_images_doc(&MEDIA_IMAGES, true),
            root.join("images.md"),
            120 << 20,
            400,
        ),
        media_case(
            "media-image-giant",
            corpus::media_images_doc(&[GIANT_IMAGE], true),
            root.join("giant.md"),
        ),
        media_case(
            "media-images-large-4",
            corpus::media_images_doc(&MEDIA_LARGE, true),
            root.join("large.md"),
        ),
        media_case_under(
            "media-images-large-4-unlimited",
            corpus::media_images_doc(&MEDIA_LARGE, true),
            root.join("large.md"),
            usize::MAX,
        ),
        media_case(
            "media-mermaid-12",
            corpus::media_mermaid_doc(&MEDIA_MERMAID),
            root.join("mermaid.md"),
        ),
        media_case_tight(
            "media-mermaid-24-tight",
            corpus::media_mermaid_long_doc(),
            root.join("mermaid.md"),
            170 << 20,
            400,
        ),
        media_case(
            "media-mermaid-repeat",
            corpus::media_mermaid_repeat_doc(&MEDIA_MERMAID[..2], 6),
            root.join("mermaid-repeat.md"),
        ),
        media_case(
            "media-text-12",
            corpus::media_text_doc(12),
            root.join("text.md"),
        ),
    ]
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
    mount_shell_doc_budget(case.fresh_doc(), case.job_bytes, case.resident_bytes, cx)
}

pub fn mount_shell_text(text: &str, cx: &mut BenchAppContext<'_, '_>) -> Entity<Shell> {
    mount_shell_doc(doc(text), cx)
}

pub fn mount_shell_doc(doc: Doc, cx: &mut BenchAppContext<'_, '_>) -> Entity<Shell> {
    mount_shell_doc_budget(doc, None, None, cx)
}

pub fn mount_shell_doc_budget(
    doc: Doc,
    job_bytes: Option<usize>,
    resident_bytes: Option<usize>,
    cx: &mut BenchAppContext<'_, '_>,
) -> Entity<Shell> {
    // `MD_BENCH_RESIDENT=<MiB>` wins over the case so one build can sweep the
    // ceiling: the value that matters is where the content stops fitting, and
    // that is not known before the first run.
    let resident_bytes = std::env::var("MD_BENCH_RESIDENT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|mib| mib << 20)
        .or(resident_bytes);
    md_content::media::set_resident_override(resident_bytes);
    let mut window = cx.add_empty_window();
    let shell = window.update(|window, cx| {
        window.resize(size(px(EDITOR_VIEWPORT.0), px(EDITOR_VIEWPORT.1)));
        if let Some(limit) = job_bytes {
            md_content::media::MediaCoordinator::install_exact(cx, limit);
        }
        window.replace_root(cx, |_window, cx| Shell::new(doc, cx))
    });
    window.update(|window, cx| {
        let focus = shell.read(cx).editor_focus().clone();
        focus.focus(window, cx);
    });
    shell
}
