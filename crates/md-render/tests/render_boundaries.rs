use gpui::TestAppContext;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia, ShapePart};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use md_core::inline::InlineAlign;
use md_layout::assembly::assemble_tree;
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::{FrameContext, FrameRequest, compose, from_assembly};
use md_render::snap::SnapOperator;
use md_render::snapshot::{
    CellPiece, DecorationPiece, Frame, LayoutSnapshot, SnapshotRevs, TextPiece,
};
use md_theme::DocumentTheme;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::rc::Rc;

fn artifact(height: Px) -> md_content::shaper::ShapeArtifact {
    md_content::shaper::ShapeArtifact::plain(Vec::new(), 1, height, 16.0, 20.0)
}

fn snapshot(geometry_revision: u64) -> LayoutSnapshot {
    LayoutSnapshot {
        document_revision: 1,
        layout_revision: 1,
        viewport_revision: 1,
        geometry_revision,
        total_height: 400.0,
        scroll: 0.0,
        viewport: (800.0, 600.0),
        texts: vec![TextPiece {
            box_id: LayoutBoxId::frame(1),
            block: 7,
            kind: BlockKind::Paragraph,
            edit_source: false,
            content_origin_device: (0.0, 0.0),
            content_width: 100.0,
            view_height: 20.0,
            art: Rc::new(artifact(20.0)),
            align: InlineAlign::Start,
        }],
        decorations: vec![DecorationPiece {
            rect_device: (0.0, 0.0, 40.0, 20.0),
            clip_device: None,
            kind: BlockKind::ListItem,
            role: BoxRole::Slot,
            hit_block: 7,
            gutter_dot: Some((0.0, 0.0)),
            gutter_label: None,
            gutter_label_at: None,
            gutter_label_size: 16.0,
            list_nest: 0,
            task: Some(false),
            alert: None,
        }],
        cells: Vec::new(),
        caret_device: None,
        caret_logical_y: Some(12.0),
        selection_device: Vec::new(),
        inline_code_device: Vec::new(),
        search_device: Vec::new(),
        search_active_device: Vec::new(),
        ime_device: Vec::new(),
        spans: BTreeMap::new(),
        content_atoms_painted: 0,
        absent_visible: Vec::new(),
    }
}

#[test]
fn a_snapshot_from_another_frame_answers_nothing() {
    let painted = snapshot(11);
    let stale = painted.geometry_revision + 1;

    assert_eq!(
        md_render::query::a11y_bounds(&painted, painted.geometry_revision).len(),
        1,
        "with matching revisions, a11y must report that one block"
    );
    assert_eq!(
        md_render::query::hit_list_item_slot(&painted, painted.geometry_revision, (10.0, 10.0)),
        Some(7),
        "with matching revisions, a click on the checkbox must hit"
    );

    assert!(
        md_render::query::a11y_bounds(&painted, stale).is_empty(),
        "when revisions differ, a11y must not pass off stale geometry"
    );
    assert_eq!(
        md_render::query::hit_list_item_slot(&painted, stale, (10.0, 10.0)),
        None,
        "when revisions differ, a click must not land on the old position"
    );
}

const FIXTURE: &str = "alpha needle omega\n";
const NEEDLE: &str = "needle";

fn fixture_doc() -> Doc {
    Doc::new(load_markdown(FIXTURE, editor_options()))
}

fn first_block(doc: &Doc) -> md_core::block::BlockId {
    let root = doc.document.root;
    doc.document
        .arena
        .children(root)
        .next()
        .expect("the fixture must have at least one block")
        .index
}

fn test_shaper(window: &gpui::Window, app: &gpui::App, theme: &DocumentTheme) -> GpuiShaper {
    test_shaper_at_scale(window, app, theme, 1.0)
}

