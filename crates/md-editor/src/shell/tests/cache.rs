use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use crate::shell::outline::{OutlineCache, OutlineRow};
use crate::shell::status::{
    StatusCache, leaf_visits, recounts, reset_leaf_visits, status_counts, status_line_column,
};
use gpui::{TestAppContext, VisualTestContext};
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};

fn first_cursor(doc: &Doc) -> Cursor {
    Cursor {
        block: doc.first_text_leaf().expect("a text leaf"),
        offset: 0,
    }
}

#[test]
fn status_cache_skips_the_walk_when_identity_revision_and_cursor_hold() {
    let doc = test_doc();
    let cursor = first_cursor(&doc);
    let mut cache = StatusCache::default();
    let first = cache.get(&doc, cursor);
    assert_eq!(first, status_counts(&doc, cursor));

    reset_leaf_visits();
    assert_eq!(cache.get(&doc, cursor), first);
    assert_eq!(
        leaf_visits(),
        0,
        "a repeat read at the same identity, revision and cursor must not walk the document"
    );
}

#[test]
fn status_cache_recounts_when_the_identity_changes_at_the_same_revision() {
    let first = Doc::new(load_markdown("a\n", editor_options()));
    let second = Doc::new(load_markdown("a much longer body here\n", editor_options()));
    assert_eq!(
        first.document.revision(),
        second.document.revision(),
        "the two fixtures must collide on the revision for this test to discriminate"
    );

    let mut cache = StatusCache::default();
    let before = cache.get(&first, first_cursor(&first));
    assert_eq!(before, status_counts(&first, first_cursor(&first)));

    reset_leaf_visits();
    let after = cache.get(&second, first_cursor(&second));
    assert!(
        leaf_visits() > 0,
        "a new identity must re-walk the document"
    );
    assert!(after.chars > before.chars);
    assert_eq!(after, status_counts(&second, first_cursor(&second)));
}

#[test]
fn status_cache_keeps_the_totals_when_only_the_cursor_moves() {
    let doc = test_doc();
    let start = first_cursor(&doc);
    let leaves = doc.text_leaves();
    let moved = Cursor {
        block: leaves[leaves.len() - 1],
        offset: 0,
    };

    let mut cache = StatusCache::default();
    let before = cache.get(&doc, start);
    reset_leaf_visits();
    let after = cache.get(&doc, moved);

    assert_eq!(
        leaf_visits(),
        0,
        "moving the caret must not re-walk the leaf counts"
    );
    assert_eq!(after.words, before.words);
    assert_eq!(after.chars, before.chars);
    assert!(after.line > before.line);
    assert_eq!(after, status_counts(&doc, moved));
}

#[test]
fn status_line_column_matches_full_snapshot() {
    let doc = test_doc();
    for id in doc.text_leaves() {
        let text = doc.text(id).unwrap_or("");
        for offset in [0usize, text.len() / 2, text.len()] {
            let cursor = Cursor { block: id, offset };
            let full = status_counts(&doc, cursor);
            let (line, column) = status_line_column(&doc, cursor);
            assert_eq!((line, column), (full.line, full.column));
        }
    }
}

#[test]
fn outline_cache_skips_rebuild_while_revision_holds() {
    let mut cache = OutlineCache::default();
    let mut builds = 0;
    let row = OutlineRow {
        block: 1,
        level: 1,
        label: "A".into(),
    };
    cache.refresh_with(5, 1, || {
        builds += 1;
        vec![row.clone()]
    });
    cache.refresh_with(5, 1, || {
        builds += 1;
        vec![row.clone()]
    });
    assert_eq!(builds, 1);
    assert_eq!(cache.rows.len(), 1);
    cache.refresh_with(5, 2, || {
        builds += 1;
        vec![row]
    });
    assert_eq!(builds, 2);
}

#[test]
fn outline_cache_rebuilds_when_the_document_changes_despite_the_revision() {
    let mut cache = OutlineCache::default();
    let mut builds = 0;
    let row = |label: &str| OutlineRow {
        block: 1,
        level: 1,
        label: label.into(),
    };
    cache.refresh_with(1, 1, || {
        builds += 1;
        vec![row("Previous")]
    });
    cache.refresh_with(2, 1, || {
        builds += 1;
        vec![row("Next")]
    });
    assert_eq!(
        builds, 2,
        "with the identity changed it must rebuild even at the same revision"
    );
    assert_eq!(cache.rows[0].label, "Next");
}

#[gpui::test]
fn status_cache_follows_insert(cx: &mut TestAppContext) {
    use md_core::document::{Command, Sel};
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let before = cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            let editor = shell.editor.read(cx);
            shell
                .status_cache
                .get(&editor.state.doc, editor.state.cursor)
        })
    });
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |editor, cx| {
                let sel = Sel::collapsed(editor.state.cursor);
                editor.state.cursor = editor.state.doc.apply(
                    sel,
                    Command::Insert {
                        text: "hello ".into(),
                    },
                );
                cx.notify();
            });
        })
    });
    let after = cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            let editor = shell.editor.read(cx);
            shell
                .status_cache
                .get(&editor.state.doc, editor.state.cursor)
        })
    });
    let fresh = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        status_counts(&editor.state.doc, editor.state.cursor)
    });
    assert!(after.chars > before.chars);
    assert!(after.words > before.words);
    assert_eq!(after, fresh);
}

