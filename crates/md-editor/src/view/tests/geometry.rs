use super::support::editor_with_doc;
use crate::view::EditorElement;
use gpui::TestAppContext;
use gpui::{point, px, size};
use md_core::doc::Cursor;
use md_core::inline::InlineAlign;
use md_render::snapshot::LayoutSnapshot;
use md_theme::DocumentTheme;

#[gpui::test]
fn draw_builds_text_and_caret_geometry_for_current_cursor(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello", cx);
    let (_request, prepaint) = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    assert!(!prepaint.frame.snapshot.texts.is_empty());
    assert!(prepaint.frame.snapshot.caret_device.is_some());
    assert!(prepaint.frame.snapshot.caret_logical_y.is_some());
    assert!(prepaint.frame.snapshot.total_height > 0.0);
}

#[gpui::test]
fn draw_caret_moves_forward_and_selection_has_geometry(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello", cx);
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let first_x = first.1.frame.snapshot.caret_device.expect("caret").0;

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.cursor.block;
            view.state.cursor = Cursor { block, offset: 3 };
            view.state.selection = Some((Cursor { block, offset: 1 }, Cursor { block, offset: 4 }));
        });
    });
    let second = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let second_snapshot = &second.1.frame.snapshot;
    let second_x = second_snapshot.caret_device.expect("caret").0;
    assert!(second_x > first_x);
    assert_eq!(second_snapshot.selection_device.len(), 1);
    let (x, y, w, h) = second_snapshot.selection_device[0];
    assert!(x.is_finite() && y.is_finite() && w > 0.0 && h > 0.0);
}

#[gpui::test]
fn caret_uses_ink_box_while_selection_keeps_the_line_box(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("hello\n", cx);
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &first.1.frame.snapshot;
    let (_, cy, _, ch) = snap.caret_device.expect("caret");
    let piece = snap.texts.first().expect("text");
    let origin_y = piece.content_origin_device.1;
    let row = piece.art.row_advance;
    assert!(
        ch + 0.5 < row,
        "caret ink {ch} should be shorter than line box {row}"
    );
    let lead = (row - ch) / 2.0;
    assert!(
        (cy - origin_y - lead).abs() < 1.5,
        "caret top {cy} should sit on the type baseline (origin {origin_y}, lead {lead})"
    );

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let block = view.state.cursor.block;
            view.state.selection = Some((Cursor { block, offset: 0 }, Cursor { block, offset: 5 }));
        });
    });
    let second = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let sel = second.1.frame.snapshot.selection_device[0];
    assert!(
        (sel.3 - row).abs() < 1.0,
        "selection should keep the line box, got {} want {row}",
        sel.3
    );
}

#[gpui::test]
fn inline_code_gets_a_plate_that_hugs_the_text(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("lead\n\n`code`\n", cx);
    let code_block = cx.update(|_, app| {
        let mut found = None;
        editor
            .read(app)
            .state
            .doc
            .document
            .for_each_text_leaf(|id, text| {
                if text.contains("code") {
                    found = Some(id);
                    return false;
                }
                true
            });
        found.expect("the paragraph holding the code span")
    });
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &drawn.1.frame.snapshot;
    let inline = DocumentTheme::one_dark().inline;
    let (pad_x, pad_y) = (inline.inline_code_pad_x, inline.inline_code_pad_y);

    assert_eq!(snap.inline_code_device.len(), 1);
    let (plate_x, plate_y, plate_w, plate_h) = snap.inline_code_device[0];
    let piece = snap
        .texts
        .iter()
        .find(|t| t.block == code_block)
        .expect("code paragraph");
    let (ox, oy) = piece.content_origin_device;
    let ink_w = piece.art.max_line_width;

    assert!(
        (plate_x - (ox - pad_x)).abs() < 0.6,
        "plate x {plate_x} should start {pad_x} left of the text {ox}"
    );
    assert!(
        (plate_w - (ink_w + pad_x * 2.0)).abs() < 0.6,
        "plate width {plate_w} should be the ink {ink_w} plus 2x{pad_x}"
    );

    let ink_h = snap.caret_device.expect("caret in the lead paragraph").3;
    let lead = (piece.art.row_advance - ink_h) / 2.0;
    assert!(
        (plate_h - (ink_h + pad_y * 2.0)).abs() < 1.0,
        "plate height {plate_h} should be the ink box {ink_h} plus 2x{pad_y}"
    );
    assert!(
        plate_h + 0.5 < piece.art.row_advance,
        "plate {plate_h} should stay inside the line box {}",
        piece.art.row_advance
    );
    assert!(
        (plate_y - (oy + lead - pad_y)).abs() < 1.0,
        "plate top {plate_y} should sit {pad_y} above the ink box (origin {oy}, lead {lead})"
    );
}

