use super::support::{
    TABLE_2X3, TABLE_3ROW, draw_editor, editor_with_doc, focus_editor, place_table_caret,
    table_leaf,
};
use gpui::TestAppContext;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::{Cursor, Doc};
use md_core::document::TableOp;

fn synthetic_table_hits(doc: &Doc) -> Vec<crate::view::table_cols::CellHit> {
    doc.text_leaves()
        .into_iter()
        .filter_map(|block| {
            let loc = doc.table_loc(block)?;
            Some((
                block,
                (loc.col as Px * 100.0, loc.row as Px * 30.0, 100.0, 30.0),
            ))
        })
        .collect()
}

#[gpui::test]
fn table_grip_hover_requires_caret_in_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let hits = synthetic_table_hits(&view.state.doc);
            view.update_table_grip_hover(&hits, (50.0, 15.0), cx);
            let hover = view.table_ui.grip_hover.expect("hover");
            assert_eq!(hover.row, Some(0));
            assert_eq!(hover.col, Some(0));
            view.update_table_grip_hover(&hits, (150.0, 15.0), cx);
            let hover = view.table_ui.grip_hover.expect("first row");
            assert_eq!(hover.row, None);
            assert_eq!(hover.col, Some(1));
            view.update_table_grip_hover(&hits, (50.0, 45.0), cx);
            let hover = view.table_ui.grip_hover.expect("first col");
            assert_eq!(hover.row, Some(1));
            assert_eq!(hover.col, None);
            view.update_table_grip_hover(&hits, (150.0, 45.0), cx);
            assert!(view.table_ui.grip_hover.is_none());
            view.update_table_grip_hover(&hits, (500.0, 500.0), cx);
            assert!(view.table_ui.grip_hover.is_none());
        });
    });
    place_table_caret(&editor, cx, "after", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let hits = synthetic_table_hits(&view.state.doc);
            view.update_table_grip_hover(&hits, (50.0, 15.0), cx);
            assert!(view.table_ui.grip_hover.is_none());
        });
    });
}

#[gpui::test]
fn table_col_grip_sits_on_top_border(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let hits = synthetic_table_hits(&view.state.doc);
            let hit = crate::view::table_reorder::hit_grip(
                &view.state.doc,
                &hits,
                view.state.cursor.block,
                (50.0, 0.0),
            )
            .expect("col grip on top border");
            assert!(matches!(
                hit.axis,
                crate::view::table_reorder::ReorderAxis::Col
            ));
            assert_eq!(hit.from, 0);
            assert!(
                crate::view::table_reorder::hit_grip(
                    &view.state.doc,
                    &hits,
                    view.state.cursor.block,
                    (50.0, -25.0),
                )
                .is_none()
            );
        });
    });
}

#[gpui::test]
fn table_toolbar_hover_shows_caret_grips(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.set_table_toolbar_hover(true, cx);
            let hover = view.table_ui.grip_hover.expect("grips");
            assert_eq!(hover.row, Some(1));
            assert_eq!(hover.col, None);
            view.set_table_menu_op_hover(Some(TableOp::MoveColumnLeft), cx);
            assert!(view.table_ui.grip_hover.is_none());
            view.set_table_menu_op_hover(Some(TableOp::MoveRowDown), cx);
            let hover = view.table_ui.grip_hover.expect("row");
            assert_eq!(hover.row, Some(1));
            assert_eq!(hover.col, None);
            view.set_table_toolbar_hover(false, cx);
            view.set_table_menu_op_hover(None, cx);
            assert!(view.table_ui.grip_hover.is_none());
            view.set_table_toolbar_hover(true, cx);
            assert!(view.table_ui.grip_hover.is_some());
            view.set_table_more_open(true, cx);
            assert!(view.table_ui.grip_hover.is_none());
            view.set_table_menu_op_hover(Some(TableOp::InsertRowBelow), cx);
            assert!(view.table_ui.grip_hover.is_none());
            view.update_table_grip_hover(&synthetic_table_hits(&view.state.doc), (50.0, 15.0), cx);
            assert!(view.table_ui.grip_hover.is_none());
        });
    });
}

#[gpui::test]
fn table_toolbar_more_move_column_left(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.table_ui.more_open = true;
            view.apply_table_toolbar(TableOp::MoveColumnLeft, window, cx);
            assert!(!view.table_ui.more_open);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(md.contains("| a | c | b |"), "{md}");
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        assert_eq!(loc.col, 1);
    });
}