#[gpui::test]
fn status_cache_matches_a_full_count_across_a_real_edit_sequence(cx: &mut TestAppContext) {
    use md_core::document::{Command, Sel};
    let doc = Doc::new(load_markdown(
        "# Heading\n\nfirst paragraph here\n\nsecond paragraph here\n\n\
         third paragraph here\n\nneedle appears twice: needle\n",
        editor_options(),
    ));
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(doc, cx));
    stop_blink(&shell, cx);

    let check = |cx: &mut VisualTestContext, label: &str| {
        let (cached, fresh, recomputed) = cx.update(|_, app| {
            let fresh = {
                let editor = shell.read(app).editor.read(app);
                status_counts(&editor.state.doc, editor.state.cursor)
            };
            let cached = shell.update(app, |shell, cx| {
                let editor = shell.editor.read(cx);
                shell
                    .status_cache
                    .get(&editor.state.doc, editor.state.cursor)
            });
            (cached, fresh, recounts())
        });
        assert_eq!(
            cached, fresh,
            "{label}: the cache drifted from a full count"
        );
        assert_eq!(
            recomputed, 0,
            "{label}: the fold fell back to a full recount"
        );
    };

    let apply = |cx: &mut VisualTestContext, cmd: Command| {
        cx.update(|_, app| {
            shell.update(app, |shell, cx| {
                shell.editor.update(cx, |editor, cx| {
                    let sel = Sel::collapsed(editor.state.cursor);
                    editor.state.cursor = editor.state.doc.apply(sel, cmd);
                    cx.notify();
                });
            });
        });
    };

    let history = |cx: &mut VisualTestContext, undo: bool| {
        cx.update(|_, app| {
            shell.update(app, |shell, cx| {
                shell.editor.update(cx, |editor, cx| {
                    let moved = if undo {
                        editor.state.doc.undo()
                    } else {
                        editor.state.doc.redo()
                    };
                    if let Some(sel) = moved {
                        editor.state.cursor = sel.head;
                    }
                    cx.notify();
                });
            });
        });
    };

    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            let editor = shell.editor.read(cx);
            let _ = shell
                .status_cache
                .get(&editor.state.doc, editor.state.cursor);
        });
    });
    reset_leaf_visits();

    apply(cx, Command::Insert { text: "xy ".into() });
    check(cx, "insert");
    apply(cx, Command::Insert { text: "z".into() });
    check(cx, "insert again");
    apply(cx, Command::Break);
    check(cx, "break");
    apply(cx, Command::Break);
    check(cx, "break again");
    apply(cx, Command::DeleteBackward);
    check(cx, "backspace");
    apply(cx, Command::DeleteBackward);
    check(cx, "backspace again");
    history(cx, true);
    check(cx, "undo");
    history(cx, true);
    check(cx, "undo again");
    history(cx, false);
    check(cx, "redo");
    history(cx, false);
    check(cx, "redo again");
    apply(
        cx,
        Command::Insert {
            text: "tail".into(),
        },
    );
    check(cx, "insert after redo");
    apply(cx, Command::DeleteForward);
    check(cx, "delete forward");
    apply(cx, Command::Break);
    check(cx, "break once more");
    apply(cx, Command::DeleteBackward);
    check(cx, "backspace once more");
    history(cx, true);
    check(cx, "final undo");
}

#[test]
fn outline_cache_ignores_a_focus_only_revision_bump() {
    let mut doc = Doc::new(load_markdown(
        "## A `x`\n\nbody\n\n## B `y`\n\nbody\n",
        editor_options(),
    ));
    let mut cache = OutlineCache::default();

    assert!(cache.get(&doc), "the first pass adopts the document");
    assert_eq!(cache.rows.len(), 2);
    let rows = cache.rows.as_ptr();

    let block = doc.text_leaves()[0];
    let before = doc.document.revision();
    doc.retarget_focus(Cursor { block, offset: 2 });
    assert_ne!(
        doc.document.revision(),
        before,
        "the caret move must bump the revision for this test to discriminate"
    );

    assert!(
        !cache.get(&doc),
        "moving the caret does not switch documents"
    );
    assert_eq!(
        cache.rows.as_ptr(),
        rows,
        "a focus-only revision bump must not rebuild the rows"
    );
}

#[test]
fn outline_cache_rebuilds_when_a_heading_text_changes() {
    use md_core::document::{Command, Sel};

    let mut doc = Doc::new(load_markdown("## A\n\nbody\n", editor_options()));
    let mut cache = OutlineCache::default();

    assert!(cache.get(&doc));
    assert_eq!(cache.rows[0].label, "A");

    let block = doc.text_leaves()[0];
    let caret = doc.retarget_focus(Cursor { block, offset: 1 });
    let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "B".into() });

    assert!(!cache.get(&doc));
    assert_eq!(cache.rows[0].label, "AB");
}