fn empty_media() -> ShapeMedia {
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

fn test_shaper_at_scale(
    window: &gpui::Window,
    app: &gpui::App,
    theme: &DocumentTheme,
    scale: f64,
) -> GpuiShaper {
    GpuiShaper::new(window, app, theme, scale, ShapeCache::new(), empty_media())
}

fn test_shaper_with_cache(
    window: &gpui::Window,
    app: &gpui::App,
    theme: &DocumentTheme,
    cache: Rc<ShapeCache>,
) -> GpuiShaper {
    GpuiShaper::new(window, app, theme, 1.0, cache, empty_media())
}

#[gpui::test]
fn scale_two_frame_aligns_caret_to_the_painted_row(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let mut theme = DocumentTheme::one_dark();
        theme.type_scale.body.size_px = 17.0;
        theme.type_scale.body.line_height_em = 1.37;
        let doc = fixture_doc();
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper_at_scale(window, app, &theme, 2.0);
        let snap = SnapOperator::new(2.0);
        let block = first_block(&doc);
        let offset = FIXTURE.find(NEEDLE).expect("needle");
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.3,
                cursor: Cursor { block, offset },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let text = frame
            .texts
            .iter()
            .find(|text| text.block == block)
            .expect("text");
        let (_, row) =
            shaper.position_for_offset(&text.art, offset, text.align, text.content_width);
        let (dy, _) = shaper.caret_ink(
            BlockKind::Paragraph,
            frame.assembly.tree.get(text.box_id).type_slot(),
            &text.art,
            row,
        );
        let expected_y = text.content_origin_device.1 + text.art.row_top(row) + dy;
        let caret = frame.caret_device.expect("caret");
        assert_eq!(caret.1, expected_y);
        assert_eq!((text.content_origin_device.1 * 2.0).fract(), 0.0);
    });
}

#[gpui::test]
fn marked_text_publishes_every_wrapped_row(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let markdown = format!("{}\n", "marked text ".repeat(30));
        let doc = Doc::new(load_markdown(&markdown, editor_options()));
        let block = first_block(&doc);
        let len = doc.text(block).expect("text").len();
        let env = BoxLayoutEnvironment {
            viewport_width: 180.0,
        };
        let theme = DocumentTheme::one_dark();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor { block, offset: len },
                selection: None,
                marked: Some((block, 0..len)),
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let rows = frame
            .texts
            .iter()
            .find(|text| text.block == block)
            .expect("text")
            .art
            .rows;
        assert!(rows > 1, "fixture must wrap");
        assert_eq!(frame.ime_device.len(), rows as usize);
    });
}

#[gpui::test]
fn clipped_leaf_decoration_keeps_full_box_geometry(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let markdown = format!("```\n{}\n```\n", "long code line\n".repeat(40));
        let doc = Doc::new(load_markdown(&markdown, editor_options()));
        let block = first_block(&doc);
        let env = BoxLayoutEnvironment::default();
        let theme = DocumentTheme::one_dark();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 100.0),
                scroll: 100.0,
                cursor: Cursor { block, offset: 0 },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let well = frame
            .decorations
            .iter()
            .find(|piece| piece.kind == BlockKind::CodeBlock)
            .expect("code decoration");
        let clip = well.clip_device.expect("leaf clip");
        assert!(well.rect_device.1 < clip.1);
        assert!(well.rect_device.3 > clip.3);
        assert_eq!(clip.1, 0.0);
        assert_eq!(clip.3, 100.0);
    });
}

