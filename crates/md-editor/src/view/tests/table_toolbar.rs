use super::support::{
    TABLE_2X2, TABLE_3ROW, draw_editor, editor_with_doc, focus_editor, place_table_caret,
    table_leaf, table_row_count,
};
use crate::view::{CursorMotion, EditorElement, EditorView};
use gpui::Modifiers;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{MouseButton, TextRun, point, px, size};
use md_core::block::{BlockKind, TableCellAlign};
use md_core::doc::Cursor;
use md_core::document::TableOp;

#[gpui::test]
fn table_toolbar_shows_in_cell_and_hides_on_escape(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        assert_eq!(chrome.loc.rows, 2);
        assert_eq!(chrome.loc.cols, 2);
        assert_eq!(
            crate::view::table_toolbar::table_size_label(&chrome.loc),
            md_i18n::fmt::table_size_in(md_i18n::current(), 2, 2)
        );
    });
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.table_ui.more_open = true;
            cx.notify();
        });
    });
    cx.simulate_keystrokes("escape");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.table_ui.chrome.is_none());
        assert!(!view.table_ui.more_open);
    });
}

#[gpui::test]
fn table_toolbar_chrome_layout_y_holds_across_scroll(cx: &mut TestAppContext) {
    let mut md = TABLE_2X2.to_string();
    for i in 0..40 {
        md.push_str(&format!("para {i}\n\n"));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let y0 = cx.update(|_, app| editor.read(app).table_ui.chrome.expect("chrome").y);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.follow_caret = false;
            view.state.scroll += 40.0;
            cx.notify();
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        assert!(
            (view.state.scroll - 40.0).abs() < 1.0,
            "scroll clamped away: {}",
            view.state.scroll
        );
        assert!(
            (chrome.y - y0).abs() < 2.0,
            "layout y jumped on scroll: {y0} -> {}",
            chrome.y
        );
    });
}

#[gpui::test]
fn table_toolbar_center_align_commits_column(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_table_toolbar(TableOp::SetColumnAlign(TableCellAlign::Center), window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        assert_eq!(chrome.loc.align, TableCellAlign::Center);
        assert_eq!(
            view.state
                .doc
                .table_loc(view.state.cursor.block)
                .map(|l| l.align),
            Some(TableCellAlign::Center)
        );
    });
}

#[gpui::test]
fn table_toolbar_delete_removes_table_and_chrome(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.apply_table_toolbar(TableOp::DeleteTable, window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(view.table_ui.chrome.is_none());
        assert!(!view.state.doc.in_table(view.state.cursor.block));
        assert_eq!(table_row_count(&view.state.doc), 0);
    });
}

#[gpui::test]
fn table_toolbar_more_insert_row_below(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.table_ui.more_open = true;
            view.apply_table_toolbar(TableOp::InsertRowBelow, window, cx);
            assert!(!view.table_ui.more_open);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(table_row_count(&view.state.doc), 3);
        let chrome = view.table_ui.chrome.expect("chrome");
        assert_eq!(chrome.loc.rows, 3);
        assert_eq!(
            crate::view::table_toolbar::table_size_label(&chrome.loc),
            md_i18n::fmt::table_size_in(md_i18n::current(), 3, 2)
        );
    });
}

fn mono_px(cx: &mut VisualTestContext, text: &str, size: f32) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: gpui::Font {
            family: crate::ui::theme::MONO_FONT.into(),
            features: Default::default(),
            fallbacks: None,
            weight: gpui::FontWeight(400.0),
            style: gpui::FontStyle::Normal,
        },
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    cx.update(|window, _| {
        f32::from(
            window
                .text_system()
                .shape_line(text.to_string().into(), px(size), &[run], None)
                .width,
        )
    })
}

