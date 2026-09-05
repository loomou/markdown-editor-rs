use super::support::{context_labels, stop_blink, test_doc};
use crate::keymap::Cmd;
use crate::shell::Shell;
use crate::shell::menu::MenuAction;
use gpui::TestAppContext;
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_core::document::TableOp;
use md_core::document::{editor_options, load_markdown};

fn table_row_count(doc: &Doc) -> usize {
    doc.document
        .preorder()
        .into_iter()
        .filter(|&id| {
            doc.document
                .arena
                .get(id)
                .is_some_and(|n| n.kind == BlockKind::TableRow)
        })
        .count()
}

#[gpui::test]
fn context_table_item_tracks_caret(cx: &mut TestAppContext) {
    let outside = {
        let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
        stop_blink(&shell, cx);
        cx.update(|_, app| {
            let editor = shell.read(app).editor.read(app);
            editor.caret_in_table()
        })
    };
    assert!(!outside);
    assert!(!context_labels(outside).contains(&"Table"));

    let inside = {
        let (shell, cx) = cx.add_window_view(|_, cx| {
            Shell::new(
                Doc::new(load_markdown(
                    "| a | b |\n| --- | --- |\n| c | d |\n\nafter\n",
                    editor_options(),
                )),
                cx,
            )
        });
        stop_blink(&shell, cx);
        cx.update(|_, app| {
            let editor = shell.read(app).editor.read(app);
            editor.caret_in_table()
        })
    };
    assert!(inside);
    assert!(context_labels(inside).contains(&"Table"));
}

#[gpui::test]
fn context_table_insert_row_below_grows_the_table(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "| a | b |\n| --- | --- |\n| c | d |\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    let before = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.caret_in_table());
        table_row_count(&editor.state.doc)
    });
    assert_eq!(before, 2);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Table(TableOp::InsertRowBelow), window, cx);
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(table_row_count(&editor.state.doc), before + 1);
    });
}

#[gpui::test]
fn insert_table_menu_creates_a_2x2_on_confirm(cx: &mut TestAppContext) {
    let (shell, cx) =
        cx.add_window_view(|_, cx| Shell::new(Doc::new(load_markdown("", editor_options())), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Cmd(Cmd::InsertTable), window, cx);
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.insert_table.is_some());
        assert_eq!(table_row_count(&editor.state.doc), 0);
    });
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |editor, cx| {
                editor.confirm_insert_table(window, cx);
            });
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.insert_table.is_none());
        assert_eq!(table_row_count(&editor.state.doc), 2);
        assert_eq!(
            editor.state.doc.kind(editor.state.cursor.block),
            Some(BlockKind::TableCell)
        );
        let loc = editor
            .state
            .doc
            .table_loc(editor.state.cursor.block)
            .expect("loc");
        assert_eq!(loc.rows, 2);
        assert_eq!(loc.cols, 2);
    });
}

#[gpui::test]
fn insert_table_menu_cancel_does_not_mutate(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(Doc::new(load_markdown("hello\n", editor_options())), cx)
    });
    stop_blink(&shell, cx);
    let before = cx.update(|_, app| {
        shell
            .read(app)
            .editor
            .read(app)
            .state
            .doc
            .document
            .to_markdown()
    });
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Cmd(Cmd::InsertTable), window, cx);
            shell.editor.update(cx, |editor, cx| {
                editor.close_insert_table(window, cx);
            });
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.insert_table.is_none());
        assert_eq!(editor.state.doc.document.to_markdown(), before);
    });
}

#[gpui::test]
fn insert_table_menu_is_noop_inside_a_table(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown(
                "| a | b |\n| --- | --- |\n| c | d |\n",
                editor_options(),
            )),
            cx,
        )
    });
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            assert!(shell.editor.read(cx).caret_in_table());
            shell.run_menu_action(MenuAction::Cmd(Cmd::InsertTable), window, cx);
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.insert_table.is_none());
        assert_eq!(table_row_count(&editor.state.doc), 2);
    });
}
