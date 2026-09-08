use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use crate::shell::outline::{OutlineCache, OutlineRow};
use crate::shell::status::{StatusCache, StatusCounts, status_counts, status_line_column};
use gpui::TestAppContext;
use md_core::doc::Cursor;

#[test]
fn status_cache_skips_full_walk_when_revision_and_cursor_hold() {
    let mut cache = StatusCache::default();
    let cursor = Cursor {
        block: 1,
        offset: 0,
    };
    let mut full = 0;
    let mut line_col = 0;
    let first = StatusCounts {
        line: 3,
        column: 2,
        words: 10,
        chars: 40,
    };
    assert_eq!(
        cache.refresh_with(
            1,
            1,
            cursor,
            || {
                full += 1;
                first
            },
            || {
                line_col += 1;
                (3, 2)
            },
        ),
        first
    );
    assert_eq!(full, 1);
    assert_eq!(line_col, 0);
    assert_eq!(
        cache.refresh_with(
            1,
            1,
            cursor,
            || {
                full += 1;
                first
            },
            || {
                line_col += 1;
                (3, 2)
            },
        ),
        first
    );
    assert_eq!(full, 1);
    assert_eq!(line_col, 0);
}

#[test]
fn status_cache_recomputes_when_the_document_changes_despite_the_revision() {
    let mut cache = StatusCache::default();
    let cursor = Cursor {
        block: 1,
        offset: 0,
    };
    let mut full = 0;
    cache.refresh_with(
        1,
        1,
        cursor,
        || {
            full += 1;
            StatusCounts {
                line: 1,
                column: 1,
                words: 10,
                chars: 40,
            }
        },
        || (1, 1),
    );
    let next = cache.refresh_with(
        2,
        1,
        cursor,
        || {
            full += 1;
            StatusCounts {
                line: 1,
                column: 1,
                words: 99,
                chars: 999,
            }
        },
        || (1, 1),
    );
    assert_eq!(
        full, 2,
        "with the identity changed it must re-walk the document even at the same revision"
    );
    assert_eq!(
        next,
        StatusCounts {
            line: 1,
            column: 1,
            words: 99,
            chars: 999,
        }
    );
}

#[test]
fn status_cache_keeps_word_count_when_only_cursor_moves() {
    let mut cache = StatusCache::default();
    let start = Cursor {
        block: 1,
        offset: 0,
    };
    let moved = Cursor {
        block: 1,
        offset: 4,
    };
    let mut full = 0;
    let mut line_col = 0;
    cache.refresh_with(
        1,
        1,
        start,
        || {
            full += 1;
            StatusCounts {
                line: 1,
                column: 1,
                words: 10,
                chars: 40,
            }
        },
        || {
            line_col += 1;
            (1, 1)
        },
    );
    let next = cache.refresh_with(
        1,
        1,
        moved,
        || {
            full += 1;
            StatusCounts {
                line: 1,
                column: 5,
                words: 99,
                chars: 99,
            }
        },
        || {
            line_col += 1;
            (1, 5)
        },
    );
    assert_eq!(full, 1);
    assert_eq!(line_col, 1);
    assert_eq!(
        next,
        StatusCounts {
            line: 1,
            column: 5,
            words: 10,
            chars: 40,
        }
    );
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

#[gpui::test]
fn outline_cache_skips_rebuild_while_revision_holds(cx: &mut TestAppContext) {
    let (_shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    let mut cache = OutlineCache::default();
    let mut builds = 0;
    let row = OutlineRow {
        block: 1,
        level: 1,
        label: "A".into(),
    };
    cx.update(|window, _| {
        cache.refresh_with(window, 5, 1, || {
            builds += 1;
            vec![row.clone()]
        });
        cache.refresh_with(window, 5, 1, || {
            builds += 1;
            vec![row.clone()]
        });
    });
    assert_eq!(builds, 1);
    assert_eq!(cache.measure_ix, Some(0));
    cx.update(|window, _| {
        cache.refresh_with(window, 5, 2, || {
            builds += 1;
            vec![row]
        });
    });
    assert_eq!(builds, 2);
}

#[gpui::test]
fn outline_cache_rebuilds_when_the_document_changes_despite_the_revision(cx: &mut TestAppContext) {
    let (_shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    let mut cache = OutlineCache::default();
    let mut builds = 0;
    let row = |label: &str| OutlineRow {
        block: 1,
        level: 1,
        label: label.into(),
    };
    cx.update(|window, _| {
        cache.refresh_with(window, 1, 1, || {
            builds += 1;
            vec![row("Previous")]
        });
        cache.refresh_with(window, 2, 1, || {
            builds += 1;
            vec![row("Next")]
        });
    });
    assert_eq!(
        builds, 2,
        "with the identity changed it must rebuild even at the same revision"
    );
    assert_eq!(cache.rows[0].label, "Next");
}

#[gpui::test]
fn outline_measure_index_picks_the_widest_row(cx: &mut TestAppContext) {
    let (_shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    let mut cache = OutlineCache::default();
    let narrow = OutlineRow {
        block: 1,
        level: 1,
        label: "A".into(),
    };
    let wide = OutlineRow {
        block: 2,
        level: 2,
        label: "BBBB".into(),
    };
    cx.update(|window, _| {
        cache.refresh_with(window, 5, 1, || vec![narrow, wide]);
        assert_eq!(cache.measure_ix, Some(1));
        cache.refresh_with(window, 5, 2, Vec::new);
        assert_eq!(cache.measure_ix, None);
    });
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