fn open_table_more(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) {
    focus_editor(editor, cx);
    place_table_caret(editor, cx, "a", 0);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.table_ui.more_open = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn the_table_menu_shows_only_the_key_it_has(cx: &mut TestAppContext) {
    use crate::keymap::{Chord, Cmd};
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    let chord = Chord::parse("ctrl-f9").expect("must parse");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.keymap
                .set(Cmd::TableRowBelow, chord.clone())
                .expect("must be a legal key");
        });
    });
    open_table_more(&editor, cx);

    let drawn = f32::from(
        cx.debug_bounds("menukb:TableInsertRowBelow")
            .expect("the key cell for \"insert row below\" must be painted")
            .size
            .width,
    );
    let want = mono_px(cx, &chord.display(), 10.5);
    let stale = Cmd::TableRowBelow
        .default_chord()
        .expect("the command must ship with a default key")
        .display();
    assert!(
        (want - mono_px(cx, &stale, 10.5)).abs() >= 1.0,
        "precondition: `{}` and the stock `{stale}` must measure different widths",
        chord.display()
    );
    assert!(
        (drawn - want).abs() < 1.0,
        "the cell painted {drawn}px, but by `{}` it should be {want}px — it drew some other key",
        chord.display()
    );
    for selector in [
        "menukb:TableInsertRowAbove",
        "menukb:TableMoveRowUp",
        "menukb:TableInsertColLeft",
        "menukb:TableDeleteRow",
        "menukb:TableDeleteTable",
    ] {
        assert!(
            cx.debug_bounds(selector).is_none(),
            "{selector} has no key, so that cell must not be painted"
        );
    }

    let (editor, cx) = editor_with_doc(TABLE_2X2, &mut cx.cx);
    cx.update(|_, app| {
        editor.update(app, |view, _| view.keymap.clear(Cmd::TableRowBelow));
    });
    open_table_more(&editor, cx);
    assert!(
        cx.debug_bounds("menukb:TableInsertRowBelow").is_none(),
        "the key was cleared, so that cell must not be painted"
    );
}

#[gpui::test]
fn table_toolbar_overlay_contains_more_menu(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        let scroll = view.state.scroll;
        let x = chrome.x + chrome.w - 20.0;
        let y = chrome.y - scroll - 38.0 + 34.0 + 20.0;
        assert!(
            crate::view::table_toolbar::overlay_contains(chrome, scroll, true, false, (x, y)),
            "more menu at ({x}, {y}) chrome={chrome:?} scroll={scroll}"
        );
        assert!(!crate::view::table_toolbar::overlay_contains(
            chrome,
            scroll,
            false,
            false,
            (x, y)
        ));
    });
}

#[gpui::test]
fn table_toolbar_hover_first_row_shows_col_grip(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_3ROW, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "b", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.set_table_toolbar_hover(true, cx);
            let hover = view.table_ui.grip_hover.expect("col");
            assert_eq!(hover.row, None);
            assert_eq!(hover.col, Some(1));
        });
    });
}

#[gpui::test]
fn table_right_click_moves_caret_before_menu(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let (x, y, want) = cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        let want = table_leaf(&view.state.doc, "b");
        (
            chrome.x + chrome.w * 0.75,
            chrome.y - view.state.scroll + 8.0,
            want,
        )
    });
    cx.simulate_mouse_down(
        point(px(x as f32), px(y as f32)),
        MouseButton::Right,
        Modifiers::none(),
    );
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(view.state.cursor.block, want);
    });
}

#[gpui::test]
fn left_click_moves_caret_on_press(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    let (x, y) = {
        let drawn = cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(600.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        );
        let piece = drawn.1.frame.snapshot.texts.first().expect("text");
        let x = piece.content_origin_device.0
            + f64::from(f32::from(
                piece.art.lines[0].unwrapped_layout.x_for_index(6),
            ))
            + 2.0;
        let y = piece.content_origin_device.1 + piece.art.row_advance * 0.5;
        (x, y)
    };
    cx.simulate_mouse_down(
        point(px(x as f32), px(y as f32)),
        MouseButton::Left,
        Modifiers::none(),
    );
    let offset = cx.update(|_, app| editor.read(app).state.cursor.offset);
    assert!(
        offset >= 5,
        "on press the caret must land on world, but it sits at offset={offset}"
    );
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let caret = drawn.1.frame.snapshot.caret_device.expect("caret");
    assert!(
        (caret.0 - x).abs() < 16.0,
        "the next frame must draw the new caret, caret.x={} click.x={x}",
        caret.0
    );
}

fn text_click_at(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    index: usize,
) -> gpui::Point<gpui::Pixels> {
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let piece = drawn.1.frame.snapshot.texts.first().expect("text");
    let x = piece.content_origin_device.0
        + f64::from(f32::from(
            piece.art.lines[0].unwrapped_layout.x_for_index(index),
        ))
        + 2.0;
    let y = piece.content_origin_device.1 + piece.art.row_advance * 0.5;
    point(px(x as f32), px(y as f32))
}

