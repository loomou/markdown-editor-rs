use gpui::{Modifiers, TestAppContext, point, px, size};
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_editor::view::EditorView;
use md_theme::DocumentTheme;

fn test_doc() -> Doc {
    Doc::new(load_markdown(
        "# Heading\n\nneedle appears twice: needle\n\n- item\n  - nested\n",
        editor_options(),
    ))
}

#[gpui::test]
fn editor_can_mount_and_focus(cx: &mut TestAppContext) {
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(test_doc(), DocumentTheme::one_dark(), cx));

    let focused = cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
        focus.is_focused(window)
    });
    assert!(
        focused,
        "the editor focus handle should accept focus after mount"
    );
}

#[gpui::test]
fn editor_delivers_resize_pointer_and_input_events(cx: &mut TestAppContext) {
    let (_editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(test_doc(), DocumentTheme::one_dark(), cx));

    cx.simulate_resize(size(px(360.), px(240.)));
    cx.simulate_mouse_move(point(px(12.), px(12.)), None, Modifiers::none());
    cx.simulate_click(point(px(12.), px(12.)), Modifiers::none());
    cx.simulate_input("needle");
}

#[gpui::test]
fn editor_routes_text_and_keyboard_input_to_document_state(cx: &mut TestAppContext) {
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(test_doc(), DocumentTheme::one_dark(), cx));

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
    cx.simulate_input("X");
    cx.simulate_keystrokes("left right");

    let (text, cursor) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    assert!(text.starts_with('X'));
    assert_eq!(cursor.offset, 1);
}

#[gpui::test]
fn editor_routes_editing_keys_and_break_to_document_state(cx: &mut TestAppContext) {
    let doc = Doc::new(load_markdown("ab", editor_options()));
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
    cx.simulate_input("xy");
    cx.simulate_keystrokes("backspace delete enter");

    let (texts, cursor, second_block) = cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        let texts = leaves
            .iter()
            .map(|&block| view.state.doc.text(block).unwrap_or("").to_string())
            .collect::<Vec<_>>();
        (texts, view.state.cursor, leaves[1])
    });
    assert_eq!(texts, vec!["x", "b", ""]);
    assert_eq!(
        cursor,
        Cursor {
            block: second_block,
            offset: 0
        }
    );
}

#[gpui::test]
fn editor_routes_shift_enter_as_one_soft_break(cx: &mut TestAppContext) {
    let doc = Doc::new(load_markdown("ab", editor_options()));
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
    });
    cx.simulate_keystrokes("right shift-enter");

    let (text, cursor) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor,
        )
    });
    assert_eq!(text, "a\nb");
    assert_eq!(cursor.offset, 2);
}

#[gpui::test]
fn editor_routes_tab_without_inserting_a_second_tab_character(cx: &mut TestAppContext) {
    let doc = Doc::new(load_markdown("- a\n- b\n", editor_options()));
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[1];
            view.state.cursor = Cursor { block, offset: 0 };
        });
    });
    cx.simulate_keystrokes("tab");

    let (texts, cursor) = cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        let texts = leaves
            .iter()
            .map(|&block| view.state.doc.text(block).unwrap_or("").to_string())
            .collect::<Vec<_>>();
        (texts, view.state.cursor)
    });
    assert_eq!(texts, vec!["a", "b", ""]);
    assert_eq!(cursor.offset, 0);
}

#[gpui::test]
fn editor_routes_tab_into_a_paragraph_once(cx: &mut TestAppContext) {
    let doc = Doc::new(load_markdown("hello\n", editor_options()));
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.state.cursor = Cursor { block, offset: 0 };
        });
    });
    cx.simulate_keystrokes("tab");

    let (text, offset) = cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        (
            view.state.doc.text(block).unwrap_or("").to_string(),
            view.state.cursor.offset,
        )
    });
    assert_eq!(text, "\thello");
    assert_eq!(offset, 1);
}

#[gpui::test]
fn editor_routes_primary_bracket_as_indent(cx: &mut TestAppContext) {
    let doc = Doc::new(load_markdown("hello\n", editor_options()));
    let (editor, cx) =
        cx.add_window_view(|_, cx| EditorView::new(doc, DocumentTheme::one_dark(), cx));
    let chord = md_editor::keymap::Chord::new(md_editor::keymap::Mods::primary(), "[");

    cx.update(|window, app| {
        let focus = editor.read(app).focus.clone();
        focus.focus(window);
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.state.cursor = Cursor { block, offset: 0 };
        });
    });
    cx.simulate_keystrokes(&chord.unparse());

    let text = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text(view.state.cursor.block)
            .unwrap_or("")
            .to_string()
    });
    assert_eq!(text, "\thello");
}
