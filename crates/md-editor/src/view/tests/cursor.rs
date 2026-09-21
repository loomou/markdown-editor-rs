use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{CursorMotion, Direction, EditorElement, EditorView, LineEdge};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::doc::Cursor;

#[gpui::test]
fn cursor_horizontal_motion_stays_on_utf8_boundaries(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("你好", cx);
    let cursors = cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.cursor.block;
            view.state.cursor = Cursor { block, offset: 0 };
            view.horizontal(Direction::Next, CursorMotion::Move);
            let first = view.state.cursor;
            view.horizontal(Direction::Next, CursorMotion::Move);
            let second = view.state.cursor;
            view.horizontal(Direction::Prev, CursorMotion::Move);
            (first, second, view.state.cursor)
        })
    });
    assert_eq!(cursors.0.offset, "你".len());
    assert_eq!(cursors.1.offset, "你好".len());
    assert_eq!(cursors.2.offset, "你".len());
}

#[gpui::test]
fn cursor_crosses_sibling_leaves_and_line_edges(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("one\n\ntwo\n", cx);
    let result = cx.update(|_, app| {
        editor.update(app, |view, _| {
            let leaves = view.state.doc.text_leaves();
            let first = leaves[0];
            let second = leaves[1];
            view.state.cursor = Cursor {
                block: first,
                offset: view.state.doc.text(first).unwrap().len(),
            };
            view.horizontal(Direction::Next, CursorMotion::Move);
            let crossed = view.state.cursor;
            view.line_edge(LineEdge::End, CursorMotion::Move);
            let end = view.state.cursor;
            view.horizontal(Direction::Prev, CursorMotion::Move);
            (crossed, end, view.state.cursor, second)
        })
    });
    assert_eq!(
        result.0,
        Cursor {
            block: result.3,
            offset: 0
        }
    );
    assert_eq!(
        result.1,
        Cursor {
            block: result.3,
            offset: 3
        }
    );
    assert_eq!(result.2.offset, 2);
}

#[gpui::test]
fn cursor_word_motion_skips_runs_and_sibling_leaves(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n\nnext\n", cx);
    let result = cx.update(|_, app| {
        editor.update(app, |view, _| {
            let leaves = view.state.doc.text_leaves();
            let first = leaves[0];
            let second = leaves[1];
            view.state.cursor = Cursor {
                block: first,
                offset: 0,
            };
            view.word(Direction::Next, CursorMotion::Move);
            let after_hello = view.state.cursor;
            view.word(Direction::Next, CursorMotion::Move);
            let end_first = view.state.cursor;
            view.word(Direction::Next, CursorMotion::Move);
            let crossed = view.state.cursor;
            view.word(Direction::Prev, CursorMotion::Move);
            (after_hello, end_first, crossed, view.state.cursor, second)
        })
    });
    assert_eq!(result.0.offset, 6);
    assert_eq!(result.1.offset, "hello world".len());
    assert_eq!(
        result.2,
        Cursor {
            block: result.4,
            offset: 0
        }
    );
    assert_eq!(result.3.offset, "hello world".len());
}

#[gpui::test]
fn word_mod_keystroke_moves_by_word(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 0);
    #[cfg(target_os = "macos")]
    cx.simulate_keystrokes("alt-right");
    #[cfg(not(target_os = "macos"))]
    cx.simulate_keystrokes("ctrl-right");
    let offset = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert_eq!(offset, 6);
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn cmd_arrows_jump_line_edges(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 6);
    cx.simulate_keystrokes(&primary_key("left"));
    let start = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert_eq!(start, 0);
    cx.simulate_keystrokes(&primary_key("right"));
    let end = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert_eq!(end, "hello world".len());
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn cmd_up_down_jump_document_edges(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# Heading\n\nhello world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 1, 6);
    cx.simulate_keystrokes(&primary_key("up"));
    let (block, offset, first, last_end) = cx.update(|_, app| {
        let v = editor.read(app);
        let leaves = v.state.doc.text_leaves();
        let last = *leaves.last().expect("leaves exist");
        (
            v.state.cursor.block,
            v.state.cursor.offset,
            leaves[0],
            v.state.doc.text(last).map_or(0, |t| t.len()),
        )
    });
    assert_eq!(block, first, "Cmd-Up should reach the first leaf");
    assert_eq!(offset, 0);
    cx.simulate_keystrokes(&primary_key("down"));
    let (block, offset) = cx.update(|_, app| {
        let v = editor.read(app);
        (v.state.cursor.block, v.state.cursor.offset)
    });
    let last = cx.update(|_, app| {
        *editor
            .read(app)
            .state
            .doc
            .text_leaves()
            .last()
            .expect("leaves exist")
    });
    assert_eq!(block, last, "Cmd-Down should reach the last leaf");
    assert_eq!(offset, last_end);
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn cmd_backspace_deletes_to_line_start(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 6);
    cx.simulate_keystrokes(&primary_key("backspace"));
    let md = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    assert_eq!(
        md, "world\n",
        "⌘⌫ should delete from the row start up to the caret: {md:?}"
    );
    let offset = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert_eq!(offset, 0);

    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.state.cursor = md_core::doc::Cursor { block, offset: 5 };
            view.state.selection = Some((
                md_core::doc::Cursor { block, offset: 0 },
                md_core::doc::Cursor { block, offset: 5 },
            ));
        });
    });
    cx.simulate_keystrokes(&primary_key("backspace"));
    let md = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    assert_eq!(
        md, " world\n",
        "with a selection ⌘⌫ eats only the selection: {md:?}"
    );
}

