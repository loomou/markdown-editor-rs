use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{CursorMotion, EditorView};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use md_core::block::BlockKind;
use md_core::doc::Cursor;

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, app| {
        app.read_from_clipboard()
            .and_then(|it| it.text())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn double_click_selects_unicode_word(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 2);
    let span = cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.select_word_at(view.state.cursor);
            view.state.selection.map(|(a, b)| (a.offset, b.offset))
        })
    });
    assert_eq!(span, Some((0, 5)));
}

#[gpui::test]
fn double_click_selects_cjk_ideograph(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("😀😀😀😀\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 0);
    let span = cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.select_word_at(view.state.cursor);
            view.state.selection.map(|(a, b)| (a.offset, b.offset))
        })
    });
    assert_eq!(span, Some((0, "😀".len())));
}

fn select_range(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    leaf: usize,
    from: usize,
    to: usize,
) {
    place_caret(editor, cx, leaf, from);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[leaf];
            view.place_cursor(Cursor { block, offset: to }, CursorMotion::Extend);
        })
    });
}

fn leaf_texts(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, app| {
        let view = editor.read(app);
        let mut texts: Vec<String> = view
            .state
            .doc
            .text_leaves()
            .into_iter()
            .map(|b| view.state.doc.text(b).unwrap_or("").to_string())
            .collect();
        if texts.len() > 1 && texts.last().is_some_and(|s| s.is_empty()) {
            texts.pop();
        }
        texts
    })
}

#[gpui::test]
fn ctrl_c_copies_selection_as_markdown(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello **bold** world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 8);
    assert_eq!(
        leaf_texts(&editor, cx),
        vec!["hello **bold** world".to_string()]
    );
    select_range(&editor, cx, 0, 6, 14);
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(clipboard(cx), "**bold**");
    assert_eq!(
        leaf_texts(&editor, cx),
        vec!["hello **bold** world".to_string()]
    );
}

#[gpui::test]
fn extending_selection_keeps_revealed_inline(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello **bold** world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 8);
    assert!(leaf_texts(&editor, cx)[0].contains("**"));
    select_range(&editor, cx, 0, 6, 14);
    assert_eq!(
        leaf_texts(&editor, cx),
        vec!["hello **bold** world".to_string()]
    );
}

#[gpui::test]
fn dragging_out_of_a_collapsed_paragraph_keeps_inline_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello **bold** world\n\nbeta\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let leaves = view.state.doc.text_leaves();
            view.state.cursor = Cursor {
                block: leaves[0],
                offset: 6,
            };
            view.select_anchor = None;
            view.place_cursor(
                Cursor {
                    block: leaves[1],
                    offset: 2,
                },
                CursorMotion::Extend,
            );
        })
    });
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(clipboard(cx), "**bold** world\n\nbe");
}

#[gpui::test]
fn ctrl_c_without_a_selection_leaves_the_clipboard_alone(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| app.write_to_clipboard(gpui::ClipboardItem::new_string("keep".into())));
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(clipboard(cx), "keep");
}

#[gpui::test]
fn ctrl_v_pastes_into_the_same_paragraph(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| app.write_to_clipboard(gpui::ClipboardItem::new_string("xx".into())));
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.place_cursor(Cursor { block, offset: 2 }, CursorMotion::Move);
        })
    });
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(leaf_texts(&editor, cx), vec!["hexxllo".to_string()]);
    let offset = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert_eq!(offset, 4);
}

#[gpui::test]
fn copy_then_paste_round_trips_inline_source(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello **bold** world\n", cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 8);
    select_range(&editor, cx, 0, 6, 14);
    cx.simulate_keystrokes("secondary-c");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            let end = view.state.doc.text(block).unwrap_or("").len();
            view.place_cursor(Cursor { block, offset: end }, CursorMotion::Move);
        })
    });
    cx.simulate_keystrokes("secondary-v");
    let markdown = cx.update(|_, app| editor.read(app).state.doc.document.to_markdown());
    assert_eq!(markdown, "hello **bold** world**bold**\n");
    let leaves = cx.update(|_, app| editor.read(app).state.doc.text_leaves().len());
    assert_eq!(leaves, 2);
}

#[gpui::test]
fn ctrl_x_cuts_the_selection_and_undo_restores_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("alpha\n\nbeta\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let leaves = view.state.doc.text_leaves();
            view.state.cursor = Cursor {
                block: leaves[0],
                offset: 2,
            };
            view.select_anchor = None;
            view.place_cursor(
                Cursor {
                    block: leaves[1],
                    offset: 3,
                },
                CursorMotion::Extend,
            );
        })
    });
    cx.simulate_keystrokes("secondary-x");
    assert_eq!(clipboard(cx), "pha\n\nbet");
    assert_eq!(leaf_texts(&editor, cx), vec!["ala".to_string()]);
    cx.update(|_, app| editor.update(app, |view, _| view.undo()));
    assert_eq!(
        leaf_texts(&editor, cx),
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[gpui::test]
fn paste_inside_a_code_block_stays_literal(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("```rust\nfn x() {}\n```\n", cx);
    focus_editor(&editor, cx);
    cx.update(|_, app| app.write_to_clipboard(gpui::ClipboardItem::new_string("# t\n".into())));
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
        })
    });
    cx.simulate_keystrokes("secondary-v");
    let (kinds, text) = cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        (
            leaves
                .iter()
                .filter_map(|&b| view.state.doc.kind(b))
                .collect::<Vec<_>>(),
            view.state.doc.text(leaves[0]).unwrap_or("").to_string(),
        )
    });
    assert_eq!(kinds[0], BlockKind::CodeBlock);
    assert_eq!(text, "# t\nfn x() {}");
}

#[gpui::test]
fn ctrl_a_then_copy_takes_the_whole_document(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("# title\n\nhello\n\n- a\n- b\n", cx);
    focus_editor(&editor, cx);
    cx.simulate_keystrokes("secondary-a");
    let span = cx.update(|_, app| {
        let view = editor.read(app);
        view.state.selection.map(|(a, b)| (a.offset, b.offset))
    });
    assert_eq!(span, Some((0, 1)));
    cx.simulate_keystrokes("secondary-c");
    assert_eq!(clipboard(cx), "# title\n\nhello\n\n- a\n- b");
}
