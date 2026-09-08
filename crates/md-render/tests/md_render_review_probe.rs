use gpui::TestAppContext;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{Caret, Command, Sel, apply, editor_options, load_markdown};
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{FrameContext, FrameRequest, compose, from_assembly};
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_render::search::{SearchMatch, SearchScan, scan_document};
use md_render::snap::SnapOperator;
use md_render::snapshot::SnapshotRevs;
use md_theme::DocumentTheme;
use std::rc::Rc;

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
            image_sizes: Rc::new(Default::default()),
            image_failed: Rc::new(Default::default()),
            image_gen: 0,
            link_dests: Rc::new(Default::default()),
            link_raw: Rc::new(Default::default()),
            block_image_dest: Rc::new(Default::default()),
            block_code_lang: Rc::new(Default::default()),
        },
    )
}

#[gpui::test]
fn r1_revealed_markup_preserves_search_highlight(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let mut doc = Doc::new(load_markdown("a**b**c\n", editor_options()));
        let block = doc.text_leaves()[0];
        let _ = doc.retarget_focus(Cursor { block, offset: 1 });
        assert_eq!(doc.text(block), Some("a**b**c"));
        assert_eq!(
            scan_document(&doc, "abc"),
            SearchScan::Hits(vec![SearchMatch {
                block,
                start: 0,
                end: 3
            }])
        );
        let range = 0..3;
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 300.0),
                scroll: 0.0,
                cursor: Cursor {
                    block,
                    offset: doc.visual_range(block, range.clone()).end,
                },
                selection: None,
                marked: None,
                search_query: "abc",
                search_skip: Some(SearchMatch {
                    block,
                    start: range.start,
                    end: range.end,
                }),
            },
            &FallbackSolver,
            None,
        );
        assert!(!frame.texts.is_empty());
        assert!(
            !frame.search_active_device.is_empty(),
            "the collapsed match survives focus: active={}, ordinary={}",
            frame.search_active_device.len(),
            frame.search_device.len()
        );
    });
}

#[gpui::test]
fn r2_select_all_highlights_visible_text_with_a_deferred_endpoint(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let markdown = (0..200)
            .map(|i| format!("paragraph {i}\n\n"))
            .collect::<String>();
        let doc = Doc::new(load_markdown(&markdown, editor_options()));
        let leaves = doc.text_leaves();
        let start = Cursor {
            block: leaves[0],
            offset: 0,
        };
        let last = *leaves.last().unwrap();
        let end = Cursor {
            block: last,
            offset: doc.text(last).unwrap().len(),
        };
        let mut engine = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
            0.0,
            120.0,
        );
        engine.materialize_pin_block(end.block, &shaper, &FallbackSolver);
        let (assembly, published) = engine.assemble_with_doc(
            &doc.document,
            ScrollAnchor::top(),
            120.0,
            &shaper,
            &FallbackSolver,
        );
        assert!(
            !assembly
                .tree
                .nodes()
                .contains_key(&LayoutBoxId::frame(last))
        );
        let frame = from_assembly(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 120.0),
                scroll: published.resolved_top,
                cursor: end,
                selection: Some((start, end)),
                marked: None,
                search_query: "",
                search_skip: None,
            },
            assembly,
            SnapshotRevs::default(),
        );
        assert!(!frame.texts.is_empty());
        assert!(
            !frame.selection_device.is_empty(),
            "select-all has {} visible text pieces but no highlight",
            frame.texts.len()
        );
    });
}

#[gpui::test]
fn r3_list_gutter_growth_reflows_existing_table_rows(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let mut doc = load_markdown(
            "998. first\n\n     | a | b |\n     | --- | --- |\n     | c | d |\n\n999. tail\n",
            editor_options(),
        );
        let _ = doc.take_changes();
        let tail = doc
            .text_leaves()
            .into_iter()
            .find(|&id| doc.text_of(id) == Some("tail"))
            .unwrap();
        let list = doc
            .arena
            .children(doc.root)
            .find(|&id| doc.arena.get(id).unwrap().kind == BlockKind::List)
            .unwrap();
        assert_eq!(doc.arena.children(list).count(), 2);
        let mut hot = IncrementalEngine::new(
            &doc,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        let (before, _) =
            hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let row = *before
            .geometries
            .iter()
            .find(|(_, g)| !g.cells.is_empty())
            .unwrap()
            .0;
        let old_width = before.geometries[&row].cells[0].width;
        let _ = apply(
            &mut doc,
            Sel::collapsed(Caret {
                block: tail,
                offset: 4,
            }),
            Command::Break,
        );
        let changes = doc.take_changes();
        assert_eq!(doc.arena.children(list).count(), 3);
        let settlement = hot.apply_changes(&doc, &changes);
        assert!(!settlement.structural_full_clear);
        let (after, _) =
            hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let mut cold = IncrementalEngine::new(
            &doc,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        let (rebuilt, _) =
            cold.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let list_box = LayoutBoxId::frame(list.index);
        let old_gutter = before.tree.style(list_box).padding.left;
        let new_gutter = after.tree.style(list_box).padding.left;
        assert!(
            new_gutter > old_gutter,
            "gutter did not grow: {old_gutter} -> {new_gutter}"
        );
        let actual = after.geometries[&row].cells[0].width;
        let expected = rebuilt.geometries[&row].cells[0].width;
        assert_ne!(
            old_width, expected,
            "fixture must change the available column width"
        );
        assert_eq!(
            actual, expected,
            "gutter {old_gutter} -> {new_gutter}; old width={old_width}; changes={:?}",
            changes.changes
        );
    });
}

#[gpui::test]
fn r4_list_looseness_refreshes_existing_flow_gaps(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let mut doc = load_markdown("- a\n- b\n- c\n- d\n", editor_options());
        let _ = doc.take_changes();
        let leaves = doc.text_leaves();
        let b = leaves[1];
        let d = leaves[3];
        let list = doc
            .arena
            .children(doc.root)
            .find(|&id| doc.arena.get(id).unwrap().kind == BlockKind::List)
            .unwrap();
        assert!(!doc.extra(list).list_loose());
        let mut hot = IncrementalEngine::new(
            &doc,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let before = hot.content_top(b).unwrap();
        let _ = apply(
            &mut doc,
            Sel::collapsed(Caret {
                block: d,
                offset: 0,
            }),
            Command::Indent,
        );
        let changes = doc.take_changes();
        assert!(doc.extra(list).list_loose());
        hot.apply_changes(&doc, &changes);
        hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let mut cold = IncrementalEngine::new(
            &doc,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        cold.assemble_incremental(ScrollAnchor::top(), 2000.0, &shaper, &FallbackSolver);
        let actual = hot.content_top(b).unwrap();
        let expected = cold.content_top(b).unwrap();
        assert_ne!(before, expected, "tight-to-loose spacing must move b");
        assert_eq!(
            actual, expected,
            "before={before}; changes={:?}",
            changes.changes
        );
    });
}
