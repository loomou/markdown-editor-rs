use super::support::{TABLE_2X2, editor_with_doc, focus_editor, place_table_caret, table_leaf};
use gpui::TestAppContext;
use md_core::block::BlockKind;

#[gpui::test]
fn ctrl_t_opens_insert_table_dialog(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("secondary-t");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.insert_table.is_some());
        assert!(!view.caret_in_table());
    });
}

#[gpui::test]
fn ctrl_t_inside_table_is_noop(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    cx.simulate_keystrokes("secondary-t");
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.insert_table.is_none());
        assert!(view.caret_in_table());
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some("a"));
    });
}

#[gpui::test]
fn insert_table_dialog_escape_closes_without_mutating(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    let before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_insert_table(window, cx);
            assert!(view.insert_table.is_some());
            view.close_insert_table(window, cx);
            assert!(view.insert_table.is_none());
        })
    });
    cx.update(|_, app| {
        assert_eq!(editor.read(app).state.doc.document.to_markdown(), before);
    });
}

#[gpui::test]
fn insert_table_dialog_create_inserts_sized_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_insert_table(window, cx);
            let state = view.insert_table.as_mut().expect("dialog");
            state.rows = "3".into();
            state.cols = "4".into();
            view.confirm_insert_table(window, cx);
            assert!(view.insert_table.is_none());
            let loc = view
                .state
                .doc
                .table_loc(view.state.cursor.block)
                .expect("loc");
            assert_eq!(loc.rows, 3);
            assert_eq!(loc.cols, 4);
            assert_eq!(
                view.state.doc.kind(view.state.cursor.block),
                Some(BlockKind::TableCell)
            );
        })
    });
}

#[gpui::test]
fn insert_table_dialog_rejects_invalid_dims(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    focus_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_insert_table(window, cx);
            view.insert_table.as_mut().expect("dialog").rows = "0".into();
            view.confirm_insert_table(window, cx);
            assert!(view.insert_table.is_some());
            assert!(!view.caret_in_table());
        })
    });
}

#[gpui::test]
fn pipe_header_enter_creates_table_in_editor(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("|a|b|", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("enter");
    cx.update(|_, app| {
        let view = editor.read(app);
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("table");
        assert_eq!(loc.rows, 2);
        assert_eq!(loc.cols, 2);
        assert_eq!(
            view.state.doc.text(table_leaf(&view.state.doc, "a")),
            Some("a")
        );
        assert_eq!(
            view.state.doc.text(table_leaf(&view.state.doc, "b")),
            Some("b")
        );
        assert_eq!(view.state.doc.text(view.state.cursor.block), Some(""));
        assert_ne!(view.state.cursor.block, table_leaf(&view.state.doc, "a"));
    });
}
