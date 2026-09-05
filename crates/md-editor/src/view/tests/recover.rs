use super::support::{editor_with_doc, temp_md};
use crate::view::EditorView;
use crate::view::unsaved::{PendingNav, UnsavedChoice};
use gpui::TestAppContext;
use md_core::document::Command;
use md_theme::DocumentTheme;

fn unique_recovery_dir(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "md-test-editor-recovery-{tag}-{}-{n}",
        std::process::id()
    ));
    p
}

fn cleanup_recovery_dir(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[gpui::test]
fn recovery_writes_untitled_draft_without_touching_autosave(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let dir = unique_recovery_dir("untitled");
    cleanup_recovery_dir(&dir);
    let store = crate::store::recovery::Recovery::in_dir(dir.clone());
    let path = temp_md("recovery-not-source");
    std::fs::write(&path, "seed\n").expect("seed");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_recovery(store.clone());
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let pending = store.load().expect("draft");
    assert!(pending.markdown.contains('x'), "{:?}", pending.markdown);
    assert_eq!(pending.source_path.as_ref(), Some(&path));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "seed\n");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(!view.autosave);
            assert!(view.state.doc.is_dirty());
        })
    });
    cleanup_recovery_dir(&dir);
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn recovery_keeps_latest_idle_snapshot(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let dir = unique_recovery_dir("latest");
    cleanup_recovery_dir(&dir);
    let store = crate::store::recovery::Recovery::in_dir(dir.clone());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_recovery(store.clone());
            view.apply_cmd(Command::Insert { text: "a".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "b".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let pending = store.load().expect("draft");
    assert!(pending.markdown.contains('b'), "{:?}", pending.markdown);
    assert!(pending.markdown.contains('a'), "{:?}", pending.markdown);
    cleanup_recovery_dir(&dir);
}

#[gpui::test]
fn save_clears_recovery_draft(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let dir = unique_recovery_dir("save-clear");
    cleanup_recovery_dir(&dir);
    let store = crate::store::recovery::Recovery::in_dir(dir.clone());
    let path = temp_md("recovery-save");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_recovery(store.clone());
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(store.load().is_some());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.begin_save(path.clone(), true, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(store.load().is_none());
    cleanup_recovery_dir(&dir);
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn discard_new_clears_recovery_draft(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let dir = unique_recovery_dir("discard-new");
    cleanup_recovery_dir(&dir);
    let store = crate::store::recovery::Recovery::in_dir(dir.clone());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_recovery(store.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(store.load().is_some());
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.request_nav(PendingNav::New, window, cx);
            view.apply_unsaved_choice(UnsavedChoice::Discard, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(store.load().is_none());
    cleanup_recovery_dir(&dir);
}

#[gpui::test]
fn recovered_document_opens_dirty_with_source_path(cx: &mut TestAppContext) {
    let path = std::path::PathBuf::from("notes.md");
    let doc = crate::store::recovery::PendingDraft {
        markdown: "# recovered\n".into(),
        source_path: Some(path.clone()),
    }
    .into_doc();
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));
    cx.update(|window, app| {
        editor.update(app, |view, _| {
            view.sync_os_title(window);
            assert!(view.state.doc.is_dirty());
            assert_eq!(view.state.doc.source_path.as_ref(), Some(&path));
            assert!(view.state.doc.document.to_markdown().contains("recovered"));
        })
    });
    assert_eq!(cx.window_title().as_deref(), Some("md-test · notes.md •"));
}