#[gpui::test]
fn a_frame_can_find_its_own_text_again(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let (block, revs, frame_revs, hit) = cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let doc = fixture_doc();
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let block = first_block(&doc);

        let tree = md_layout::compose::compose(&doc.document, &theme.layout_theme());
        let assembly = assemble_tree(tree, env, &shaper, &FallbackSolver);
        let revs = SnapshotRevs {
            document: doc.document.revision(),
            layout: 7,
            viewport: 3,
        };
        let frame = from_assembly(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor { block, offset: 0 },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            assembly,
            revs,
        );

        let text = frame
            .texts
            .first()
            .expect("the fixture paragraph must yield a text piece")
            .clone();

        let (x, y) = text.content_origin_device;
        let probe = (x + 2.0, y + text.view_height / 2.0);
        let hit =
            md_render::query::hit_test(&frame, frame.geometry_revision, probe, &shaper, |_| {
                (0.0, 0.0)
            });
        let frame_revs = (
            frame.document_revision,
            frame.layout_revision,
            frame.viewport_revision,
        );
        (block, revs, frame_revs, hit)
    });

    let hit = hit.expect("a click on the laid-out text must resolve to a caret");
    assert_eq!(
        hit.block, block,
        "the round trip must land on the same block: what was painted is what the click comes back to"
    );

    assert_eq!(
        frame_revs,
        (revs.document, revs.layout, revs.viewport),
        "`from_assembly` must copy the caller-supplied revisions into the snapshot"
    );
}

#[gpui::test]
fn list_item_starting_with_table_builds_frame(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let (cell, slot) = cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let doc = Doc::new(load_markdown(
            "- | a | b |\n  | --- | --- |\n  | 1 | 2 |\n",
            editor_options(),
        ));
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let cell = doc
            .text_leaves()
            .into_iter()
            .find(|&block| doc.kind(block) == Some(BlockKind::TableCell))
            .expect("table cell");
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor {
                    block: cell,
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
        let slot = frame
            .decorations
            .iter()
            .find(|piece| piece.kind == BlockKind::ListItem && piece.role == BoxRole::Slot)
            .map(|piece| (piece.hit_block, piece.gutter_label_size));
        (cell, slot)
    });

    let (hit_block, label_size) = slot.expect("list slot decoration");
    assert_eq!(hit_block, cell);
    assert!(label_size > 0.0);
}

#[gpui::test]
fn block_edit_consumers_use_one_coordinate_space(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let mut doc = Doc::new(load_markdown(
            "![needle](https://example.com/a-long-image-name.png)\n",
            editor_options(),
        ));
        let block = doc
            .text_leaves()
            .into_iter()
            .find(|&id| doc.kind(id) == Some(BlockKind::Image))
            .expect("image block");
        let _ = doc.retarget_focus(Cursor { block, offset: 0 });
        assert_eq!(doc.block_edit(), Some(block));

        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: Cursor { block, offset: 0 },
                selection: None,
                marked: None,
                search_query: "needle",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        let pieces: Vec<_> = frame.texts.iter().filter(|t| t.block == block).collect();
        assert_eq!(pieces.len(), 2, "source and preview pieces");
        let source = pieces.iter().find(|t| t.edit_source).expect("source piece");
        let preview = pieces
            .iter()
            .find(|t| !t.edit_source)
            .expect("preview piece");

        assert_eq!(
            frame.search_device.len(),
            1,
            "search must not highlight twice"
        );
        assert_eq!(
            frame.search_device[0].1, source.content_origin_device.1,
            "search offsets map to the editable source artifact"
        );

        let needle_at = "![needle](https://example.com/a-long-image-name.png)"
            .find("needle")
            .expect("needle in source");
        let (start_x, _) =
            shaper.position_for_offset(&source.art, needle_at, source.align, source.content_width);
        let (end_x, _) = shaper.position_for_offset(
            &source.art,
            needle_at + "needle".len(),
            source.align,
            source.content_width,
        );
        assert_eq!(
            frame.search_device[0].0,
            source.content_origin_device.0 + start_x,
            "highlight starts on the matched source glyphs"
        );
        assert_eq!(
            frame.search_device[0].2,
            end_x - start_x,
            "highlight spans exactly the query glyphs"
        );

        let bounds = md_render::query::a11y_bounds(&frame, frame.geometry_revision);
        let image_bounds: Vec<_> = bounds.iter().filter(|(id, _)| *id == block).collect();
        assert_eq!(
            image_bounds.len(),
            1,
            "a11y must not publish the block twice"
        );
        assert_eq!(
            image_bounds[0].1.1, preview.content_origin_device.1,
            "a11y should expose the rendered preview"
        );

        let x = source.content_origin_device.0 + source.content_width * 0.75;
        let y = preview.content_origin_device.1 + preview.view_height * 0.5;
        let source_row = (((y - source.content_origin_device.1) / source.art.row_advance)
            .floor()
            .max(0.0) as u32)
            .min(source.art.rows.max(1) - 1);
        let source_offset = shaper.offset_for_position(
            &source.art,
            (x - source.content_origin_device.0).max(0.0),
            source_row,
            source.align,
            source.content_width,
        );
        let preview_offset = shaper.offset_for_position(
            &preview.art,
            (x - preview.content_origin_device.0).max(0.0),
            0,
            preview.align,
            preview.content_width,
        );
        assert_ne!(
            source_offset, preview_offset,
            "fixture must distinguish spaces"
        );
        let hit =
            md_render::query::hit_test(&frame, frame.geometry_revision, (x, y), &shaper, |_| {
                (0.0, 0.0)
            })
            .expect("preview click should resolve to the editable source");
        assert_eq!(hit.block, block);
        assert_eq!(hit.offset, source_offset);
    });
}

