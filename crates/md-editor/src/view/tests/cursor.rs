use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{CursorMotion, Direction, LineEdge};
use gpui::TestAppContext;
use md_core::doc::Cursor;

#[gpui::test]
fn cursor_horizontal_motion_stays_on_utf8_boundaries(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("αβ", cx);
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
    assert_eq!(cursors.0.offset, "α".len());
    assert_eq!(cursors.1.offset, "αβ".len());
    assert_eq!(cursors.2.offset, "α".len());
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
        "Cmd-Backspace must delete from the line start to the caret: {md:?}"
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
        "with a selection, Cmd-Backspace must delete only the selection: {md:?}"
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