#[cfg(target_os = "macos")]
fn primary_key(key: &'static str) -> String {
    crate::keymap::Chord::new(crate::keymap::Mods::primary(), key).unparse()
}

#[gpui::test]
fn shift_motion_and_selection_seed_preserve_selected_utf8_text(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("aαβb\n", cx);
    let seed = cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.cursor.block;
            view.state.cursor = Cursor { block, offset: 1 };
            view.horizontal(Direction::Next, CursorMotion::Extend);
            view.horizontal(Direction::Next, CursorMotion::Extend);
            view.selection_seed()
        })
    });
    assert_eq!(seed, "αβ");
}

fn wrap_sweep_doc() -> String {
    let unit = "lorem ipsum dolor sit amet consectetur adipiscing ".repeat(3);
    let mut md = String::new();
    for n in 0..=unit.len() {
        md.push_str(&unit[..n]);
        md.push_str("**bold run here** and `code span` tail\n\n");
    }
    md
}

struct CaretProbe {
    block: u32,
    offset: usize,
    row: Option<i64>,
}

fn probe(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> CaretProbe {
    let cur = cx.update(|_, app| {
        let v = editor.read(app);
        (v.state.cursor.block, v.state.cursor.offset)
    });
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let f = &drawn.1.frame;
    let origin = f
        .texts
        .iter()
        .find(|t| t.block == cur.0)
        .map(|t| (t.content_origin_device.1, &t.art))
        .or_else(|| {
            f.cells
                .iter()
                .find(|c| c.block == cur.0)
                .map(|c| (c.content_origin_device.1, &c.art))
        });
    let row = origin.and_then(|(coy, art)| {
        f.caret_device
            .map(|c| md_render::query::row_at_y(art, c.1 - coy) as i64)
    });
    CaretProbe {
        block: cur.0,
        offset: cur.1,
        row,
    }
}

fn settle(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> CaretProbe {
    for _ in 0..8 {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        if !drawn.1.refresh {
            return probe(editor, cx);
        }
    }
    probe(editor, cx)
}

#[gpui::test]
fn down_arrow_never_stalls_on_the_row_it_left(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&wrap_sweep_doc(), cx);
    focus_editor(&editor, cx);
    let leaves = cx.update(|_, app| editor.read(app).state.doc.text_leaves().len());
    let mut stalled = Vec::new();
    let mut advanced = 0;
    for leaf in 0..leaves {
        place_caret(&editor, cx, leaf, 0);
        let before = settle(&editor, cx);
        cx.simulate_keystrokes("down");
        let after = settle(&editor, cx);
        if after.block != before.block {
            continue;
        }
        if after.row == before.row && after.offset != before.offset {
            stalled.push((leaf, before.offset, after.offset));
        } else if after.row > before.row {
            advanced += 1;
        }
    }
    assert!(
        advanced > 20,
        "the sweep stopped exercising same-block vertical motion ({advanced} rows advanced)"
    );
    assert!(
        stalled.is_empty(),
        "down landed back on the row it started from: {stalled:?}"
    );
}

fn cell_sweep_doc() -> String {
    let unit = "lorem ipsum dolor sit amet consectetur adipiscing ".repeat(2);
    let fixed = "fixed tail that keeps the second column wide ".repeat(2);
    let mut md = String::from("| left | right |\n| --- | --- |\n");
    for n in 0..=unit.len() {
        md.push_str(&format!("| {}**bold run here** | {fixed} |\n", &unit[..n]));
    }
    md
}

#[gpui::test]
fn down_arrow_never_stalls_inside_a_table_cell(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&cell_sweep_doc(), cx);
    focus_editor(&editor, cx);
    let targets = cx.update(|_, app| {
        let v = editor.read(app);
        v.state
            .doc
            .text_leaves()
            .iter()
            .enumerate()
            .filter(|&(_, b)| {
                v.state
                    .doc
                    .text(*b)
                    .is_some_and(|t| t.contains("bold run here"))
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    });
    assert!(targets.len() > 50, "the sweep lost its cells");
    let mut stalled = Vec::new();
    let mut advanced = 0;
    for &leaf in &targets {
        place_caret(&editor, cx, leaf, 0);
        let before = settle(&editor, cx);
        cx.simulate_keystrokes("down");
        let after = settle(&editor, cx);
        if after.block != before.block {
            continue;
        }
        if after.row == before.row && after.offset != before.offset {
            stalled.push((leaf, before.offset, after.offset));
        } else if after.row > before.row {
            advanced += 1;
        }
    }
    assert!(
        advanced > 20,
        "the sweep stopped exercising vertical motion inside cells ({advanced})"
    );
    assert!(
        stalled.is_empty(),
        "down landed back on the row it started from inside a cell: {stalled:?}"
    );
}

fn uneven_table_doc(rows: usize) -> String {
    let unit = "lorem ipsum dolor sit amet consectetur adipiscing ".repeat(2);
    let mut md = String::from("| left | right |\n| --- | --- |\n");
    md.push_str(&format!("| {unit} | x |\n"));
    for _ in 1..rows {
        md.push_str("| aa | bb |\n");
    }
    md
}

#[gpui::test]
fn down_arrow_inside_a_table_never_lands_on_a_sibling_cell(cx: &mut TestAppContext) {
    let mut sideways = Vec::new();
    let mut crossed = 0;
    for rows in [1usize, 2, 3] {
        let (editor, cx) = editor_with_doc(&uneven_table_doc(rows), cx);
        focus_editor(&editor, cx);
        let leaves: Vec<u32> =
            cx.update(|_, app| editor.read(app).state.doc.text_leaves().to_vec());
        for (i, &leaf) in leaves.iter().enumerate() {
            place_caret(&editor, cx, i, 0);
            let before = cx.update(|_, app| {
                let v = editor.read(app);
                let b = v.state.cursor.block;
                (b, v.state.doc.table_loc(b).map(|l| (l.row, l.col)))
            });
            cx.simulate_keystrokes("down");
            let after = cx.update(|_, app| {
                let v = editor.read(app);
                let b = v.state.cursor.block;
                (b, v.state.doc.table_loc(b).map(|l| (l.row, l.col)))
            });
            if after.0 == before.0 {
                continue;
            }
            match (before.1, after.1) {
                (Some((r, c)), Some((r2, c2))) => {
                    if r2 == r + 1 && c2 == c {
                        crossed += 1;
                    } else {
                        sideways.push((rows, i, leaf, (r, c), (r2, c2)));
                    }
                }
                (Some(_), None) => crossed += 1,
                _ => {}
            }
        }
    }
    assert!(
        crossed > 3,
        "the sweep never stepped across cells ({crossed})"
    );
    assert!(
        sideways.is_empty(),
        "down landed on a sibling cell instead of the one below: {sideways:?}"
    );
}

#[gpui::test]
fn down_arrow_walks_the_lines_inside_a_wrapping_table_cell(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&uneven_table_doc(2), cx);
    focus_editor(&editor, cx);
    let tall = cx.update(|_, app| {
        let v = editor.read(app);
        v.state
            .doc
            .text_leaves()
            .iter()
            .enumerate()
            .find(|&(_, b)| v.state.doc.text(*b).is_some_and(|t| t.len() > 80))
            .map(|(i, _)| i)
    });
    let tall = tall.expect("the fixture lost its tall cell");
    place_caret(&editor, cx, tall, 0);
    let before = settle(&editor, cx);
    cx.simulate_keystrokes("down");
    let after = settle(&editor, cx);
    assert_eq!(
        after.block, before.block,
        "down left a cell that still has a lower line: blk {} row {:?} -> blk {} row {:?}",
        before.block, before.row, after.block, after.row
    );
    assert!(
        after.row > before.row,
        "down did not advance a line inside the cell: blk {} row {:?} -> blk {} row {:?}",
        before.block,
        before.row,
        after.block,
        after.row
    );
}