#[gpui::test]
fn inline_code_in_a_table_cell_gets_a_plate(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("| a | b |\n| --- | --- |\n| `t` | c |\n", cx);
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let frame = &drawn.1.frame;
    assert_eq!(frame.snapshot.inline_code_device.len(), 1);
    let (plate_x, plate_y, plate_w, plate_h) = frame.snapshot.inline_code_device[0];
    let cell = frame
        .snapshot
        .cells
        .iter()
        .find(|c| frame.assembly.tree.text(c.cell_box).contains('t'))
        .expect("the cell holding the code span");
    let (cx0, cy0, cw, ch) = cell.rect_device;
    assert!(
        plate_x > cx0 && plate_x + plate_w < cx0 + cw,
        "plate x {plate_x}+{plate_w} should stay inside the cell {cx0}+{cw}"
    );
    assert!(
        plate_y > cy0 && plate_y + plate_h < cy0 + ch,
        "plate y {plate_y}+{plate_h} should stay inside the cell {cy0}+{ch}"
    );
}

#[gpui::test]
fn wrapped_inline_code_does_not_paint_the_line_slack(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("`aa` `bb` `cc` `thisonewraps`\n", cx);
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(220.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &drawn.1.frame.snapshot;
    let piece = snap
        .texts
        .iter()
        .find(|t| t.art.rows >= 2)
        .expect("the paragraph should wrap at this width");
    let (ox, oy) = piece.content_origin_device;
    let col_right = ox + piece.content_width;
    let row_h = piece.art.row_advance;
    let first_row_plate_right = snap
        .inline_code_device
        .iter()
        .filter(|(x, y, w, h)| *w > 0.0 && *x + *w > ox && *y < oy + row_h && *y + *h > oy)
        .map(|(x, _, w, _)| *x + *w)
        .fold(ox, f64::max);
    assert!(
        first_row_plate_right < col_right - 8.0,
        "first-row code plates end at {first_row_plate_right}, filling slack to {col_right}"
    );
    assert!(
        first_row_plate_right > ox,
        "first row should still have code plates"
    );

    let code_block = piece.block;
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let n = view
                .state
                .doc
                .document
                .text_of(code_block)
                .map(str::len)
                .unwrap_or(0);
            view.state.selection = Some((
                Cursor {
                    block: code_block,
                    offset: 0,
                },
                Cursor {
                    block: code_block,
                    offset: n,
                },
            ));
        });
    });
    let selected = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(220.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let sel_snap = &selected.1.frame.snapshot;
    let piece = sel_snap
        .texts
        .iter()
        .find(|t| t.block == code_block)
        .expect("wrapped paragraph");
    let (ox, oy) = piece.content_origin_device;
    let col_right = ox + piece.content_width;
    let row_h = piece.art.row_advance;
    let first_row_sel_right = sel_snap
        .selection_device
        .iter()
        .filter(|(x, y, w, h)| *w > 0.0 && *x + *w > ox && *y < oy + row_h && *y + *h > oy)
        .map(|(x, _, w, _)| *x + *w)
        .fold(ox, f64::max);
    assert!(
        first_row_sel_right < col_right - 8.0,
        "first-row selection ends at {first_row_sel_right}, filling slack to {col_right}"
    );
}

