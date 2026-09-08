use gpui::TestAppContext;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_core::inline::InlineRun;
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{FrameContext, FrameRequest, from_assembly};
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_render::snap::SnapOperator;
use md_render::snapshot::SnapshotRevs;
use md_theme::DocumentTheme;
use std::collections::HashMap;
use std::rc::Rc;

#[test]
fn resize_columns_after_offscreen_rows_released() {
    let source = format!("| a | b |\n| --- | --- |\n{}", "| x | y |\n".repeat(1000));
    let doc = Doc::new(load_markdown(&source, editor_options()));
    let table = doc
        .document
        .arena
        .children(doc.document.root)
        .find(|id| doc.document.kind(id.index) == Some(BlockKind::Table))
        .expect("table")
        .index;
    let theme = DocumentTheme::one_dark();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::with_window(
        &doc.document,
        env,
        Estimator::from_theme(&theme),
        theme.layout_theme(),
        0.0,
        300.0,
    );
    let _ = engine.assemble_with_doc(
        &doc.document,
        ScrollAnchor::top(),
        300.0,
        &FlatMeasure,
        &FallbackSolver,
    );
    engine.set_table_col_tracks(&HashMap::from([(table, vec![180.0, 220.0])]));

    let total = engine.total_height();
    let bottom = engine.anchor_at_y(total - 1.0, &FlatMeasure, &FallbackSolver);
    let _ = engine.assemble_with_doc(&doc.document, bottom, 300.0, &FlatMeasure, &FallbackSolver);
}

struct FlatMeasure;

impl TextMeasure for FlatMeasure {
    fn begin_island(&self) {}
    fn measure(
        &self,
        _text: &str,
        _runs: &[InlineRun],
        width: f64,
        _kind: MeasureKind,
        _block: BlockKind,
        _ident: ShapeIdentity,
    ) -> MeasureResult {
        MeasureResult {
            width,
            height: 20.0,
            rows: 1,
            first_baseline: 16.0,
        }
    }
}

fn shaper(window: &gpui::Window, app: &gpui::App, theme: &DocumentTheme) -> GpuiShaper {
    GpuiShaper::new(
        window,
        app,
        theme,
        1.0,
        ShapeCache::new(),
        ShapeMedia {
            mermaid_fitted: Rc::new(Default::default()),
            math_metrics: Rc::new(Default::default()),
            math_gen: 0,
            image_sizes: Rc::new(HashMap::new()),
            image_failed: Rc::new(Default::default()),
            image_gen: 0,
            link_dests: Rc::new(HashMap::new()),
            link_raw: Rc::new(Default::default()),
            block_image_dest: Rc::new(Default::default()),
            block_code_lang: Rc::new(Default::default()),
        },
    )
}

#[gpui::test]
fn selection_after_front_split_with_deferred_end(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let source = (0..300)
            .map(|i| format!("paragraph {i}\n\n"))
            .collect::<String>();
        let mut doc = Doc::new(load_markdown(&source, editor_options()));
        let first = doc.text_leaves()[0];
        let (_, inserted) = doc.document.split_leaf(first, 0);
        let last = *doc.text_leaves().last().unwrap();
        assert!(inserted > last, "fixture must have non-document-order IDs");
        let start = Cursor { block: inserted, offset: 0 };
        let end = Cursor { block: last, offset: doc.text(last).unwrap().len() };
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let shaper = shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let mut engine = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
            0.0,
            300.0,
        );
        let (assembly, published) = engine.assemble_with_doc(
            &doc.document,
            ScrollAnchor::top(),
            300.0,
            &shaper,
            &FallbackSolver,
        );
        assert!(
            !assembly.tree.nodes().contains_key(&LayoutBoxId::frame(last)),
            "fixture end must be deferred"
        );
        let frame = from_assembly(
            FrameContext { doc: &doc, env, shaper: &shaper, snap: &snap, theme: &theme },
            &FrameRequest {
                viewport: (env.viewport_width, 300.0),
                scroll: published.resolved_top,
                cursor: start,
                selection: Some((start, end)),
                marked: None,
                search_query: "",
                search_skip: None,
            },
            assembly,
            SnapshotRevs::default(),
        );
        assert!(
            !frame.texts.is_empty(),
            "fixture must render visible text fragments"
        );
        assert!(
            !frame.selection_device.is_empty(),
            "selecting from new front paragraph through deferred last paragraph must highlight visible text"
        );

        let (assembly, published) = engine.assemble_with_doc(
            &doc.document,
            ScrollAnchor::top(),
            300.0,
            &shaper,
            &FallbackSolver,
        );
        let flipped = from_assembly(
            FrameContext { doc: &doc, env, shaper: &shaper, snap: &snap, theme: &theme },
            &FrameRequest {
                viewport: (env.viewport_width, 300.0),
                scroll: published.resolved_top,
                cursor: start,
                selection: Some((end, start)),
                marked: None,
                search_query: "",
                search_skip: None,
            },
            assembly,
            SnapshotRevs::default(),
        );
        assert!(
            !flipped.selection_device.is_empty(),
            "reversed selection direction must highlight the same visible text"
        );
        assert_eq!(
            frame.selection_device.len(),
            flipped.selection_device.len(),
            "selection rectangles must be direction-independent"
        );
    });
}

#[test]
fn deleting_a_deferred_leaf_updates_spine_height() {
    let source = (0..300)
        .map(|i| format!("paragraph {i}\n\n"))
        .collect::<String>();
    let mut doc = Doc::new(load_markdown(&source, editor_options()));
    let last = *doc.text_leaves().last().unwrap();
    let theme = DocumentTheme::one_dark();
    let env = BoxLayoutEnvironment::default();
    let make = |doc: &Doc| {
        IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
            0.0,
            300.0,
        )
    };
    let mut engine = make(&doc);
    let (changes, _, _) = doc
        .document
        .merge_into_prev(last)
        .expect("merge two ordinary cold paragraphs");
    let settlement = engine.apply_changes(&doc.document, &changes);
    let cold = make(&doc);
    println!(
        "updated_height={} fresh_height={} full_clear={}",
        engine.total_height(),
        cold.total_height(),
        settlement.structural_full_clear,
    );
    assert_eq!(
        engine.total_height(),
        cold.total_height(),
        "deleted deferred leaf must no longer contribute height",
    );
}

#[test]
fn deleting_two_deferred_leaves_accumulates_no_height() {
    let source = (0..300)
        .map(|i| format!("paragraph {i}\n\n"))
        .collect::<String>();
    let mut doc = Doc::new(load_markdown(&source, editor_options()));
    let theme = DocumentTheme::one_dark();
    let env = BoxLayoutEnvironment::default();
    let make = |doc: &Doc| {
        IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
            0.0,
            300.0,
        )
    };
    let mut engine = make(&doc);
    for _ in 0..2 {
        let last = *doc.text_leaves().last().unwrap();
        let (changes, _, _) = doc
            .document
            .merge_into_prev(last)
            .expect("merge two ordinary cold paragraphs");
        let _ = engine.apply_changes(&doc.document, &changes);
    }
    let cold = make(&doc);
    assert_eq!(
        engine.total_height(),
        cold.total_height(),
        "two deferred deletions must both leave the spine",
    );
}
