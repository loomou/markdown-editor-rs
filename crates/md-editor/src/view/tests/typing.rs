use super::support::editor_with_doc;
use crate::view::EditorElement;
use gpui::TestAppContext;
use gpui::{point, px, size};

#[gpui::test]
fn typing_inline_spaces_keeps_source_display_caret_and_shape_in_sync(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_input("a");
    let after_a = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    let drawn_a = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let piece_a = drawn_a
        .1
        .frame
        .snapshot
        .texts
        .iter()
        .find(|piece| piece.block == after_a.1.block)
        .expect("text piece after a");
    assert_eq!(after_a.0, "a");
    assert_eq!(after_a.1.offset, 1);
    assert_eq!(piece_a.art.lines[0].text.as_ref(), "a");
    let caret_a = drawn_a
        .1
        .frame
        .snapshot
        .caret_device
        .expect("caret after a");
    let width_a = f32::from(piece_a.art.lines[0].width());

    cx.simulate_input(" ");
    let after_one_space = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    let drawn_one_space = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let piece_one_space = drawn_one_space
        .1
        .frame
        .snapshot
        .texts
        .iter()
        .find(|piece| piece.block == after_one_space.1.block)
        .expect("text piece after one space");
    let caret_one_space = drawn_one_space
        .1
        .frame
        .snapshot
        .caret_device
        .expect("caret after one space");
    assert_eq!(after_one_space.0, "a ");
    assert_eq!(after_one_space.1.offset, 2);
    assert_eq!(piece_one_space.art.lines[0].text.as_ref(), "a ");
    assert!(caret_one_space.0 > caret_a.0);
    let width_one_space = f32::from(piece_one_space.art.lines[0].width());
    assert!(width_one_space.is_finite() && width_one_space > width_a);
    let expected_one_space_x = piece_one_space.content_origin_device.0
        + f64::from(f32::from(
            piece_one_space.art.lines[0].unwrapped_layout.x_for_index(2),
        ));
    assert!((caret_one_space.0 - expected_one_space_x).abs() < 0.51);

    cx.simulate_input(" ");
    let after_two_spaces = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    let drawn_two_spaces = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let piece_two_spaces = drawn_two_spaces
        .1
        .frame
        .snapshot
        .texts
        .iter()
        .find(|piece| piece.block == after_two_spaces.1.block)
        .expect("text piece after two spaces");
    assert_eq!(after_two_spaces.0, "a  ");
    assert_eq!(after_two_spaces.1.offset, 3);
    assert_eq!(piece_two_spaces.art.lines[0].text.as_ref(), "a  ");
    let width_two_spaces = f32::from(piece_two_spaces.art.lines[0].width());
    assert!(width_two_spaces > width_one_space);

    cx.simulate_input("b");
    let after_b = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    assert_eq!(after_b.0, "a  b");
    assert_eq!(after_b.1.offset, 4);
}

#[gpui::test]
fn key_dispatch_a_then_space_keeps_inline_run_ranges_and_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes("a");
    cx.simulate_keystrokes("space");

    let (text, markdown, cursor, runs) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        let id = view.state.doc.document.live_id(block).expect("live");
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.doc.document.to_markdown(),
            view.state.cursor,
            view.state.doc.document.runs(id).to_vec(),
        )
    });
    assert_eq!(text, "a ");
    assert_eq!(markdown, "a \n");
    assert_eq!(cursor.offset, 2);
    assert_eq!(
        runs.iter()
            .map(|run| run.display_range.clone())
            .collect::<Vec<_>>(),
        vec![0..2]
    );

    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let piece = drawn
        .1
        .frame
        .snapshot
        .texts
        .iter()
        .find(|piece| piece.block == cursor.block)
        .expect("text piece");
    let caret = drawn.1.frame.snapshot.caret_device.expect("caret");
    let expected_x = piece.content_origin_device.0
        + f64::from(f32::from(
            piece.art.lines[0].unwrapped_layout.x_for_index(2),
        ));
    assert!((caret.0 - expected_x).abs() < 0.51);
    let line_end_x =
        piece.content_origin_device.0 + f64::from(f32::from(piece.art.lines[0].width()));
    assert!((caret.0 - line_end_x).abs() < 0.51);
}