fn table_body_ink_origin(
    snap: &LayoutSnapshot,
    align: InlineAlign,
) -> (md_core::block::BlockId, f64, f64) {
    let cell = snap
        .cells
        .iter()
        .find(|c| !c.header && c.align == align)
        .expect("aligned body cell");
    let ink = cell.art.max_line_width;
    let inner = cell.content_width;
    let dx = match align {
        InlineAlign::Start => 0.0,
        InlineAlign::Center => (inner - ink) / 2.0,
        InlineAlign::End => inner - ink,
    };
    (
        cell.block,
        cell.content_origin_device.0,
        cell.content_origin_device.0 + dx,
    )
}

#[gpui::test]
fn table_aligned_caret_and_selection_follow_painted_text(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(
        "| L | C | R |\n| --- | :---: | ---: |\n| left | mid | right |\n",
        cx,
    );
    let first = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &first.1.frame.snapshot;
    let (left_block, left_cox, left_ink) = table_body_ink_origin(snap, InlineAlign::Start);
    let (center_block, center_cox, center_ink) = table_body_ink_origin(snap, InlineAlign::Center);
    let (right_block, right_cox, right_ink) = table_body_ink_origin(snap, InlineAlign::End);
    assert!(
        center_ink - center_cox > 8.0,
        "center cell should have room to shift, got {}",
        center_ink - center_cox
    );
    assert!(
        right_ink - right_cox > 8.0,
        "right cell should have room to shift, got {}",
        right_ink - right_cox
    );

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.state.cursor = Cursor {
                block: left_block,
                offset: 0,
            };
        });
    });
    let left = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let left_x = left.1.frame.snapshot.caret_device.expect("left caret").0;
    assert!(
        (left_x - left_ink).abs() < 1.5,
        "left caret {left_x} should stay on the ink origin {left_ink} (cox {left_cox})"
    );

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.state.cursor = Cursor {
                block: center_block,
                offset: 0,
            };
            view.state.selection = Some((
                Cursor {
                    block: center_block,
                    offset: 0,
                },
                Cursor {
                    block: center_block,
                    offset: 3,
                },
            ));
        });
    });
    let center = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let center_snap = &center.1.frame.snapshot;
    let center_x = center_snap.caret_device.expect("center caret").0;
    assert!(
        (center_x - center_ink).abs() < 1.5,
        "center caret {center_x} should sit on the painted text {center_ink} (cox {center_cox})"
    );
    let (sx, _, sw, _) = center_snap.selection_device[0];
    assert!(
        (sx - center_ink).abs() < 1.5,
        "center selection {sx} should start on the painted text {center_ink}"
    );
    assert!(sw > 0.0);

    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.state.cursor = Cursor {
                block: right_block,
                offset: 0,
            };
            view.state.selection = None;
        });
    });
    let right = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let right_x = right.1.frame.snapshot.caret_device.expect("right caret").0;
    assert!(
        (right_x - right_ink).abs() < 1.5,
        "right caret {right_x} should sit on the painted text {right_ink} (cox {right_cox})"
    );
}

#[gpui::test]
fn empty_centered_table_cell_caret_sits_in_the_middle(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("| |\n| :---: |\n| |\n", cx);
    let drawn = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &drawn.1.frame.snapshot;
    let cell = snap
        .cells
        .iter()
        .find(|c| !c.header && c.align == InlineAlign::Center)
        .expect("empty center cell");
    let (cox, _) = cell.content_origin_device;
    let expected = cox + cell.content_width / 2.0;
    let block = cell.block;
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.state.cursor = Cursor { block, offset: 0 };
        });
    });
    let again = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let x = again.1.frame.snapshot.caret_device.expect("empty caret").0;
    assert!(
        (x - expected).abs() < 1.5,
        "empty center caret {x} should sit at mid inner {expected} (cox {cox})"
    );
}