#[gpui::test]
fn wrapped_aligned_cell_selection_starts_at_each_rows_ink(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for separator in [":---:", "---:"] {
            let value = format!("{}x", "abcdefghij ".repeat(24));
            let markdown = format!("| value |\n| {separator} |\n| {value} |\n");
            let doc = Doc::new(load_markdown(&markdown, editor_options()));
            let block = doc
                .text_leaves()
                .into_iter()
                .find(|&id| {
                    doc.kind(id) == Some(BlockKind::TableCell)
                        && doc
                            .text(id)
                            .is_some_and(|text| text.starts_with("abcdefghij"))
                })
                .expect("body cell");
            let len = doc.text(block).expect("cell text").len();
            let env = BoxLayoutEnvironment {
                viewport_width: 240.0,
            };
            let theme = DocumentTheme::one_dark();
            let shaper = test_shaper(window, app, &theme);
            let snap = SnapOperator::new(1.0);
            let frame = compose(
                FrameContext {
                    doc: &doc,
                    env,
                    shaper: &shaper,
                    snap: &snap,
                    theme: &theme,
                },
                &FrameRequest {
                    viewport: (env.viewport_width, 600.0),
                    scroll: 0.0,
                    cursor: Cursor { block, offset: len },
                    selection: Some((Cursor { block, offset: 0 }, Cursor { block, offset: len })),
                    marked: None,
                    search_query: "",
                    search_skip: None,
                },
                &FallbackSolver,
                None,
            );
            let cell = frame
                .cells
                .iter()
                .find(|cell| cell.block == block)
                .expect("cell piece");
            assert!(cell.art.rows > 1, "fixture must wrap: {separator}");
            assert_eq!(frame.selection_device.len(), cell.art.rows as usize);

            let mut saw_shifted_followup = false;
            for (row, rect) in frame.selection_device.iter().enumerate() {
                let expected = cell.content_origin_device.0
                    + shaper.row_content_start(
                        &cell.art,
                        row as u32,
                        cell.align,
                        cell.content_width,
                    );
                assert_eq!(rect.0, expected, "row {row}, separator {separator}");
                if row > 0 && expected > cell.content_origin_device.0 {
                    saw_shifted_followup = true;
                }
            }
            assert!(
                saw_shifted_followup,
                "fixture must contain an aligned follow-up row: {separator}"
            );
        }
    });
}

