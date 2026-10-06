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
            "998. first\n\n     | a | b |\n     | --- | --- |\n     | c | d |\n999. tail\n",
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
        let mut doc = load_markdown("- a\n- b\n- c\n  > q\n- d\n", editor_options());
        let _ = doc.take_changes();
        let leaves = doc.text_leaves();
        let b = leaves[1];
        let d = leaves[4];
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
            Command::DeleteBackward,
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

#[gpui::test]
fn r5_a_blank_line_between_two_paragraphs_costs_exactly_one_line(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let doc = Doc::new(load_markdown("a\n\n\nb\n", editor_options()));
        let first = doc.first_text_leaf().unwrap();
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 2000.0),
                scroll: 0.0,
                cursor: Cursor {
                    block: first,
                    offset: 0,
                },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let pieces: Vec<_> = frame.snapshot.texts.iter().collect();
        assert_eq!(pieces.len(), 3, "a, the blank paragraph, b");
        let advance = pieces[0].art.row_advance;
        assert_eq!(pieces[1].art.row_advance, advance);
        let top_a = pieces[0].content_origin_device.1;
        let top_b = pieces[2].content_origin_device.1;
        assert_eq!(
            top_b - top_a,
            2.0 * advance,
            "one blank line costs one line and nothing else, or the caret cannot reach it"
        );
    });
}

#[gpui::test]
fn r6_a_soft_break_at_the_end_of_a_paragraph_lands_on_the_last_painted_line(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let mut doc = Doc::new(load_markdown("132\n", editor_options()));
        let first = doc.first_text_leaf().unwrap();
        let mut at = Cursor {
            block: first,
            offset: 3,
        };
        for (pressed, lines, saved) in [
            (1usize, 1usize, "132"),
            (2, 1, "132\n\n"),
            (3, 2, "132\n\n\n"),
        ] {
            at = doc.apply(Sel::collapsed(at), Command::SoftBreak);
            let frame = compose(
                FrameContext {
                    doc: &doc,
                    env,
                    shaper: &shaper,
                    snap: &snap,
                    theme: &theme,
                },
                &FrameRequest {
                    viewport: (env.viewport_width, 2000.0),
                    scroll: 0.0,
                    cursor: at,
                    selection: None,
                    marked: None,
                    search_query: "",
                    search_skip: None,
                },
                &FallbackSolver,
                None,
            );
            let pieces: Vec<_> = frame.snapshot.texts.iter().collect();
            let advance = pieces[0].art.row_advance;
            let first_top = pieces[0].content_origin_device.1;
            let last = pieces.last().unwrap();
            let last_row_top =
                last.content_origin_device.1 + last.art.rows.saturating_sub(1) as f64 * advance;
            assert_eq!(
                last_row_top,
                first_top + lines as f64 * advance,
                "after {pressed} presses the last painted line must sit {lines} rows below the first"
            );
            assert_eq!(
                at.block, last.block,
                "press {pressed} must leave the caret on the last line"
            );
            let caret_y = frame
                .snapshot
                .caret_device
                .expect("the caret must be painted")
                .1;
            assert_eq!(
                ((caret_y - first_top) / advance).round(),
                lines as f64,
                "after {pressed} presses the caret must be painted on the last painted line"
            );
            assert_eq!(
                doc.document.to_markdown(),
                saved,
                "after {pressed} presses the file must hold the paragraphs the presses asked for"
            );
        }
    });
}

#[gpui::test]
fn r7_a_soft_break_at_a_seam_starts_a_paragraph(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let live_shaper = shaper(window, app, &theme);
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let mut doc = Doc::new(load_markdown("a\nb\n", editor_options()));
        let _ = doc.take_changes();
        let first = doc.first_text_leaf().unwrap();
        let mut hot = IncrementalEngine::new(
            &doc.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &live_shaper, &FallbackSolver);
        let at = doc.apply(
            Sel::collapsed(Cursor {
                block: first,
                offset: 2,
            }),
            Command::SoftBreak,
        );
        let changes = doc.take_changes();
        hot.apply_changes(&doc.document, &changes);
        hot.assemble_incremental(ScrollAnchor::top(), 2000.0, &live_shaper, &FallbackSolver);

        assert_eq!(at.offset, 0, "the caret lands at the start of the tail");
        assert_eq!(
            doc.text_leaves().len(),
            2,
            "the seam becomes a paragraph break, not a third block"
        );
        assert_eq!(doc.text(first).unwrap(), "a");
        assert_eq!(doc.text(at.block).unwrap(), "b");
        let saved = doc.document.to_markdown();
        assert_eq!(saved, "a\n\nb");

        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &live_shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 2000.0),
                scroll: 0.0,
                cursor: at,
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let pieces: Vec<_> = frame.snapshot.texts.iter().collect();
        assert_eq!(pieces.len(), 2, "one painted piece per paragraph");
        for (i, piece) in pieces.iter().enumerate() {
            assert_eq!(piece.art.rows, 1, "piece {i} must be one line");
        }
        let advance = pieces[0].art.row_advance;
        let caret_y = frame
            .snapshot
            .caret_device
            .expect("the caret must be painted")
            .1;
        assert_eq!(
            ((caret_y - pieces[1].content_origin_device.1) / advance).round(),
            0.0,
            "the caret must sit on the first line of the tail"
        );

        let reloaded = Doc::new(load_markdown(&saved, editor_options()));
        assert_eq!(reloaded.text_leaves().len(), 2);
        let reloaded_leaf = reloaded.text_leaves()[1];
        assert_eq!(reloaded.text(reloaded_leaf).unwrap(), "b");
        let cold_shaper = shaper(window, app, &theme);
        let mut cold = IncrementalEngine::new(
            &reloaded.document,
            env,
            Estimator::from_theme(&theme),
            theme.layout_theme(),
        );
        cold.assemble_incremental(ScrollAnchor::top(), 2000.0, &cold_shaper, &FallbackSolver);
        let cold_frame = compose(
            FrameContext {
                doc: &reloaded,
                env,
                shaper: &cold_shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 2000.0),
                scroll: 0.0,
                cursor: Cursor {
                    block: reloaded_leaf,
                    offset: 0,
                },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let cold_pieces: Vec<_> = cold_frame.snapshot.texts.iter().collect();
        assert_eq!(
            cold_pieces.len(),
            pieces.len(),
            "the saved file must paint one piece per paragraph"
        );
        for (i, (live, saved_piece)) in pieces.iter().zip(cold_pieces.iter()).enumerate() {
            assert_eq!(
                live.content_origin_device.1, saved_piece.content_origin_device.1,
                "piece {i} must be spaced like the saved file"
            );
        }
        assert_eq!(
            hot.total_height(),
            cold.total_height(),
            "the live paragraphs must be spaced like the saved file"
        );
    });
}

