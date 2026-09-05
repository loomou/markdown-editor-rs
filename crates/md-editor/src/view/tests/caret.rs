use super::support::{editor_with_doc, focus_editor};
use crate::view::{CursorMotion, Direction, EditorElement, PendingVertical};
use gpui::TestAppContext;
use gpui::{point, px, size};
use md_core::doc::Cursor;

#[gpui::test]
fn heading_caret_is_taller_than_body_caret(cx: &mut TestAppContext) {
    let body_h = {
        let (editor, cx) = editor_with_doc("hello\n", cx);
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        )
        .1
        .frame
        .snapshot
        .caret_device
        .expect("body caret")
        .3
    };
    let heading_h = {
        let (editor, cx) = editor_with_doc("# Title\n", cx);
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        )
        .1
        .frame
        .snapshot
        .caret_device
        .expect("heading caret")
        .3
    };
    assert!(
        heading_h > body_h + 2.0,
        "heading caret {heading_h} should follow the larger type, body was {body_h}"
    );
}

#[gpui::test]
fn draw_caret_moves_down_for_the_next_text_leaf(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("one\n\ntwo", cx);
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let first_y = first
        .1
        .frame
        .snapshot
        .caret_logical_y
        .expect("first caret y");

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[1];
            view.state.cursor = Cursor { block, offset: 0 };
        });
    });
    let second = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let second_y = second
        .1
        .frame
        .snapshot
        .caret_logical_y
        .expect("second caret y");
    assert!(second_y > first_y);
}

#[gpui::test]
fn down_arrow_schedules_the_frame_that_paints_the_new_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("one\n\ntwo", cx);
    focus_editor(&editor, cx);
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let first_y = first
        .1
        .frame
        .snapshot
        .caret_logical_y
        .expect("first caret y");
    let first_block = cx.update(|_, app| editor.read(app).state.cursor.block);

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.pending_vertical = Some(PendingVertical {
                dir: Direction::Next,
                motion: CursorMotion::Move,
            });
        });
    });
    let landing = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let landed = cx.update(|_, app| editor.read(app).state.cursor.block);
    assert_ne!(landed, first_block, "down already moved the live caret");
    assert!(
        landing.1.refresh,
        "the frame that lands the caret still painted the old one, so paint must ask for another"
    );
    assert_eq!(
        landing.1.frame.snapshot.caret_logical_y.expect("landing y"),
        first_y
    );

    let painted = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let painted_y = painted
        .1
        .frame
        .snapshot
        .caret_logical_y
        .expect("painted caret y");
    assert!(painted_y > first_y);
    assert!(!painted.1.refresh);
}

#[gpui::test]
fn draw_marks_ime_range_without_replacing_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello", cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.cursor.block;
            view.state.cursor = Cursor { block, offset: 2 };
            view.state.marked = Some((block, 1..3));
        });
    });
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snapshot = &drawn.1.frame.snapshot;
    assert!(snapshot.caret_device.is_some());
    let (x, y, w, h) = snapshot.ime_device[0];
    assert!(x.is_finite() && y.is_finite() && w > 0.0 && h > 0.0);
}