#[gpui::test]
fn caret_selection_and_search_agree_on_one_word(cx: &mut TestAppContext) {
    let start = FIXTURE
        .find(NEEDLE)
        .expect("the fixture must contain the word");
    let end = start + NEEDLE.len();

    let cx = cx.add_empty_window();
    let (caret, selection, search) = cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let doc = fixture_doc();
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let block = first_block(&doc);
        let at = |offset: usize| Cursor { block, offset };
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor: at(start),
                selection: Some((at(start), at(end))),
                marked: None,
                search_query: NEEDLE,
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        (
            frame.caret_device,
            frame.selection_device.clone(),
            frame.search_device.clone(),
        )
    });

    let caret = caret.expect("a caret inside the word must measure a vertical bar");
    assert_eq!(
        selection.len(),
        1,
        "a selection within one line must be a single band"
    );
    assert_eq!(
        search.len(),
        1,
        "the word must appear exactly once in the fixture"
    );

    assert_eq!(
        caret.0, selection[0].0,
        "the caret must stand on the selection's left edge"
    );
    assert_eq!(
        caret.0, search[0].0,
        "the caret must stand on the match's left edge"
    );

    assert_eq!(
        selection[0].2, search[0].2,
        "the selection and the match must be the same width"
    );

    assert!(
        selection[0].2 > 0.0 && selection[0].3 > 0.0,
        "the selection must have real area, not a stack of zeros"
    );
    assert!(caret.3 > 0.0, "the caret must have real height");
}

#[gpui::test]
fn alert_label_owns_a_band_above_the_first_content_line(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let doc = Doc::new(load_markdown("> [!NOTE]\n> hello\n", editor_options()));
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let block = doc.text_leaves()[0];
        let frame = compose(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
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
        let label = frame
            .decorations
            .iter()
            .find(|d| d.gutter_label.is_some())
            .expect("alert chrome carries the label");
        let (_, ly) = label.gutter_dot.expect("label position");
        let text = frame
            .texts
            .iter()
            .find(|t| t.block == block)
            .expect("first content line");
        let label_row = f64::from(label.gutter_label_size) * 1.75;
        assert!(
            text.content_origin_device.1 >= ly + label_row,
            "the label band ({ly}..{}) must lie wholly above the body text ({})",
            ly + label_row,
            text.content_origin_device.1
        );
    });
}

#[test]
fn snap_operator_uses_the_scale_grid() {
    let snap = SnapOperator::new(2.0);
    assert_eq!(snap.snap(1.24), 1.0);
    assert_eq!(snap.snap(1.25), 1.5);
}

#[test]
fn snap_operator_rejects_invalid_scale_results() {
    let snap = SnapOperator::new(0.0);
    assert!(snap.snap(10.0).is_finite());
    assert!(snap.snap(f64::NAN).is_finite());
}

#[gpui::test]
fn a_dropped_cell_is_booked_as_absent_visible(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let doc = Doc::new(load_markdown(
            "para\n\n| a | b |\n| --- | --- |\n",
            editor_options(),
        ));
        let env = BoxLayoutEnvironment::default();
        let shaper = test_shaper(window, app, &theme);
        let snap = SnapOperator::new(1.0);
        let para = first_block(&doc);
        let cursor = Cursor {
            block: para,
            offset: 0,
        };

        let tree = md_layout::compose::compose(&doc.document, &theme.layout_theme());
        let mut assembly = assemble_tree(tree, env, &shaper, &FallbackSolver);

        let island = assembly
            .geometries
            .values_mut()
            .find(|g| !g.cells.is_empty())
            .expect("the table must solve to a row island with cells");
        let island_cells = island.cells.len();
        let para_box = LayoutBoxId::for_kind(BlockKind::Paragraph, para);
        assert!(
            assembly.tree.nodes().contains_key(&para_box),
            "paragraph box must be a real tree node"
        );
        assert!(
            assembly.tree.ancestor_table(para_box).is_none(),
            "fixture paragraph must not sit under a table"
        );

        island.cells[0].cell_box = para_box;

        let frame = from_assembly(
            FrameContext {
                doc: &doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme: &theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, 600.0),
                scroll: 0.0,
                cursor,
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            assembly,
            SnapshotRevs {
                document: doc.document.revision(),
                layout: 1,
                viewport: 1,
            },
        );
        assert_eq!(
            frame.cells.len(),
            island_cells - 1,
            "the corrupted record must not paint a cell"
        );
        assert!(
            frame.absent_visible.contains(&para_box),
            "the dropped record must be booked, not silently skipped"
        );
    });
}

