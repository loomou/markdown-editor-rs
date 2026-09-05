use super::support::{key_for, stop_blink, test_doc};
use crate::keymap::Cmd;
use crate::shell::CloseFind;
use crate::shell::Shell;
use crate::view::PendingNav;
use gpui::{TestAppContext, VisualTestContext};
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use std::time::Duration;

fn settle_find(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
}

#[gpui::test]
fn f5_and_f6_toggle_editor_flags_off_by_default(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    let (show_fps, stress) = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        (editor.state.show_fps, editor.state.stress_redraw)
    });
    assert!(!show_fps);
    assert!(!stress);

    cx.simulate_keystrokes("f5");
    cx.simulate_keystrokes("f6");
    let (show_fps, stress) = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        (editor.state.show_fps, editor.state.stress_redraw)
    });
    assert!(show_fps);
    assert!(stress);

    cx.simulate_keystrokes("f5");
    cx.simulate_keystrokes("f6");
    let (show_fps, stress) = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        (editor.state.show_fps, editor.state.stress_redraw)
    });
    assert!(!show_fps);
    assert!(!stress);
}

#[gpui::test]
fn the_find_key_opens_the_bar_and_seeds_the_search(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes(&key_for(Cmd::Find));
    cx.simulate_input("needle");
    cx.run_until_parked();
    settle_find(cx);

    let (find_open, query, search_open, match_count) = cx.update(|_, app| {
        let state = shell.read(app);
        let find = state.find.read(app);
        let editor = state.editor.read(app);
        (
            find.open,
            find.query().to_string(),
            editor.search_open,
            editor.search.matches.len(),
        )
    });
    assert!(find_open);
    assert_eq!(query, "needle");
    assert!(search_open);
    assert_eq!(match_count, 2);
}

#[gpui::test]
fn rebinding_find_moves_the_key_and_retires_the_old_one(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });
    let old = key_for(Cmd::Find);

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.rebind(Cmd::Find, "ctrl-alt-k", cx);
        });
    });
    cx.run_until_parked();

    cx.simulate_keystrokes(&old);
    cx.run_until_parked();
    assert!(
        !cx.update(|_, app| shell.read(app).find.read(app).open),
        "the retired old key {old} still opens the find bar"
    );

    cx.simulate_keystrokes("ctrl-alt-k");
    cx.run_until_parked();
    assert!(
        cx.update(|_, app| shell.read(app).find.read(app).open),
        "the new key should open the find bar"
    );
}

#[gpui::test]
fn the_editor_gets_the_new_table_too(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    cx.simulate_input("x");
    cx.run_until_parked();
    let typed = cx.update(|_, app| {
        shell
            .read(app)
            .editor
            .read(app)
            .state
            .doc
            .document
            .to_markdown()
    });
    assert!(
        typed.contains('x'),
        "precondition: the typed character should be in the document"
    );

    let old = key_for(Cmd::Undo);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.rebind(Cmd::Undo, "ctrl-alt-u", cx);
        });
    });
    cx.run_until_parked();

    let markdown = |cx: &mut VisualTestContext| {
        cx.update(|_, app| {
            shell
                .read(app)
                .editor
                .read(app)
                .state
                .doc
                .document
                .to_markdown()
        })
    };

    cx.simulate_keystrokes(&old);
    cx.run_until_parked();
    assert_eq!(
        markdown(cx),
        typed,
        "the retired old key {old} still triggers undo"
    );

    cx.simulate_keystrokes("ctrl-alt-u");
    cx.run_until_parked();
    assert_ne!(markdown(cx), typed, "the new key should trigger undo");
}

