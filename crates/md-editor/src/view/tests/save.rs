use super::support::{editor_with_doc, temp_md};
use crate::view::save::{SaveConflictChoice, SaveWriteResult};
use crate::view::unsaved::PendingNav;
use gpui::TestAppContext;
use md_core::doc::Doc;
use md_core::document::{Command, editor_options, load_markdown};

#[gpui::test]
fn save_to_path_clears_dirty_when_unedited(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("clean");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            assert!(view.state.doc.is_dirty());
            view.begin_save(path.clone(), true, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(!view.state.doc.is_dirty());
            assert_eq!(view.state.doc.source_path.as_ref(), Some(&path));
            assert!(!view.save.in_flight);
        })
    });
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn save_stays_dirty_if_edited_after_snapshot(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("dirty");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.begin_save(path.clone(), true, window, cx);
            view.apply_cmd(Command::Insert { text: "y".into() });
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.is_dirty());
            assert_eq!(view.state.doc.source_path.as_ref(), Some(&path));
            assert!(!view.save.in_flight);
        })
    });
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn save_requires_confirmation_after_an_external_edit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("placeholder\n", cx);
    let path = temp_md("external-conflict");
    std::fs::write(&path, "opened\n").expect("seed");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_from_path(path.clone(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "x".into() });
        })
    });
    std::fs::write(&path, "changed elsewhere\n").expect("external edit");

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.begin_save(path.clone(), false, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert_eq!(view.save_conflict.as_ref(), Some(&path));
            assert!(view.state.doc.is_dirty());
            assert!(!view.save.in_flight);
        })
    });
    assert_eq!(
        std::fs::read_to_string(&path).expect("read conflict"),
        "changed elsewhere\n"
    );
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            assert!(!view.on_window_should_close(window, cx));
            assert!(view.unsaved_nav.is_none());
            assert_eq!(view.save_conflict.as_ref(), Some(&path));
        })
    });

    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_save_conflict_choice(SaveConflictChoice::Overwrite, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.save_conflict.is_none());
            assert!(!view.state.doc.is_dirty());
            assert!(!view.save.in_flight);
        })
    });
    let written = std::fs::read_to_string(&path).expect("read overwrite");
    assert!(written.starts_with('x'), "{written:?}");
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn second_save_is_ignored_while_in_flight(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("inflight");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.save.in_flight = true;
            let epoch = view.save.epoch;
            view.begin_save(path.clone(), true, window, cx);
            assert_eq!(view.save.epoch, epoch);
            assert!(view.save.in_flight);
        })
    });
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn save_failure_cancels_pending_navigation(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "md-test-editor-save-fail-dir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&path).expect("dir");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.request_nav(PendingNav::Close, window, cx);
            assert_eq!(view.unsaved_nav, Some(PendingNav::Close));
            view.begin_save(path.clone(), true, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert_eq!(view.unsaved_nav, None);
            assert_eq!(view.save.pending_after_save, None);
            assert!(matches!(view.notice, Some(crate::Error::Save { .. })));
            assert!(view.state.doc.is_dirty());
            assert!(!view.save.in_flight);
        })
    });
    let _ = std::fs::remove_dir_all(&path);
}

#[gpui::test]
fn open_missing_file_sets_notice_and_keeps_doc(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("keep me\n", cx);
    let path = temp_md("missing");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_from_path(path.clone(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(matches!(view.notice, Some(crate::Error::Read { .. })));
            assert_eq!(
                view.state.doc.document.to_markdown(),
                load_markdown("keep me\n", editor_options()).to_markdown()
            );
        })
    });
}

#[gpui::test]
fn open_non_markdown_sets_notice_and_keeps_doc(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("keep me\n", cx);
    let mut path = temp_md("not-md");
    path.set_extension("txt");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.open_from_path(path.clone(), window, cx);
            assert!(
                matches!(view.notice, Some(crate::Error::NotMarkdown { .. })),
                "{:?}",
                view.notice.as_ref().map(ToString::to_string)
            );
            assert_eq!(
                view.state.doc.document.to_markdown(),
                load_markdown("keep me\n", editor_options()).to_markdown()
            );
        })
    });
}

#[gpui::test]
fn a_save_as_landing_on_an_in_flight_save_is_deferred_not_dropped(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let old = temp_md("busy-old");
    let new = temp_md("busy-new");
    std::fs::write(&old, "seed\n").expect("seed old");
    let _ = std::fs::remove_file(&new);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.state.doc.source_path = Some(old.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.save.in_flight = true;
            view.save_as_confirmed_for_test(new.clone(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.save.in_flight, "Busy should have blocked this takeoff");
            assert_eq!(
                view.save.save_as_retry,
                Some(new.clone()),
                "a Save As landing on an in-flight save should record the intent for retry instead of silently dropping it"
            );
            assert_ne!(view.state.doc.source_path.as_ref(), Some(&new));
            assert!(view.state.doc.is_dirty());
        })
    });
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let epoch = view.save.epoch;
            let edit_gen = view.state.doc.edit_gen();
            let disk = crate::platform::fs_atomic::disk_state(&old).expect("disk");
            view.settle_save(
                old.clone(),
                edit_gen,
                epoch,
                Ok(SaveWriteResult::Written(disk)),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert_eq!(view.save.save_as_retry, None, "the retry should be drained");
            assert_eq!(
                view.state.doc.source_path.as_ref(),
                Some(&new),
                "the settle should have applied the Save As, not stayed on the old path"
            );
            assert!(!view.save.in_flight);
        })
    });
    let written = std::fs::read_to_string(&new).expect("new written");
    assert!(written.contains('x'), "{written:?}");
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::remove_file(&new);
}

