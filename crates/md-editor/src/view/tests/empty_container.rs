use super::support::editor_with_doc;
use crate::view::{CursorMotion, EditorElement};
use gpui::TestAppContext;
use gpui::{
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, VisualTestContext, point, px, size,
};
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_core::document::Command;

fn bullets(
    editor: &gpui::Entity<crate::view::EditorView>,
    cx: &mut VisualTestContext,
) -> Vec<(f64, f64, f64, f64, f64)> {
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let mut dots: Vec<(f64, f64, f64, f64, f64)> = drawn
        .1
        .frame
        .decorations
        .iter()
        .filter(|d| d.kind == BlockKind::ListItem)
        .filter_map(|d| {
            d.gutter_dot
                .map(|(x, y)| (x, y, d.rect_device.0, d.rect_device.1, d.rect_device.3))
        })
        .collect();
    dots.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
    dots
}

fn backspace_at_leaf(
    editor: &gpui::Entity<crate::view::EditorView>,
    cx: &mut VisualTestContext,
    leaf: usize,
) {
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[leaf];
            view.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
            view.apply_cmd(Command::DeleteBackward);
        });
    });
}

#[gpui::test]
fn a_nested_empty_item_puts_its_bullet_on_the_outer_baseline(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("- -\n", cx);
    let dots = bullets(&editor, cx);
    assert_eq!(dots.len(), 2, "one bullet per list item");
    assert!(
        dots[0].0 < dots[1].0,
        "the nested bullet sits further right: {dots:?}"
    );
    assert!(
        (dots[0].1 - dots[1].1).abs() < 0.5,
        "the outer and the nested bullet must share one baseline: {dots:?}"
    );
}

#[gpui::test]
fn deleting_the_nested_item_leaves_the_outer_bullet_where_it_was(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("- -\n", cx);
    let before = bullets(&editor, cx);
    assert_eq!(before.len(), 2, "one bullet per list item");

    backspace_at_leaf(&editor, cx, 0);

    let after = bullets(&editor, cx);
    assert_eq!(after.len(), 1, "the nested item is gone");
    assert!(
        (before[0].1 - after[0].1).abs() < 0.5,
        "removing the nested list must not move the outer bullet: {before:?} -> {after:?}"
    );
}

#[gpui::test]
fn a_nested_empty_item_takes_the_caret_and_answers_a_click(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("- -\n", cx);
    let (first_leaf, last_leaf) = cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        (leaves[0], *leaves.last().expect("leaves"))
    });
    assert_ne!(
        first_leaf, last_leaf,
        "the nested item must hold a leaf of its own, ahead of the trailing blank"
    );

    let on_open = cx.update(|_, app| editor.read(app).state.cursor);
    assert_eq!(
        on_open.block, first_leaf,
        "opening the file must leave the caret in the nested item"
    );

    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let mut slots: Vec<(f64, (f64, f64, f64, f64))> = drawn
        .1
        .frame
        .decorations
        .iter()
        .filter(|d| d.kind == BlockKind::ListItem && d.gutter_dot.is_some())
        .map(|d| (d.rect_device.0, d.rect_device))
        .collect();
    slots.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite"));
    let nested = slots.last().expect("the nested slot").1;
    assert!(nested.3 > 0.0, "the nested slot must have height");

    let target = point(
        px((nested.0 + nested.2 * 0.5) as f32),
        px((nested.1 + nested.3 * 0.5) as f32),
    );
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: target,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: target,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    let clicked = cx.update(|_, app| editor.read(app).state.cursor);
    assert_eq!(
        clicked.block, first_leaf,
        "clicking the nested item must put the caret in it"
    );
}