#[gpui::test]
fn r8_a_soft_break_and_a_hard_break_are_one_shape_to_the_renderer(cx: &mut TestAppContext) {
    struct BreakFact {
        spelling: &'static str,
        trailing: bool,
        source: &'static str,
        saved: String,
        display: String,
        rows: usize,
        caret_rows: Vec<usize>,
    }

    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        let spellings: [(&str, &str); 3] = [
            ("soft", "a\nb\n"),
            ("hard-backslash", "a\\\nb\n"),
            ("hard-two-spaces", "a  \nb\n"),
        ];
        let mut facts: Vec<BreakFact> = Vec::new();
        for (spelling, source) in spellings {
            for trailing in [false, true] {
                let mut doc = Doc::new(load_markdown(source, editor_options()));
                if trailing {
                    doc.enable_trailing_blank();
                }
                let leaf = doc.first_text_leaf().expect("one prose leaf");
                let display = doc.text(leaf).expect("display text").to_string();
                let saved = doc.document.to_markdown();
                let shaper = shaper(window, app, &theme);
                let mut caret_rows = Vec::new();
                let mut rows = 0usize;
                for offset in 0..=display.len() {
                    let frame = compose(
                        FrameContext {
                            doc: &doc,
                            env,
                            shaper: &shaper,
                            snap: &snap,
                            theme: &theme,
                        },
                        &FrameRequest {
                            viewport: (env.viewport_width, 2000.0),
                            scroll: 0.0,
                            cursor: Cursor {
                                block: leaf,
                                offset,
                            },
                            selection: None,
                            marked: None,
                            search_query: "",
                            search_skip: None,
                        },
                        &FallbackSolver,
                        None,
                    );
                    let pieces: Vec<_> = frame.snapshot.texts.iter().collect();
                    rows = pieces[0].art.rows as usize;
                    let advance = pieces[0].art.row_advance;
                    let first_top = pieces[0].content_origin_device.1;
                    let caret_y = frame
                        .snapshot
                        .caret_device
                        .expect("the caret must be painted")
                        .1;
                    caret_rows.push(((caret_y - first_top) / advance).round() as usize);
                }
                facts.push(BreakFact {
                    spelling,
                    trailing,
                    source,
                    saved,
                    display,
                    rows,
                    caret_rows,
                });
            }
        }

        for fact in &facts {
            println!(
                "{:<16} trailing={:<5} source={:?} saved={:?} display={:?} rows={} caret_rows={:?}",
                fact.spelling,
                fact.trailing,
                fact.source,
                fact.saved,
                fact.display,
                fact.rows,
                fact.caret_rows
            );
        }

        for trailing in [false, true] {
            let group: Vec<&BreakFact> = facts.iter().filter(|f| f.trailing == trailing).collect();
            let first = group[0];
            assert_eq!(first.display, "a\nb", "the fixture must hold one break");
            assert_eq!(first.rows, 2, "the fixture must paint two rows");
            assert_eq!(
                first.caret_rows,
                vec![0, 0, 1, 1],
                "display offsets 0 and 1 sit on the first row, 2 and 3 on the second"
            );
            for fact in &group[1..] {
                assert_eq!(
                    fact.display, first.display,
                    "{} and {} must bind to the same display text",
                    fact.spelling, first.spelling
                );
                assert_eq!(
                    fact.rows, first.rows,
                    "{} and {} must paint the same number of rows",
                    fact.spelling, first.spelling
                );
                assert_eq!(
                    fact.caret_rows, first.caret_rows,
                    "{} and {} must place the caret on the same row for the same display offset",
                    fact.spelling, first.spelling
                );
            }
        }
    });
}