#[gpui::test]
fn a_save_as_with_a_pending_nav_still_reaches_the_new_path(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let old = temp_md("nav-old");
    let new = temp_md("nav-new");
    std::fs::write(&old, "seed\n").expect("seed old");
    let _ = std::fs::remove_file(&new);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.state.doc.source_path = Some(old.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.save.pending_after_save = Some(PendingNav::New);
            view.save.in_flight = true;
            view.save_as_confirmed_for_test(new.clone(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let epoch = view.save.epoch;
            let edit_gen = view.state.doc.edit_gen();
            let disk = crate::platform::fs_atomic::disk_state(&old).expect("disk");
            view.settle_save(
                old.clone(),
                edit_gen,
                epoch,
                Ok(SaveWriteResult::Written(disk)),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    let new_written = std::fs::read_to_string(&new).expect("new written");
    assert!(
        new_written.contains('x'),
        "the Save As should have landed on the new path: {new_written:?}"
    );
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(
                view.state.doc.source_path.is_none(),
                "the nav should have left the document untitled"
            );
        })
    });
    let _ = std::fs::remove_file(&old);
    let _ = std::fs::remove_file(&new);
}

#[gpui::test]
fn a_stale_save_landing_releases_in_flight_and_marks_nothing(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("stale");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.save.pending_after_save = Some(PendingNav::Close);
            view.save.in_flight = true;
            view.save.epoch = 9;
            view.settle_save(
                path.clone(),
                0,
                3,
                Err(std::io::Error::other("stale write failed")),
                window,
                cx,
            );
            assert!(
                !view.save.in_flight,
                "a stale settle should also release in_flight — left true, every later save is blocked by Busy"
            );
            assert!(view.notice.is_none(), "a stale result should not pop an error notice");
            assert_eq!(
                view.save.pending_after_save,
                Some(PendingNav::Close),
                "a stale result should not clear pending"
            );
            assert!(view.state.doc.is_dirty(), "a stale result should not mark the document saved");
        })
    });
}

#[gpui::test]
fn replacing_a_document_resets_grouped_runtime_state(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("old\n", cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.search.query = "old".into();
            view.search.active = Some(3);
            view.search.capped = true;
            view.search.reveal = Some((1, 2));
            view.search.refresh = 4;
            view.save.in_flight = true;
            view.save.epoch = 9;
            view.save.autosave_epoch = 7;
            view.save.autosave_retry = true;
            view.save.save_as_retry = Some(std::path::PathBuf::from("pending.md"));
            view.save.pending_after_save = Some(PendingNav::Close);
            view.table_ui.more_open = true;
            view.table_ui.picker_open = true;
            view.table_ui.picker_drag = true;
            view.table_ui.col_widths.insert(7, vec![10.0, 20.0]);
            view.set_autosave(true);

            view.replace_document(Doc::new(load_markdown("new\n", editor_options())), cx);

            assert!(view.search.query.is_empty());
            assert!(view.search.matches.is_empty());
            assert_eq!(view.search.active, None);
            assert!(!view.search.capped);
            assert_eq!(view.search.reveal, None);
            assert_eq!(view.search.refresh, 0);
            assert!(!view.save.in_flight);
            assert_eq!(view.save.epoch, 0);
            assert_eq!(view.save.autosave_epoch, 0);
            assert!(!view.save.autosave_retry);
            assert_eq!(view.save.save_as_retry, None);
            assert_eq!(view.save.pending_after_save, None);
            assert!(!view.table_ui.more_open);
            assert!(!view.table_ui.picker_open);
            assert!(!view.table_ui.picker_drag);
            assert!(view.table_ui.col_widths.is_empty());
            assert!(view.autosave());
        })
    });
}

#[gpui::test]
fn autosave_off_does_not_write_after_idle(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("autosave-off");
    std::fs::write(&path, "seed\n").expect("seed");
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(!view.autosave);
            assert!(view.state.doc.is_dirty());
        })
    });
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "seed\n");
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn autosave_writes_after_idle_when_path_exists(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let path = temp_md("autosave-on");
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_autosave(true);
            view.state.doc.source_path = Some(path.clone());
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(!view.state.doc.is_dirty());
            assert_eq!(view.state.doc.source_path.as_ref(), Some(&path));
        })
    });
    let written = std::fs::read_to_string(&path).expect("read");
    assert!(written.contains('x'), "{written:?}");
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn autosave_untitled_does_not_ask_for_path(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.bind_window(window);
            view.set_autosave(true);
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.note_edit(cx);
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            assert!(view.state.doc.is_dirty());
            assert!(view.state.doc.source_path.is_none());
        })
    });
    assert!(!cx.did_prompt_for_new_path());
}

#[gpui::test]
fn dirty_title_adds_bullet_after_filename(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, _| {
            view.state.doc.source_path = Some(std::path::PathBuf::from("notes.md"));
            view.sync_os_title(window);
        })
    });
    assert_eq!(cx.window_title().as_deref(), Some("md-test · notes.md"));
    cx.update(|window, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "x".into() });
            view.sync_os_title(window);
        })
    });
    assert_eq!(cx.window_title().as_deref(), Some("md-test · notes.md •"));
}