#[gpui::test]
fn table_reorder_row_skips_middle_in_one_undo(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "e", 0);
    draw_editor(&editor, cx);
    let md_before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Row,
                    from: 2,
                    cell: view.state.cursor.block,
                },
                (50.0, 75.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (50.0, 5.0), cx);
            view.end_table_reorder(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(md.starts_with("| e | f |"), "{md}");
        assert!(md.contains("| a | b |"), "{md}");
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| view.undo());
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.document.to_markdown(), md_before);
    });
}

#[gpui::test]
fn table_reorder_header_row_is_allowed(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Row,
                    from: 0,
                    cell: view.state.cursor.block,
                },
                (50.0, 10.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (50.0, 80.0), cx);
            view.end_table_reorder(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(md.starts_with("| c | d |"), "{md}");
        assert!(md.contains("| a | b |"), "{md}");
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        assert_eq!(loc.row, 2);
    });
}

#[gpui::test]
fn table_reorder_column_permutes_session_widths(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            view.table_ui
                .col_widths
                .insert(loc.table, vec![100.0, 80.0, 60.0]);
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Col,
                    from: 2,
                    cell: view.state.cursor.block,
                },
                (250.0, 15.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (10.0, 15.0), cx);
            view.end_table_reorder(window, cx);
            let tracks = view.table_ui.col_widths.get(&loc.table).expect("tracks");
            assert!((tracks[0] - 60.0).abs() < 1e-6);
            assert!((tracks[1] - 100.0).abs() < 1e-6);
            assert!((tracks[2] - 80.0).abs() < 1e-6);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(md.contains("| c | a | b |"), "{md}");
    });
}

#[gpui::test]
fn table_reorder_column_drops_on_entered_column(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Col,
                    from: 2,
                    cell: view.state.cursor.block,
                },
                (250.0, 15.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (110.0, 15.0), cx);
            assert_eq!(view.table_ui.reorder.expect("drag").dest, 1);
            view.end_table_reorder(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(md.contains("| a | c | b |"), "{md}");
    });
}

#[gpui::test]
fn table_col_seam_hit_does_not_start_reorder(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let hits = synthetic_table_hits(&view.state.doc);
            let seam = (100.0, 20.0);
            assert!(
                crate::view::table_reorder::hit_grip(
                    &view.state.doc,
                    &hits,
                    view.state.cursor.block,
                    seam,
                )
                .is_none()
            );
            assert!(crate::view::table_cols::hit_col_seam(&view.state.doc, &hits, seam).is_some());
            let grip = (-17.0, 15.0);
            let hit = crate::view::table_reorder::hit_grip(
                &view.state.doc,
                &hits,
                view.state.cursor.block,
                grip,
            )
            .expect("row grip");
            assert!(matches!(
                hit.axis,
                crate::view::table_reorder::ReorderAxis::Row
            ));
            assert_eq!(hit.from, 0);
        });
    });
}

#[gpui::test]
fn table_reorder_targets_the_dragged_row_not_the_stale_selection(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let head = view.state.cursor.block;
            view.place_cursor(
                Cursor {
                    block: head,
                    offset: 1,
                },
                crate::view::CursorMotion::Extend,
            );
            assert!(
                view.state.selection.is_some(),
                "precondition: a selection is in place"
            );
        });
    });
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let cell = table_leaf(&view.state.doc, "e");
            let loc = view.state.doc.table_loc(cell).expect("loc");
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Row,
                    from: 2,
                    cell,
                },
                (50.0, 75.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (50.0, 5.0), cx);
            view.end_table_reorder(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert!(
            md.starts_with("| e | f |"),
            "the e row was dragged, so it should have moved up: {md}"
        );
        assert!(
            view.state.selection.is_none(),
            "the drag should leave only the caret, with the selection cleared"
        );
    });
}

#[gpui::test]
fn table_reorder_escape_cancels_without_edit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "e", 0);
    draw_editor(&editor, cx);
    let md_before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            let hits = synthetic_table_hits(&view.state.doc);
            view.begin_table_reorder(
                crate::view::table_reorder::GripHit {
                    table: loc.table,
                    axis: crate::view::table_reorder::ReorderAxis::Row,
                    from: 2,
                    cell: view.state.cursor.block,
                },
                (50.0, 75.0),
                &hits,
                cx,
            );
            view.table_reorder_to(&hits, (50.0, 5.0), cx);
            assert!(view.table_ui.reorder.is_some());
        });
    });
    cx.simulate_keystrokes("escape");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.table_ui.reorder.is_none());
        assert_eq!(view.state.doc.document.to_markdown(), md_before);
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
    });
}
