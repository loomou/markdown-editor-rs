use super::support::{
    TABLE_2X2, draw_editor, editor_with_doc, focus_editor, place_caret, place_table_caret,
    table_leaf, table_row_count,
};
use gpui::TestAppContext;
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_core::document::Command;
use md_core::document::TableOp;

#[gpui::test]
fn table_tab_moves_to_next_cell(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    cx.simulate_keystrokes("tab");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.cursor.block, table_leaf(&view.state.doc, "b"));
        assert_eq!(view.state.cursor.offset, 0);
    });
    cx.simulate_keystrokes("shift-tab");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.cursor.block, table_leaf(&view.state.doc, "a"));
    });
}

#[gpui::test]
fn table_tab_on_last_cell_inserts_row(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "d", 0);
    cx.simulate_keystrokes("tab");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(table_row_count(&view.state.doc), 3);
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some(""));
        assert_eq!(view.state.cursor.offset, 0);
        let home = view
            .state
            .doc
            .table_step(view.state.cursor, md_core::document::TableStep::RowHome)
            .expect("home");
        assert_eq!(home.block, view.state.cursor.block);
    });
}

#[gpui::test]
fn select_all_backspace_clears_mixed_table_document(cx: &mut TestAppContext) {
    let md = "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n\nworld\n";
    let (editor, cx) = editor_with_doc(md, cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_keystrokes("backspace");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(table_row_count(&view.state.doc), 0);
        let leaves = view.state.doc.text_leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(view.state.doc.kind(leaves[0]), Some(BlockKind::Paragraph));
        assert_eq!(view.state.doc.text(leaves[0]), Some(""));
        assert_eq!(view.state.cursor.block, leaves[0]);
        assert_eq!(view.state.cursor.offset, 0);
    });
}

#[gpui::test]
fn cross_cell_backspace_clears_selected_cells(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("| aa | bb |\n| --- | --- |\n| cc | dd |\n\nafter\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let a = table_leaf(&view.state.doc, "aa");
            let b = table_leaf(&view.state.doc, "bb");
            view.state.selection = Some((
                Cursor {
                    block: a,
                    offset: 1,
                },
                Cursor {
                    block: b,
                    offset: 2,
                },
            ));
            view.state.cursor = Cursor {
                block: b,
                offset: 2,
            };
            view.apply_cmd(Command::DeleteBackward);
        });
    });
    cx.update(|_, app| {
        let view = editor.read(app);
        let a = table_leaf(&view.state.doc, "a");
        assert_eq!(table_row_count(&view.state.doc), 2);
        assert_eq!(view.state.doc.text(a), Some("a"));
        assert_eq!(
            view.state.doc.text(table_leaf(&view.state.doc, "cc")),
            Some("cc")
        );
        assert_eq!(
            view.state.doc.text(table_leaf(&view.state.doc, "dd")),
            Some("dd")
        );
        assert_eq!(view.state.cursor.block, a);
        assert_eq!(view.state.cursor.offset, 1);
    });
}

#[gpui::test]
fn backspace_at_origin_of_empty_table_deletes_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Table(TableOp::Insert { rows: 2, cols: 2 }));
            assert!(view.caret_in_table());
        });
    });
    cx.simulate_keystrokes("backspace");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(!view.caret_in_table());
        assert_eq!(table_row_count(&view.state.doc), 0);
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::Paragraph)
        );
    });
}

#[gpui::test]
fn table_enter_is_noop_in_cell(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 1);
    cx.simulate_keystrokes("enter");
    cx.update(|_, app| {
        let view = editor.read(app);
        let cell = table_leaf(&view.state.doc, "a");
        assert_eq!(view.state.cursor.block, cell);
        assert_eq!(view.state.cursor.offset, 1);
        assert_eq!(view.state.doc.kind(cell), Some(BlockKind::TableCell));
        assert_eq!(view.state.doc.text(cell), Some("a"));
    });
}

#[gpui::test]
fn table_shift_enter_inserts_soft_break_in_cell(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 1);
    cx.simulate_keystrokes("shift-enter");
    cx.update(|_, app| {
        let view = editor.read(app);
        let cell = table_leaf(&view.state.doc, "a\n");
        assert_eq!(view.state.cursor.block, cell);
        assert_eq!(view.state.cursor.offset, 2);
        assert_eq!(view.state.doc.kind(cell), Some(BlockKind::TableCell));
        assert_eq!(view.state.doc.text(cell), Some("a\n"));
    });
}

#[gpui::test]
fn table_ctrl_enter_inserts_row_below(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    cx.simulate_keystrokes("secondary-enter");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(table_row_count(&view.state.doc), 3);
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some(""));
    });
}

#[gpui::test]
fn table_escape_exits_after_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    cx.simulate_keystrokes("escape");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some("after"));
        assert_eq!(view.state.cursor.offset, 0);
        assert!(view.state.selection.is_none());
    });
}

#[gpui::test]
fn table_right_leaves_last_cell_to_next_block(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "d", 1);
    cx.simulate_keystrokes("right");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some("after"));
        assert_eq!(view.state.cursor.offset, 0);
    });
}

#[gpui::test]
fn table_down_moves_to_cell_below(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.simulate_keystrokes("down");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.cursor.block, table_leaf(&view.state.doc, "c"));
        assert_eq!(view.state.cursor.offset, 0);
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
    });
}

#[gpui::test]
fn table_down_from_last_row_leaves_the_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.simulate_keystrokes("down");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some("after"));
        assert!(!view.state.doc.in_table(view.state.cursor.block));
    });
}

#[gpui::test]
fn table_arrows_leave_a_trailing_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("| a | b |\n| --- | --- |\n| c | d |\n", cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "d", 1);
    cx.simulate_keystrokes("right");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some(""));
        assert!(!view.state.doc.in_table(view.state.cursor.block));
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::Paragraph)
        );
    });
    place_table_caret(&editor, cx, "c", 0);
    draw_editor(&editor, cx);
    cx.simulate_keystrokes("down");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some(""));
        assert!(!view.state.doc.in_table(view.state.cursor.block));
    });
}

#[gpui::test]
fn caret_in_heading_does_not_mark_document_dirty(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# hi\n\npara\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(!view.state.doc.is_dirty());
    });
}

#[gpui::test]
fn loaded_document_keeps_a_trailing_blank_paragraph(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# last\n", cx);
    focus_editor(&editor, cx);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        let last = *leaves.last().expect("last");
        assert_eq!(view.state.doc.kind(last), Some(BlockKind::Paragraph));
        assert_eq!(view.state.doc.text(last), Some(""));
        assert_eq!(view.state.doc.document.to_markdown(), "# last\n");
        assert!(!view.state.doc.is_dirty());
    });
}
