use super::support::{stop_blink, test_doc};
use crate::keymap::Cmd;
use crate::shell::CloseFind;
use crate::shell::Shell;
use crate::shell::menu::MenuAction;
use crate::view::PendingNav;
use gpui::{TestAppContext, VisualTestContext};

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, app| {
        app.read_from_clipboard()
            .and_then(|it| it.text())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn file_new_opens_blank_untitled(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Cmd(Cmd::New), window, cx);
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.state.doc.source_path.is_none());
        assert!(!editor.state.doc.is_dirty());
        let leaves = editor.state.doc.text_leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(editor.state.doc.text(leaves[0]).unwrap(), "");
    });
}

#[gpui::test]
fn dirty_file_new_opens_unsaved_dialog(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |editor, cx| {
                use md_core::document::{Command, Sel};
                let sel = Sel::collapsed(editor.state.cursor);
                editor.state.cursor = editor
                    .state
                    .doc
                    .apply(sel, Command::Insert { text: "x".into() });
                cx.notify();
            });
            shell.run_menu_action(MenuAction::Cmd(Cmd::New), window, cx);
        })
    });
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert_eq!(editor.unsaved_nav, Some(PendingNav::New));
        assert!(editor.state.doc.is_dirty());
        assert!(editor.state.doc.document.to_markdown().contains("Heading"));
    });
}

#[gpui::test]
fn close_find_cancels_unsaved_dialog(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |editor, cx| {
                use md_core::document::{Command, Sel};
                let sel = Sel::collapsed(editor.state.cursor);
                editor.state.cursor = editor
                    .state
                    .doc
                    .apply(sel, Command::Insert { text: "x".into() });
                cx.notify();
            });
            shell.run_menu_action(MenuAction::Cmd(Cmd::New), window, cx);
        })
    });
    cx.dispatch_action(CloseFind);
    cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        assert!(editor.unsaved_nav.is_none());
        assert!(editor.state.doc.is_dirty());
        assert!(editor.state.doc.document.to_markdown().contains("Heading"));
    });
}

#[gpui::test]
fn edit_menu_select_all_then_copy_fills_the_clipboard(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Cmd(Cmd::SelectAll), window, cx);
            shell.run_menu_action(MenuAction::Cmd(Cmd::Copy), window, cx);
        })
    });
    assert_eq!(clipboard(cx), "# Heading\n\nneedle appears twice: needle");
}

#[gpui::test]
fn edit_menu_paste_lands_at_the_caret(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
        app.write_to_clipboard(gpui::ClipboardItem::new_string("**b**".into()));
    });
    cx.simulate_keystrokes("end");
    cx.update(|window, app| {
        shell.update(app, |shell, cx| {
            shell.run_menu_action(MenuAction::Cmd(Cmd::Paste), window, cx);
        })
    });
    let (markdown, leaves) = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        (
            editor.state.doc.document.to_markdown(),
            editor.state.doc.text_leaves().len(),
        )
    });
    assert_eq!(markdown, "# Heading**b**\n\nneedle appears twice: needle\n");
    assert_eq!(leaves, 3);
}