#[gpui::test]
fn find_next_works_from_the_document_too(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("needle needle needle\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes(&key_for(Cmd::FindNext));
    cx.run_until_parked();
    assert!(
        cx.update(|_, app| shell.read(app).find.read(app).open),
        "pressing next while the find bar is closed should open it first"
    );

    cx.simulate_input("needle");
    cx.run_until_parked();
    settle_find(cx);

    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });
    let active = |cx: &mut VisualTestContext| {
        cx.update(|_, app| shell.read(app).editor.read(app).search.active)
    };
    let (first, total) = cx.update(|_, app| {
        let e = shell.read(app).editor.read(app);
        (e.search.active, e.search.matches.len())
    });
    assert_eq!(
        total, 3,
        "precondition: this document should have three matches"
    );
    assert_eq!(first, Some(0));

    cx.simulate_keystrokes(&key_for(Cmd::FindNext));
    cx.run_until_parked();
    assert_eq!(active(cx), Some(1), "next should step to the second match");

    cx.simulate_keystrokes(&key_for(Cmd::FindPrev));
    cx.run_until_parked();
    assert_eq!(
        active(cx),
        Some(0),
        "prev should step back to the first match"
    );
}

#[gpui::test]
fn the_unsaved_dialog_takes_the_current_save_key(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let old = key_for(Cmd::Save);
    cx.update(|_, app| {
        shell.update(app, |s, cx| s.rebind(Cmd::Save, "ctrl-alt-w", cx));
    });

    let path = std::env::temp_dir().join(format!("md-test-unsaved-key-{}.md", std::process::id()));
    let _ = std::fs::remove_file(&path);
    cx.update(|window, app| {
        shell.read(app).editor.clone().update(app, |v, cx| {
            v.state.doc.source_path = Some(path.clone());
            v.apply_cmd(md_core::document::Command::Insert { text: "x".into() });
            v.request_nav(PendingNav::New, window, cx);
        });
    });
    cx.run_until_parked();
    let pending = |cx: &mut VisualTestContext| {
        cx.update(|_, app| shell.read(app).editor.read(app).unsaved_nav.is_some())
    };
    assert!(pending(cx), "precondition: the dialog should have opened");

    cx.simulate_keystrokes(&old);
    cx.run_until_parked();
    assert!(
        pending(cx),
        "the retired old save key {old} still works in this dialog"
    );

    cx.simulate_keystrokes("ctrl-alt-w");
    cx.run_until_parked();
    assert!(!pending(cx), "the new save key should work in this dialog");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file should have been written to disk"),
        "# xHeading\n\nneedle appears twice: needle\n"
    );
    let _ = std::fs::remove_file(&path);
}

#[gpui::test]
fn a_key_the_editor_declines_reaches_the_shell(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.rebind(Cmd::ToggleOutline, "ctrl-alt-o", cx);
            s.rebind(Cmd::ToggleTheme, "ctrl-alt-y", cx);
        });
    });
    cx.run_until_parked();

    let (outline, dark) = cx.update(|_, app| {
        let s = shell.read(app);
        (s.outline_open, s.is_dark())
    });

    cx.simulate_keystrokes("ctrl-alt-o");
    cx.simulate_keystrokes("ctrl-alt-y");
    cx.run_until_parked();

    let (outline_now, dark_now) = cx.update(|_, app| {
        let s = shell.read(app);
        (s.outline_open, s.is_dark())
    });
    assert_eq!(
        outline_now, !outline,
        "the outline key did not bubble up to the shell"
    );
    assert_eq!(
        dark_now, !dark,
        "the theme key did not bubble up to the shell"
    );
}

#[gpui::test]
fn close_find_clears_editor_search_state(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|window, app| {
        let focus = shell.read(app).editor_focus().clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes(&key_for(Cmd::Find));
    cx.simulate_input("needle");
    cx.run_until_parked();
    cx.dispatch_action(CloseFind);
    cx.run_until_parked();

    let (find_open, search_open, matches_empty) = cx.update(|_, app| {
        let state = shell.read(app);
        (
            state.find.read(app).open,
            state.editor.read(app).search_open,
            state.editor.read(app).search.matches.is_empty(),
        )
    });
    assert!(!find_open);
    assert!(!search_open);
    assert!(matches_empty);
}
