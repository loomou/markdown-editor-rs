use super::support::editor_with_doc;
use crate::view::{CursorMotion, EditorElement, PendingClick};
use gpui::TestAppContext;
use gpui::{EntityInputHandler, point, px, size};
use md_core::doc::Cursor;
use md_core::document::Command;

#[gpui::test]
fn command_input_updates_document_and_cursor(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    let (text, cursor) = cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "abc".into() });
            view.apply_cmd(Command::SoftBreak);
            view.apply_cmd(Command::Insert { text: "x".into() });
            let cursor = view.state.cursor;
            (
                view.state.doc.text(cursor.block).unwrap().to_string(),
                cursor,
            )
        })
    });
    assert_eq!(text, "abc\nx");
    assert_eq!(cursor.offset, "abc\nx".len());
}

#[gpui::test]
fn editor_ime_ranges_use_utf16_at_the_platform_boundary(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("αβ😀x\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let block = view.state.cursor.block;
            view.state.cursor.offset = "αβ😀".len();

            let selected = EntityInputHandler::selected_text_range(view, false, window, cx)
                .expect("selection");
            assert_eq!(selected.range, 4..4);
            assert_eq!(
                EntityInputHandler::character_index_for_point(
                    view,
                    point(px(0.), px(0.)),
                    window,
                    cx,
                ),
                Some(4)
            );

            let mut adjusted = None;
            assert_eq!(
                EntityInputHandler::text_for_range(view, 2..4, &mut adjusted, window, cx),
                Some("😀".into())
            );
            assert_eq!(adjusted, None);

            EntityInputHandler::replace_text_in_range(view, Some(1..2), "γ", window, cx);
            assert_eq!(view.state.doc.text(block), Some("αγ😀x"));
        })
    });
}

#[gpui::test]
fn undo_and_redo_restore_typed_text_and_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.apply_cmd(Command::Insert { text: "ab".into() });
            view.apply_cmd(Command::Insert { text: "c".into() });
            view.undo();
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "");
            assert_eq!(view.state.cursor.offset, 0);
            view.redo();
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abc");
            assert_eq!(view.state.cursor.offset, 3);
        })
    });
}

#[gpui::test]
fn undo_during_ime_then_cancel_keeps_prior_text_deletable(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "abc".into() });
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ddd", None, window, cx);
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abcddd");
            view.undo();
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abc");
            assert_eq!(view.state.cursor.offset, 3);

            EntityInputHandler::unmark_text(view, window, cx);
            EntityInputHandler::replace_and_mark_text_in_range(
                view,
                Some(3..6),
                "",
                None,
                window,
                cx,
            );
            assert_eq!(
                EntityInputHandler::marked_text_range(view, window, cx),
                None
            );

            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abc");
            view.apply_cmd(Command::DeleteBackward);
            view.apply_cmd(Command::DeleteBackward);
            view.apply_cmd(Command::DeleteBackward);
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "");
            assert_eq!(view.state.cursor.offset, 0);
        })
    });
}

#[gpui::test]
fn undo_during_ime_then_commit_does_not_insert_candidate(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "abc".into() });
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ddd", None, window, cx);
            assert_eq!(
                view.state.doc.text(view.state.cursor.block).unwrap(),
                "abcddd"
            );
            view.undo();
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abc");
            assert_eq!(view.state.cursor.offset, 3);

            EntityInputHandler::replace_text_in_range(view, Some(3..6), "x", window, cx);
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "abc");
            assert_eq!(view.state.cursor.offset, 3);
            assert_eq!(
                EntityInputHandler::marked_text_range(view, window, cx),
                None
            );

            EntityInputHandler::unmark_text(view, window, cx);
            view.apply_cmd(Command::DeleteBackward);
            view.apply_cmd(Command::DeleteBackward);
            view.apply_cmd(Command::DeleteBackward);
            let block = view.state.cursor.block;
            assert_eq!(view.state.doc.text(block).unwrap(), "");
            assert_eq!(view.state.cursor.offset, 0);
        })
    });
}

#[gpui::test]
fn undo_during_ime_then_delete_does_not_repaint_undone_glyphs(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_cmd(Command::Insert { text: "abc".into() });
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ddd", None, window, cx);
            view.undo();
            EntityInputHandler::unmark_text(view, window, cx);
            view.apply_cmd(Command::DeleteBackward);
        });
    });
    let (doc_text, offset) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor.offset,
        )
    });
    assert_eq!(doc_text, "ab");
    assert_eq!(offset, 2);

    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let painted = drawn
        .1
        .frame
        .snapshot
        .texts
        .first()
        .and_then(|p| p.art.lines.first().map(|l| l.text.as_ref().to_string()))
        .unwrap_or_default();
    assert_eq!(painted, "ab");
}

#[gpui::test]
fn a_click_elsewhere_takes_the_next_ime_commit_with_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("abc\n\nxyz\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let leaves = view.state.doc.text_leaves();
            let (a, b) = (leaves[0], leaves[1]);

            view.place_cursor(
                Cursor {
                    block: a,
                    offset: 3,
                },
                CursorMotion::Move,
            );
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ni", None, window, cx);
            assert_eq!(view.state.doc.text(a).unwrap(), "abcni");
            assert!(view.state.marked.is_some(), "precondition: should still be composing");


            view.apply_click_hit(
                Cursor {
                    block: b,
                    offset: 0,
                },
                PendingClick {
                    x: 0.0,
                    y: 80.0,
                    motion: CursorMotion::Move,
                    follow_link: false,
                    select_word: false,
                },
            );


            assert_eq!(view.state.marked, None, "the click should interrupt the composition");
            assert_eq!(view.state.doc.text(a).unwrap(), "abc");
            assert_eq!(view.state.cursor.block, b, "the click should drop the caret onto the new block");





            EntityInputHandler::replace_text_in_range(view, None, "x", window, cx);
            assert_eq!(
                view.state.doc.text(b).unwrap(),
                "xyz",
                "the dismissed composition must not ride the click into the new block"
            );
            assert_eq!(
                view.state.doc.text(a).unwrap(),
                "abc",
                "the commit text landed in the old composing block - the click looks swallowed by the IME"
            );
            assert_eq!(view.state.cursor.block, b, "the caret should stay on the clicked block");
        })
    });
}

#[gpui::test]
fn a_vertical_step_during_ime_interrupts_the_composition(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("abc\n\nxyz\n", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
    let a = cx.update(|window, app| {
        let mut a = None;
        editor.update(app, |view, cx| {
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ni", None, window, cx);
            a = Some(view.state.cursor.block);
            assert!(
                view.state.marked.is_some(),
                "precondition: should still be composing"
            );
        });
        a.unwrap()
    });

    cx.simulate_keystrokes("down");
    cx.run_until_parked();

    let (marked, text_a, block) = cx.update(|_, app| {
        let v = editor.read(app);
        (
            v.state.marked.clone(),
            v.state.doc.text(a).unwrap_or("").to_string(),
            v.state.cursor.block,
        )
    });
    assert_eq!(
        marked, None,
        "a keyed step to another block should interrupt the composition"
    );
    assert_eq!(text_a, "abc", "the composition string should be dropped");
    assert_ne!(block, a, "the caret should move to the next block");
}
