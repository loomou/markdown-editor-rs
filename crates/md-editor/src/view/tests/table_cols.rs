use super::support::{
    TABLE_2X3, draw_editor, editor_with_doc, focus_editor, place_table_caret, table_leaf, test_doc,
};
use gpui::TestAppContext;
use md_core::document::TableOp;

#[gpui::test]
fn table_col_resize_changes_width_not_markdown(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let md_before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            assert_eq!(loc.cols, 3);
            view.begin_col_resize(
                crate::view::table_cols::ColSeamHit {
                    table: loc.table,
                    col: 0,
                    start_x: 200.0,
                    tracks: vec![200.0, 200.0, 200.0],
                },
                cx,
            );
            view.col_resize_to(240.0, cx);
            view.end_col_resize(cx);
            let tracks = view.table_ui.col_widths.get(&loc.table).expect("tracks");
            assert!((tracks[0] - 240.0).abs() < 1e-6);
            assert!((tracks[1] - 160.0).abs() < 1e-6);
            assert!((tracks[2] - 200.0).abs() < 1e-6);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let md = view.state.doc.document.to_markdown();
        assert_eq!(md, md_before);
        assert!(!md.contains("width"));
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        let tracks = view.table_ui.col_widths.get(&loc.table).expect("tracks");
        assert!((tracks[0] - 240.0).abs() < 1e-6);
    });
}

#[gpui::test]
fn table_col_resize_clears_on_replace_document(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            view.table_ui
                .col_widths
                .insert(loc.table, vec![120.0, 80.0, 80.0]);
            view.replace_document(test_doc(TABLE_2X3), cx);
            assert!(view.table_ui.col_widths.is_empty());
            assert!(view.table_ui.col_resize.is_none());
        });
    });
    draw_editor(&editor, cx);
}

#[gpui::test]
fn table_col_resize_stops_at_min_width(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            view.begin_col_resize(
                crate::view::table_cols::ColSeamHit {
                    table: loc.table,
                    col: 0,
                    start_x: 0.0,
                    tracks: vec![200.0, 200.0, 200.0],
                },
                cx,
            );
            view.col_resize_to(-10_000.0, cx);
            let tracks = view.table_ui.col_widths.get(&loc.table).expect("tracks");
            assert!((tracks[0] - md_layout::island::TABLE_MIN_COL_WIDTH).abs() < 1e-6);
            assert!((tracks[1] - 352.0).abs() < 1e-6);
            assert!((tracks[2] - 200.0).abs() < 1e-6);
            view.end_col_resize(cx);
        });
    });
}

#[gpui::test]
fn undo_and_redo_drop_the_touched_col_tracks(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let md_before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    let table = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc")
            .table
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.table_ui
                .col_widths
                .insert(table, vec![240.0, 180.0, 180.0]);
        });
    });
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_table_toolbar(TableOp::MoveColumnRight, window, cx);
        });
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(
            view.state
                .doc
                .document
                .to_markdown()
                .contains("| b | a | c |")
        );
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| view.undo());
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state.doc.document.to_markdown(),
            md_before,
            "undo should have restored the column order"
        );
        assert!(
            !view.table_ui.col_widths.contains_key(&table),
            "the column order is back, but the width entries still sit in the post-move order — replay does not sync the column widths"
        );
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.table_ui
                .col_widths
                .insert(table, vec![180.0, 240.0, 180.0]);
        });
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| view.redo());
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(
                !view.table_ui.col_widths.contains_key(&table),
                "redo should also drop the widths its replay touched"
            );
        });
    });
}

#[gpui::test]
fn undo_of_cell_typing_keeps_the_col_tracks(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X3, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let table = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc")
            .table
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.table_ui
                .col_widths
                .insert(table, vec![240.0, 180.0, 180.0]);
        });
    });
    cx.simulate_input("x");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state
                .doc
                .text(table_leaf(&view.state.doc, "xa"))
                .unwrap_or(""),
            "xa",
            "precondition: the typed character landed in the cell"
        );
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| view.undo());
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state
                .doc
                .text(table_leaf(&view.state.doc, "a"))
                .unwrap_or(""),
            "a",
            "undo should have retracted the character"
        );
        assert_eq!(
            view.table_ui.col_widths.get(&table).map(Vec::as_slice),
            Some(&[240.0, 180.0, 180.0][..]),
            "an undo of in-cell typing does not restructure the table, so the column widths should not be lost"
        );
    });
}