#[gpui::test]
fn key_dispatch_consecutive_spaces_covers_every_inline_byte(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes("a space space b");

    let (text, cursor, runs) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        let id = view.state.doc.document.live_id(block).expect("live");
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
            view.state.doc.document.runs(id).to_vec(),
        )
    });
    assert_eq!(text, "a  b");
    assert_eq!(cursor.offset, 4);
    assert_eq!(runs.last().map(|run| run.display_range.end), Some(4));
}

#[gpui::test]
fn key_dispatch_space_steps_keep_inline_ranges_contiguous(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes("a");
    cx.simulate_keystrokes("space");
    let after_one = cx.update(|_, app| {
        let view = editor.read(app);
        let id = view
            .state
            .doc
            .document
            .live_id(view.state.cursor.block)
            .expect("live");
        view.state.doc.document.runs(id).to_vec()
    });
    assert_eq!(
        after_one
            .iter()
            .map(|run| run.display_range.clone())
            .collect::<Vec<_>>(),
        vec![0..2]
    );

    cx.simulate_keystrokes("space");
    let after_two = cx.update(|_, app| {
        let view = editor.read(app);
        let id = view
            .state
            .doc
            .document
            .live_id(view.state.cursor.block)
            .expect("live");
        view.state.doc.document.runs(id).to_vec()
    });
    assert_eq!(
        after_two
            .iter()
            .map(|run| run.display_range.clone())
            .collect::<Vec<_>>(),
        vec![0..3]
    );
}

#[gpui::test]
fn key_dispatch_a_then_space_renders_the_trailing_space_and_end_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes("a space");
    let cursor = cx.update(|_, app| editor.read(app).state.cursor);
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snapshot = &drawn.1.frame.snapshot;
    let piece = snapshot
        .texts
        .iter()
        .find(|piece| piece.block == cursor.block)
        .expect("text piece");
    assert_eq!(piece.art.lines[0].text.as_ref(), "a ");
    let caret = snapshot.caret_device.expect("caret");
    let expected_x = piece.content_origin_device.0
        + f64::from(f32::from(
            piece.art.lines[0].unwrapped_layout.x_for_index(2),
        ));
    assert!((caret.0 - expected_x).abs() < 0.51);
}

#[gpui::test]
fn key_dispatch_space_advances_rendered_width_and_caret_once(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });

    cx.simulate_keystrokes("a");
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let first_piece = &first.1.frame.snapshot.texts[0];
    let first_width = f32::from(first_piece.art.lines[0].width());
    let first_caret = first.1.frame.snapshot.caret_device.expect("first caret").0;

    cx.simulate_keystrokes("space");
    let second = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let second_piece = &second.1.frame.snapshot.texts[0];
    let second_width = f32::from(second_piece.art.lines[0].width());
    let second_caret = second
        .1
        .frame
        .snapshot
        .caret_device
        .expect("second caret")
        .0;

    assert!(second_width > first_width);
    assert!(second_caret > first_caret);
    assert!((second_caret - first_caret) > 1.0);
}

#[gpui::test]
fn tab_advances_rendered_width_and_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.state.cursor = md_core::doc::Cursor { block, offset: 0 };
        });
    });

    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let first_width = f32::from(first.1.frame.snapshot.texts[0].art.lines[0].width());
    let first_caret = first.1.frame.snapshot.caret_device.expect("first caret").0;

    cx.simulate_keystrokes("tab");
    let text = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text(view.state.cursor.block)
            .unwrap_or("")
            .to_string()
    });
    assert_eq!(text, "\thello");

    let second = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let second_width = f32::from(second.1.frame.snapshot.texts[0].art.lines[0].width());
    let second_caret = second
        .1
        .frame
        .snapshot
        .caret_device
        .expect("second caret")
        .0;
    assert!(second_width > first_width);
    assert!(second_caret > first_caret);
    assert!((second_caret - first_caret) > 1.0);
}