const CACHE_FIXTURE: &str = "alpha **bold** beta\n\n- one\n- two\n\n| a | b |\n| --- | --- |\n| c | d |\n\n```\nlet x = 1;\n```\n\ntail\n";

const CACHE_INSERTS: &[&str] = &["x", " ", "é", "🙂", "**", "`", "\nword", "line two\n"];

struct CacheRng(u64);

impl CacheRng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn pick(&mut self, len: usize) -> usize {
        (self.next() as usize) % len
    }

    fn one_in(&mut self, denominator: u64) -> bool {
        self.next().is_multiple_of(denominator)
    }
}

fn cache_dose(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn random_edit_caret(doc: &Doc, rng: &mut CacheRng) -> Caret {
    let leaves: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter(|id| !doc.kind(*id).is_some_and(BlockKind::supports_block_edit))
        .collect();
    let block = leaves[rng.pick(leaves.len())];
    let text = doc.caret_text(block).expect("live text leaf");
    let mut boundaries: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(text.len());
    Caret {
        block,
        offset: boundaries[rng.pick(boundaries.len())],
    }
}

fn compose_frame(
    doc: &Doc,
    env: BoxLayoutEnvironment,
    shaper: &GpuiShaper,
    snap: &SnapOperator,
    theme: &DocumentTheme,
) -> Frame {
    compose(
        FrameContext {
            doc,
            env,
            shaper,
            snap,
            theme,
        },
        &FrameRequest {
            viewport: (env.viewport_width, 600.0),
            scroll: 0.0,
            cursor: Cursor {
                block: doc.text_leaves()[0],
                offset: 0,
            },
            selection: None,
            marked: None,
            search_query: "",
            search_skip: None,
        },
        &FallbackSolver,
        None,
    )
}

fn write_art_sig(out: &mut String, art: &md_content::shaper::ShapeArtifact) {
    write!(
        out,
        "rows{} h{:?} fb{:?} ra{:?} mw{:?}",
        art.rows, art.height, art.first_baseline, art.row_advance, art.max_line_width,
    )
    .unwrap();
    for band in &art.bands {
        write!(out, " band({:?},{:?}", band.height, band.text_dy).unwrap();
        for part in &band.parts {
            match part {
                ShapePart::Text {
                    x,
                    byte_start,
                    byte_end,
                    dy,
                    ..
                } => write!(out, " t{x:?}@{byte_start}..{byte_end}+{dy:?}").unwrap(),
                ShapePart::Math {
                    x,
                    width,
                    paint_y,
                    latex,
                    display,
                    em,
                    slot_h,
                    byte_start,
                    byte_end,
                    ..
                } => write!(
                    out,
                    " m{x:?}w{width:?}y{paint_y:?}{latex}d{display}e{em}s{slot_h:?}@{byte_start}..{byte_end}"
                )
                .unwrap(),
                ShapePart::Image {
                    x,
                    width,
                    paint_y,
                    dest,
                    slot_w,
                    slot_h,
                    byte_start,
                    byte_end,
                    ..
                } => write!(
                    out,
                    " i{x:?}w{width:?}y{paint_y:?}{dest}s{slot_w:?}x{slot_h:?}@{byte_start}..{byte_end}"
                )
                .unwrap(),
            }
        }
        write!(out, ")").unwrap();
    }
}

fn frame_signature(frame: &Frame) -> String {
    let mut out = String::new();
    let mut pieces: Vec<&TextPiece> = frame.texts.iter().collect();
    pieces.sort_by(|a, b| {
        a.block
            .cmp(&b.block)
            .then_with(|| {
                a.content_origin_device
                    .0
                    .total_cmp(&b.content_origin_device.0)
            })
            .then_with(|| {
                a.content_origin_device
                    .1
                    .total_cmp(&b.content_origin_device.1)
            })
    });
    for piece in pieces {
        write!(
            &mut out,
            "[b{} {:?} src{} {:?} w{} vh{} at{:?} ",
            piece.block,
            piece.kind,
            piece.edit_source,
            piece.align,
            piece.content_width,
            piece.view_height,
            piece.content_origin_device,
        )
        .unwrap();
        write_art_sig(&mut out, &piece.art);
        write!(&mut out, "]").unwrap();
    }
    let mut cells: Vec<&CellPiece> = frame.cells.iter().collect();
    cells.sort_by(|a, b| {
        a.block
            .cmp(&b.block)
            .then_with(|| {
                a.content_origin_device
                    .0
                    .total_cmp(&b.content_origin_device.0)
            })
            .then_with(|| {
                a.content_origin_device
                    .1
                    .total_cmp(&b.content_origin_device.1)
            })
    });
    for cell in cells {
        write!(
            &mut out,
            "[c{} t{} {} w{} h{:?} at{:?} ",
            cell.block,
            cell.table,
            if cell.header { 'H' } else { 'd' },
            cell.content_width,
            cell.rect_device,
            cell.content_origin_device,
        )
        .unwrap();
        write_art_sig(&mut out, &cell.art);
        write!(&mut out, "]").unwrap();
    }
    let mut rects: Vec<String> = frame
        .decorations
        .iter()
        .map(|piece| format!("{:?}{:?}", piece.kind, piece.rect_device))
        .collect();
    rects.sort();
    for rect in rects {
        write!(&mut out, " d{rect}").unwrap();
    }
    write!(&mut out, " total{:?}", frame.total_height).unwrap();
    out
}

#[gpui::test]
fn random_edits_keep_the_shape_cache_honest(cx: &mut TestAppContext) {
    let cases = cache_dose("MD_TEST_SHAPE_CACHE_CASES", 6);
    let steps = cache_dose("MD_TEST_SHAPE_CACHE_STEPS", 24);
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let env = BoxLayoutEnvironment::default();
        let snap = SnapOperator::new(1.0);
        for case in 0..cases {
            let mut rng = CacheRng::new(0x5eed_0000 + case as u64 * 0x9e37);
            let mut doc = Doc::new(load_markdown(CACHE_FIXTURE, editor_options()));
            let hot = test_shaper_with_cache(window, app, &theme, ShapeCache::new());

            let _ = compose_frame(&doc, env, &hot, &snap, &theme);

            for step in 0..steps {
                let caret = random_edit_caret(&doc, &mut rng);
                let revision = doc.document.revision();
                let command = if rng.one_in(3) {
                    Command::DeleteBackward
                } else {
                    Command::Insert {
                        text: CACHE_INSERTS[rng.pick(CACHE_INSERTS.len())].to_string(),
                    }
                };
                let _ = doc.apply(Sel::collapsed(caret), command);
                if doc.document.revision() == revision {
                    continue;
                }

                let shaped_before = hot.stats().total_shape_calls;
                let first = compose_frame(&doc, env, &hot, &snap, &theme);
                assert!(
                    hot.stats().total_shape_calls > shaped_before,
                    "case {case} step {step}: an edit bumped no shape-cache key"
                );

                let steady_before = hot.stats().total_shape_calls;
                let second = compose_frame(&doc, env, &hot, &snap, &theme);
                assert_eq!(
                    hot.stats().total_shape_calls,
                    steady_before,
                    "case {case} step {step}: a steady re-compose reshaped"
                );

                let cold = test_shaper_with_cache(window, app, &theme, ShapeCache::new());
                let cold_frame = compose_frame(&doc, env, &cold, &snap, &theme);
                assert_eq!(
                    frame_signature(&first),
                    frame_signature(&cold_frame),
                    "case {case} step {step}: the shared cache served a stale artifact"
                );
                assert_eq!(
                    frame_signature(&second),
                    frame_signature(&cold_frame),
                    "case {case} step {step}: the steady frame drifted from cold"
                );
            }
        }
    });
}