fn select_offsets(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    from: usize,
    to: usize,
) {
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.doc.text_leaves()[0];
            view.place_cursor(
                Cursor {
                    block,
                    offset: from,
                },
                CursorMotion::Move,
            );
            view.place_cursor(Cursor { block, offset: to }, CursorMotion::Extend);
        });
    });
}

#[gpui::test]
fn right_click_inside_selection_keeps_it(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    select_offsets(&editor, cx, 0, 5);
    let at = text_click_at(&editor, cx, 2);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.update(|_, app| {
        let view = editor.read(app);
        let (a, b) = view.state.selection.expect("the selection must survive");
        assert_eq!((a.offset, b.offset), (0, 5));
    });
}

#[gpui::test]
fn right_click_outside_selection_moves_caret(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello world\n", cx);
    focus_editor(&editor, cx);
    select_offsets(&editor, cx, 0, 5);
    let at = text_click_at(&editor, cx, 8);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(
            view.state.selection, None,
            "clicking outside the selection should clear it"
        );
        assert!(
            view.state.cursor.offset >= 6,
            "the caret must land on world, but it sits at offset={}",
            view.state.cursor.offset
        );
    });
}

#[gpui::test]
fn table_right_click_closes_more_menu(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let (x, y) = cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        let scroll = view.state.scroll;
        (
            chrome.x + chrome.w - 20.0,
            chrome.y - scroll - 38.0 + 34.0 + 20.0,
        )
    });
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.set_table_more_open(true, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.simulate_mouse_down(
        point(px(x as f32), px(y as f32)),
        MouseButton::Right,
        Modifiers::none(),
    );
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(!view.table_ui.more_open);
        assert!(!view.table_ui.picker_open);
    });
}

#[gpui::test]
fn table_right_click_closes_picker(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    let (x, y) = cx.update(|_, app| {
        let view = editor.read(app);
        let chrome = view.table_ui.chrome.expect("chrome");
        let scroll = view.state.scroll;
        (chrome.x + 24.0, chrome.y - scroll - 38.0 + 34.0 + 20.0)
    });
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.toggle_table_picker(cx);
            assert!(view.table_ui.picker_open);
        });
    });
    draw_editor(&editor, cx);
    cx.simulate_mouse_down(
        point(px(x as f32), px(y as f32)),
        MouseButton::Right,
        Modifiers::none(),
    );
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(!view.table_ui.picker_open);
        assert!(!view.table_ui.more_open);
    });
}

#[gpui::test]
fn table_align_hover_preview_paints(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.set_table_align_hover(Some(TableCellAlign::Center), cx);
        });
    });
    draw_editor(&editor, cx);
}

#[gpui::test]
fn table_picker_opens_and_escape_keeps_caret_in_table(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |view, cx| {
            view.toggle_table_picker(cx);
            assert!(view.table_ui.picker_open);
        });
    });
    cx.simulate_keystrokes("escape");
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert!(!view.table_ui.picker_open);
        assert!(view.table_ui.chrome.is_some());
        assert_eq!(
            view.state.doc.kind(view.state.cursor.block),
            Some(BlockKind::TableCell)
        );
    });
}

#[gpui::test]
fn table_picker_resize_is_one_undo_step(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.toggle_table_picker(cx);
            view.picker_press(3, 3, cx);
            view.finish_table_picker(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        assert_eq!((loc.rows, loc.cols), (3, 3));
        assert!(!view.table_ui.picker_open);
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| view.undo());
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        assert_eq!((loc.rows, loc.cols), (2, 2));
    });
}

#[gpui::test]
fn table_picker_shrinks_to_one_row(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(TABLE_2X2, cx);
    focus_editor(&editor, cx);
    place_table_caret(&editor, cx, "a", 0);
    draw_editor(&editor, cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            view.toggle_table_picker(cx);
            view.picker_press(1, 2, cx);
            view.finish_table_picker(window, cx);
        });
    });
    draw_editor(&editor, cx);
    cx.update(|_, app| {
        let view = editor.read(app);
        assert_eq!(table_row_count(&view.state.doc), 1);
        let loc = view
            .state
            .doc
            .table_loc(view.state.cursor.block)
            .expect("loc");
        assert_eq!((loc.rows, loc.cols), (1, 2));
    });
}
