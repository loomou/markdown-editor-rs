use super::support::{draw_editor, editor_with_doc, focus_editor, place_caret, temp_md};
use crate::view::unsaved::window_title;
use crate::view::unsaved::{PendingNav, UnsavedChoice};
use gpui::TestAppContext;
use md_core::document::Command;

#[gpui::test]
fn ctrl_n_opens_blank_untitled(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Heading\n\nbody\n", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("secondary-n");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.source_path.is_none());
            assert!(!view.state.doc.is_dirty());
            let leaves = view.state.doc.text_leaves();
            assert_eq!(leaves.len(), 1);
            assert_eq!(view.state.doc.text(leaves[0]).unwrap(), "");
            assert_eq!(window_title(&view.state.doc), "md-test · untitled");
        })
    });
}

#[gpui::test]
fn dirty_ctrl_n_opens_unsaved_dialog(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Heading\n\nbody\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "x".into() });
        })
    });
    cx.simulate_keystrokes("secondary-n");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert_eq!(view.unsaved_nav, Some(PendingNav::New));
            assert!(view.state.doc.is_dirty());
            assert!(view.state.doc.document.to_markdown().contains("Heading"));
        })
    });
}

#[gpui::test]
fn revealing_heading_then_close_skips_unsaved_dialog(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Heading\n\nbody\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            assert!(!view.state.doc.is_dirty());
            assert!(view.on_window_should_close(window, cx));
            assert!(view.unsaved_nav.is_none());
        })
    });
}

#[gpui::test]
fn dirty_secondary_q_opens_unsaved_dialog(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Heading\n\nbody\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "x".into() });
        })
    });
    cx.simulate_keystrokes("secondary-q");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert_eq!(view.unsaved_nav, Some(PendingNav::Close));
            assert!(view.state.doc.is_dirty());
            assert!(view.state.doc.document.to_markdown().contains("Heading"));
        })
    });
}

#[gpui::test]
fn later_close_replaces_pending_new(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.request_nav(PendingNav::New, window, cx);
            view.request_nav(PendingNav::Close, window, cx);
            assert_eq!(view.unsaved_nav, Some(PendingNav::Close));
        })
    });
}

#[gpui::test]
fn unsaved_cancel_keeps_the_document(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let before = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.request_nav(PendingNav::New, window, cx);
            assert_eq!(view.unsaved_nav, Some(PendingNav::New));
            view.apply_unsaved_choice(UnsavedChoice::Cancel, window, cx);
            assert!(view.unsaved_nav.is_none());
        })
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.is_dirty());
            assert_eq!(view.state.doc.document.to_markdown(), format!("x{before}"));
        })
    });
}

#[gpui::test]
fn unsaved_discard_then_new_leaves_disk_unchanged(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("discard");
    std::fs::write(&path, "on disk\n").expect("seed");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.request_nav(PendingNav::New, window, cx);
            view.apply_unsaved_choice(UnsavedChoice::Discard, window, cx);
        })
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.source_path.is_none());
            assert!(!view.state.doc.is_dirty());
            let leaves = view.state.doc.text_leaves();
            assert_eq!(view.state.doc.text(leaves[0]).unwrap(), "");
        })
    });
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "on disk\n");
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn unsaved_save_then_new_writes_disk(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("save-then-new");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.request_nav(PendingNav::New, window, cx);
            view.apply_unsaved_choice(UnsavedChoice::Save, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.source_path.is_none());
            assert!(!view.state.doc.is_dirty());
            let leaves = view.state.doc.text_leaves();
            assert_eq!(view.state.doc.text(leaves[0]).unwrap(), "");
        })
    });
    let written = std::fs::read_to_string(&path).expect("read");
    assert!(written.contains('x'), "{written:?}");
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn dirty_close_is_blocked_until_discard(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    cx.update(|window, app| {
        let handle = editor.clone();
        window.on_window_should_close(app, move |window, cx| {
            handle.update(cx, |view, cx| view.on_window_should_close(window, cx))
        });
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "x".into() });
        });
    });
    assert!(!cx.simulate_close());
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.is_dirty());
            assert_eq!(view.unsaved_nav, Some(PendingNav::Close));
        })
    });
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_unsaved_choice(UnsavedChoice::Discard, window, cx);
        })
    });
}
