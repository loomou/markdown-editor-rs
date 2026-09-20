use super::support::{editor_with_doc, focus_editor, place_caret};
use crate::view::{EditorElement, EditorView, PrepaintState};
use gpui::{Entity, TestAppContext, VisualTestContext, point, px, size};
use md_core::block::BlockKind;
use std::time::Duration;

fn set_reading(editor: &Entity<EditorView>, cx: &mut VisualTestContext, on: bool) {
    cx.update(|_, app| {
        editor.update(app, |view, cx| view.set_reading(on, cx));
    });
}

fn stop_blink(editor: &Entity<EditorView>, cx: &mut VisualTestContext) {
    cx.update(|_, app| {
        editor.update(app, |view, _| view.stop_blink());
    });
}

fn draw(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> PrepaintState {
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    )
    .1
}

fn block_text(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, app| {
        let view = editor.read(app);
        let block = view.state.cursor.block;
        view.state.doc.text(block).unwrap_or("").to_string()
    })
}

fn scroll(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> f64 {
    cx.update(|_, app| editor.read(app).state.scroll)
}

fn paragraph_line_height(editor: &Entity<EditorView>, cx: &mut VisualTestContext) -> f64 {
    cx.update(|_, app| {
        let role = editor.read(app).state.theme.type_role(BlockKind::Paragraph);
        role.size_px as f64 * role.line_height_em as f64
    })
}

fn long_markdown() -> String {
    let mut out = String::new();
    for i in 0..200 {
        out.push_str(&format!("paragraph number {i}\n\n"));
    }
    out
}

#[gpui::test]
fn reading_mode_swallows_typing_and_editing_keys(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    cx.simulate_input("a");
    cx.simulate_input("b");
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes("delete");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("tab");

    assert_eq!(block_text(&editor, cx), "hello");
}

#[gpui::test]
fn reading_mode_blocks_cut_undo_and_paste(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);

    cx.simulate_input("X");
    let typed = block_text(&editor, cx);
    assert_eq!(typed, "Xhello");

    set_reading(&editor, cx, true);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(block_text(&editor, cx), typed, "undo must not run");
    cx.simulate_keystrokes("secondary-x");
    assert_eq!(block_text(&editor, cx), typed, "cut must not run");
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(block_text(&editor, cx), typed, "paste must not run");

    let dirty = cx.update(|_, app| editor.read(app).state.doc.is_dirty());
    assert!(dirty);
}

#[gpui::test]
fn reading_mode_keeps_select_all(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n\nworld\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    cx.simulate_keystrokes("secondary-a");

    let selection = cx.update(|_, app| editor.read(app).state.selection);
    assert!(selection.is_some(), "select all must survive reading mode");
}

#[gpui::test]
fn reading_mode_hides_the_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    assert!(draw(&editor, cx).caret_on);

    set_reading(&editor, cx, true);
    assert!(!draw(&editor, cx).caret_on);
}

#[gpui::test]
fn entering_reading_mode_folds_the_focused_block(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("a **b** c\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    place_caret(&editor, cx, 0, 3);

    let revealed = block_text(&editor, cx);
    assert!(revealed.contains("**"), "the focused block reveals markers");

    set_reading(&editor, cx, true);

    assert_eq!(block_text(&editor, cx), "a b c");
    let block_edit = cx.update(|_, app| editor.read(app).state.doc.block_edit());
    assert_eq!(block_edit, None);
}

#[gpui::test]
fn leaving_reading_mode_restores_typing(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);

    set_reading(&editor, cx, true);
    cx.simulate_input("X");
    assert_eq!(block_text(&editor, cx), "hello");

    set_reading(&editor, cx, false);
    cx.simulate_input("X");
    assert_eq!(block_text(&editor, cx), "Xhello");
}

#[gpui::test]
fn reading_mode_arrow_down_scrolls_exactly_one_line(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    let line = paragraph_line_height(&editor, cx);
    assert!(line > 0.0);
    assert_eq!(scroll(&editor, cx), 0.0);

    cx.simulate_keystrokes("down");
    draw(&editor, cx);

    let after = scroll(&editor, cx);
    assert!(
        (after - line).abs() < 0.51,
        "one press should move one line: {after} vs {line}"
    );

    cx.simulate_keystrokes("up");
    draw(&editor, cx);

    assert_eq!(scroll(&editor, cx), 0.0);
}

#[gpui::test]
fn reading_mode_arrow_down_steps_once_per_press(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    let line = paragraph_line_height(&editor, cx);
    cx.simulate_keystrokes("down");

    draw(&editor, cx);
    draw(&editor, cx);
    draw(&editor, cx);

    let after = scroll(&editor, cx);
    assert!(
        (after - line).abs() < 0.51,
        "extra frames must not repeat a single press: {after} vs {line}"
    );
}

#[gpui::test]
fn holding_arrow_down_keeps_scrolling(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    let line = paragraph_line_height(&editor, cx);
    cx.simulate_keystrokes("down");
    draw(&editor, cx);
    let first = scroll(&editor, cx);
    assert!((first - line).abs() < 0.51);

    let backdate = Duration::from_millis(2000);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let hold = view.reading_hold.as_mut().expect("the key is still held");
            hold.since -= backdate;
            hold.last_tick -= backdate;
        });
    });
    draw(&editor, cx);

    let held = scroll(&editor, cx);
    assert!(
        held > first + line,
        "a held key should keep scrolling: {held} vs {first}"
    );
}

#[gpui::test]
fn releasing_the_key_stops_the_scroll(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    cx.simulate_keystrokes("down");
    draw(&editor, cx);

    let stopped =
        cx.update(|_, app| editor.update(app, |view, cx| view.end_reading_scroll("down", cx)));
    assert!(stopped);

    let line = paragraph_line_height(&editor, cx);
    let before = scroll(&editor, cx);
    draw(&editor, cx);
    assert!(
        (scroll(&editor, cx) - before).abs() < 0.51,
        "after release only the pending step may land, once"
    );
    assert!(before >= line);
}

#[gpui::test]
fn reading_mode_home_and_end_reach_the_document_edges(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    let frame = draw(&editor, cx);
    let bottom = (frame.frame.total_height - 600.0).max(0.0);
    assert!(bottom > 0.0, "the test document must overflow the viewport");

    cx.simulate_keystrokes("end");
    draw(&editor, cx);
    let at_end = scroll(&editor, cx);
    assert!(
        (at_end - bottom).abs() < 0.51,
        "end should land on the last page: {at_end} vs {bottom}"
    );

    cx.simulate_keystrokes("home");
    draw(&editor, cx);
    assert_eq!(scroll(&editor, cx), 0.0);
}

#[gpui::test]
fn reading_mode_page_down_advances_by_almost_a_viewport(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(&long_markdown(), cx);
    stop_blink(&editor, cx);
    focus_editor(&editor, cx);
    set_reading(&editor, cx, true);

    let line = paragraph_line_height(&editor, cx);
    cx.simulate_keystrokes("pagedown");
    draw(&editor, cx);

    let after = scroll(&editor, cx);
    let expected = 600.0 - line;
    assert!(
        (after - expected).abs() < 0.51,
        "page down should keep one line of overlap: {after} vs {expected}"
    );
}
