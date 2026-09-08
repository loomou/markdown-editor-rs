use super::support::{editor_with_doc, focus_editor, math_parts, place_caret};
use crate::view::popover::{Anchor, place};
use crate::view::{CursorMotion, EditorElement, EditorView, PrepaintState};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::block::BlockKind;
use md_core::doc::Cursor;

fn draw_state(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> PrepaintState {
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    )
    .1
}

fn reveal_and_prepaint<'a>(
    markdown: &str,
    cx: &'a mut TestAppContext,
) -> (
    gpui::Entity<EditorView>,
    &'a mut VisualTestContext,
    PrepaintState,
) {
    let (editor, cx) = editor_with_doc(markdown, cx);
    let _ = draw_state(&editor, cx);
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            let block = v.state.cursor.block;
            v.state.cursor = Cursor { block, offset: 2 };
            v.state.cursor = v.state.doc.retarget_focus(v.state.cursor);
        });
    });
    let st = draw_state(&editor, cx);
    (editor, cx, st)
}

fn image_part_count(art: &md_content::shaper::ShapeArtifact) -> usize {
    art.bands
        .iter()
        .flat_map(|b| &b.parts)
        .filter(|p| matches!(p, md_content::shaper::ShapePart::Image { .. }))
        .count()
}

fn joined_text(art: &md_content::shaper::ShapeArtifact) -> String {
    if art.bands.is_empty() {
        return art
            .lines
            .iter()
            .map(|l| l.text.as_ref())
            .collect::<Vec<_>>()
            .join("\n");
    }
    art.bands
        .iter()
        .flat_map(|b| &b.parts)
        .filter_map(|p| match p {
            md_content::shaper::ShapePart::Text { line, .. } => Some(line.text.as_ref()),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn revealed_inline_image_leaves_the_line_and_moves_to_a_popover(cx: &mut TestAppContext) {
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let src = format!("x ![]({png}) y");
    let (_editor, _cx, st) = reveal_and_prepaint(&src, cx);
    let art = &st.frame.snapshot.texts[0].art;

    let text = joined_text(art);
    assert!(
        text.contains(&src),
        "the editable source must survive: {text:?}"
    );
    assert_eq!(
        text.matches("![](").count(),
        1,
        "the source text must not appear twice: {text:?}"
    );
    assert_eq!(
        image_part_count(art),
        0,
        "no image slot should remain in the revealed line"
    );

    let popover = st
        .popover
        .as_ref()
        .expect("revealing an inline image should create a popover");
    let plan = popover
        .plan
        .as_ref()
        .expect("once the image loads its size should be measurable");
    let (_, _, w, h) = plan.plate;
    assert!(w > 0.0 && h > 0.0, "the plate must have a size: {w}x{h}");
}

#[gpui::test]
fn revealed_inline_image_popover_stays_empty_when_loading_fails(cx: &mut TestAppContext) {
    let (editor, cx, st) = reveal_and_prepaint("x ![](./nope.png) y", cx);
    let art = &st.frame.snapshot.texts[0].art;
    assert_eq!(
        image_part_count(art),
        0,
        "no image slot should remain in the revealed line"
    );

    let popover = st
        .popover
        .as_ref()
        .expect("the popover must stay; the dest is still queued");
    assert!(
        popover.plan.is_none(),
        "on load failure no size can be measured, so there should be no geometry"
    );

    cx.run_until_parked();
    let failed = cx.update(|_, app| editor.read(app).images.failed_sources());
    assert!(
        failed.contains(popover.dest.as_str()),
        "a source that failed to load should enter failed_sources: {:?}",
        popover.dest
    );
}

#[test]
fn popover_flips_below_the_anchor_when_it_cannot_fit_above() {
    let anchor = Anchor {
        x: 40.0,
        row_top: 300.0,
        row_bottom: 320.0,
    };
    let (left, top) = place(anchor, (100.0, 80.0), 6.0, (800.0, 600.0))
        .expect("the anchor row should be inside the viewport");
    assert_eq!(left, 40.0);
    assert_eq!(top, 300.0 - 6.0 - 80.0);

    let high = Anchor {
        x: 40.0,
        row_top: 10.0,
        row_bottom: 30.0,
    };
    let (_, top) = place(high, (100.0, 80.0), 6.0, (800.0, 600.0))
        .expect("the anchor row is inside the viewport");
    assert_eq!(top, 30.0 + 6.0);

    let tight = Anchor {
        x: 40.0,
        row_top: 10.0,
        row_bottom: 30.0,
    };
    let (_, top) = place(tight, (100.0, 80.0), 6.0, (800.0, 100.0))
        .expect("the anchor row is inside the viewport");
    assert_eq!(top, 0.0, "clamped to the viewport top");
}

#[test]
fn popover_disappears_once_the_anchor_row_scrolls_off_screen() {
    let plate = (100.0, 80.0);
    let viewport = (800.0, 600.0);

    let above = Anchor {
        x: 40.0,
        row_top: -40.0,
        row_bottom: -20.0,
    };
    assert!(
        place(above, plate, 6.0, viewport).is_none(),
        "the row has scrolled out past the viewport top"
    );

    let below = Anchor {
        x: 40.0,
        row_top: 620.0,
        row_bottom: 640.0,
    };
    assert!(
        place(below, plate, 6.0, viewport).is_none(),
        "the row has scrolled out past the viewport bottom"
    );

    let half = Anchor {
        x: 40.0,
        row_top: -10.0,
        row_bottom: 10.0,
    };
    let (_, top) = place(half, plate, 6.0, viewport).expect("the row still shows half of itself");
    assert_eq!(top, 10.0 + 6.0, "no room above, so it flips below the row");

    let flush = Anchor {
        x: 40.0,
        row_top: -20.0,
        row_bottom: 0.0,
    };
    assert!(
        place(flush, plate, 6.0, viewport).is_none(),
        "flush with the edge does not count as visible"
    );
}

#[gpui::test]
fn revealed_inline_math_leaves_the_line_and_moves_to_a_popover(cx: &mut TestAppContext) {
    let (editor, cx, st) = reveal_and_prepaint("x $a^2$ y", cx);
    let art = &st.frame.snapshot.texts[0].art;

    let text = joined_text(art);
    assert!(
        text.contains("$a^2$"),
        "the editable source text should remain: {text:?}"
    );
    assert_eq!(
        text.matches("$a^2$").count(),
        1,
        "the source text must not appear twice: {text:?}"
    );
    assert!(
        math_parts(art).is_empty(),
        "no math slot should remain in the revealed line: {:?}",
        math_parts(art)
    );

    let popover = st
        .math_popover
        .as_ref()
        .expect("revealing inline math should create a popover");
    let plan = popover
        .plan
        .as_ref()
        .expect("the formula size comes from the font size, so the first frame should have it");
    let (_, _, w, h) = plan.plate;
    assert!(w > 0.0 && h > 0.0, "the plate must have a size: {w}x{h}");

    cx.run_until_parked();
    let ready = cx.update(|_, app| editor.read(app).math.ready_snapshot());
    assert!(
        ready.contains_key(&plan.key),
        "the popover key should hit the inline copy of the rasterized image"
    );
}

#[gpui::test]
fn revealed_inline_math_popover_is_suppressed_when_latex_fails(cx: &mut TestAppContext) {
    let (_editor, _cx, st) = reveal_and_prepaint("x $a^$ y", cx);
    let art = &st.frame.snapshot.texts[0].art;
    assert!(
        math_parts(art).is_empty(),
        "the revealed row should hold no math slot"
    );
    assert!(
        st.math_popover.is_none(),
        "a formula that failed to parse should have no popover: {:?}",
        st.math_popover
    );
}

#[gpui::test]
fn revealed_display_math_stays_in_flow_like_mermaid(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("line1\nline2\n$$\n\\frac{a}{b}\n$$\n", cx);
    let math = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .find(|&b| view.state.doc.kind(b) == Some(BlockKind::Math))
            .expect("the math block")
    });
    let leaf = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .position(|b| b == math)
            .expect("leaf")
    });
    place_caret(&editor, cx, leaf, 0);
    let st = draw_state(&editor, cx);
    assert!(
        st.math_popover.is_none(),
        "a display math preview should not go through a popover: {:?}",
        st.math_popover
    );
    let pieces: Vec<_> = st
        .frame
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == math)
        .collect();
    assert_eq!(
        pieces.len(),
        2,
        "both the source and preview boxes should be published:kinds={:?}",
        pieces
            .iter()
            .map(|t| (t.kind, t.edit_source))
            .collect::<Vec<_>>()
    );
    let source = pieces
        .iter()
        .find(|t| t.edit_source)
        .expect("the source well");
    let preview = pieces
        .iter()
        .find(|t| !t.edit_source)
        .expect("the preview box");
    assert_eq!(
        source.kind,
        BlockKind::CodeBlock,
        "the source is laid out as a code block"
    );
    assert_eq!(preview.kind, BlockKind::Math, "the preview is still math");
    let text = joined_text(&source.art);
    assert_eq!(
        text, "\\frac{a}{b}",
        "the editable source should not carry the blank line after the opening $$"
    );
    assert!(
        !text.contains("line1") && !text.contains("line2"),
        "those two lines above must not enter the source well: {text:?}"
    );
    assert!(
        preview.content_origin_device.1 > source.content_origin_device.1,
        "the preview should sit below the source: source y={} preview y={}",
        source.content_origin_device.1,
        preview.content_origin_device.1
    );
    assert!(
        !math_parts(&preview.art).is_empty(),
        "the preview box should lay out a math slot, not an empty box waiting for the popover to paint"
    );
}

#[gpui::test]
fn math_block_source_edit_preview_is_in_flow_not_a_popover(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n", cx);
    let math = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .find(|&b| view.state.doc.kind(b) == Some(BlockKind::Math))
            .expect("the math block")
    });
    let leaf = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .position(|b| b == math)
            .expect("leaf")
    });
    place_caret(&editor, cx, leaf, 0);
    let st = draw_state(&editor, cx);
    assert!(
        st.math_popover.is_none(),
        "display math preview should not go through a popover:{:?}",
        st.math_popover
    );
    let pieces: Vec<_> = st
        .frame
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == math)
        .collect();
    assert_eq!(
        pieces.len(),
        2,
        "both the source and preview boxes should be published:kinds={:?}",
        pieces
            .iter()
            .map(|t| (t.kind, t.edit_source))
            .collect::<Vec<_>>()
    );
    let source = pieces
        .iter()
        .find(|t| t.edit_source)
        .expect("the source well");
    let preview = pieces
        .iter()
        .find(|t| !t.edit_source)
        .expect("the preview box");
    assert_eq!(
        source.kind,
        BlockKind::CodeBlock,
        "the source is laid out as a code block"
    );
    assert_eq!(
        preview.kind,
        BlockKind::Math,
        "the preview is still a math block"
    );
    assert_eq!(
        joined_text(&source.art),
        "\\frac{a}{b}",
        "the source well should not carry the blank line after the opening $$"
    );
    assert!(
        preview.content_origin_device.1 > source.content_origin_device.1,
        "the preview should sit below the source: source y={} preview y={}",
        source.content_origin_device.1,
        preview.content_origin_device.1
    );
    assert!(
        !math_parts(&preview.art).is_empty(),
        "the preview box should lay out a math slot, not an empty box waiting for the popover to paint"
    );
}

#[gpui::test]
fn revealed_display_math_keeps_preview_box_when_latex_fails(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("line1\nline2\n$$\na^\n$$\n", cx);
    let math = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .find(|&b| view.state.doc.kind(b) == Some(BlockKind::Math))
            .expect("the math block")
    });
    let leaf = cx.update(|_, app| {
        let view = editor.read(app);
        view.state
            .doc
            .text_leaves()
            .into_iter()
            .position(|b| b == math)
            .expect("leaf")
    });
    place_caret(&editor, cx, leaf, 0);
    cx.run_until_parked();
    let st = draw_state(&editor, cx);
    assert!(
        st.math_popover.is_none(),
        "a math block preview should not go through a popover:{:?}",
        st.math_popover
    );
    let pieces: Vec<_> = st
        .frame
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == math)
        .collect();
    assert_eq!(
        pieces.len(),
        2,
        "both the source and preview boxes should be published:kinds={:?}",
        pieces
            .iter()
            .map(|t| (t.kind, t.edit_source))
            .collect::<Vec<_>>()
    );
    let source = pieces
        .iter()
        .find(|t| t.edit_source)
        .expect("the source well");
    let text = joined_text(&source.art);
    assert!(
        text.contains("a^"),
        "the failed LaTeX is still in the source text: {text:?}"
    );
    assert!(
        !text.contains("line1") && !text.contains("line2"),
        "those two lines above must not enter the source well: {text:?}"
    );
}

fn leaf_of_kind(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    kind: BlockKind,
) -> (md_core::block::BlockId, usize) {
    cx.update(|_, app| {
        let view = editor.read(app);
        let leaves = view.state.doc.text_leaves();
        let block = leaves
            .iter()
            .copied()
            .find(|&b| view.state.doc.kind(b) == Some(kind))
            .unwrap_or_else(|| panic!("no {kind:?}"));
        let i = leaves.iter().position(|&b| b == block).expect("leaf");
        (block, i)
    })
}

fn assert_block_stays_rendered(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    kind: BlockKind,
) {
    let (block, _) = leaf_of_kind(editor, cx, kind);
    let editing = cx.update(|_, app| editor.read(app).state.doc.block_edit());
    assert_eq!(editing, None, "{kind:?}");
    let st = draw_state(editor, cx);
    let pieces: Vec<_> = st
        .frame
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == block)
        .collect();
    assert!(
        pieces.iter().all(|t| !t.edit_source),
        "{kind:?} still showing source: {:?}",
        pieces
            .iter()
            .map(|t| (t.kind, t.edit_source))
            .collect::<Vec<_>>()
    );
    assert_eq!(pieces.len(), 1, "{kind:?} should keep a single preview box");
}

fn select_from_first_para_to_block(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
    kind: BlockKind,
) {
    let (block, _) = leaf_of_kind(editor, cx, kind);
    place_caret(editor, cx, 0, 0);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Extend);
        })
    });
}

#[gpui::test]
fn selecting_across_a_math_block_keeps_the_preview(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n", cx);
    focus_editor(&editor, cx);
    select_from_first_para_to_block(&editor, cx, BlockKind::Math);
    assert_block_stays_rendered(&editor, cx, BlockKind::Math);
}

#[gpui::test]
fn selecting_across_a_mermaid_block_keeps_the_preview(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc(
        "before\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nafter\n",
        cx,
    );
    focus_editor(&editor, cx);
    select_from_first_para_to_block(&editor, cx, BlockKind::Mermaid);
    assert_block_stays_rendered(&editor, cx, BlockKind::Mermaid);
}

#[gpui::test]
fn selecting_across_an_image_block_keeps_the_preview(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("before\n\n![a](u)\n\nafter\n", cx);
    focus_editor(&editor, cx);
    select_from_first_para_to_block(&editor, cx, BlockKind::Image);
    assert_block_stays_rendered(&editor, cx, BlockKind::Image);
}

#[gpui::test]
fn pointer_press_on_a_math_block_does_not_enter_until_click(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n", cx);
    focus_editor(&editor, cx);
    let (block, _) = leaf_of_kind(&editor, cx, BlockKind::Math);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.place_pointer_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
        })
    });
    assert_block_stays_rendered(&editor, cx, BlockKind::Math);
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.place_cursor(
                Cursor {
                    block,
                    offset: view.state.doc.caret_text(block).map_or(0, |t| t.len()),
                },
                CursorMotion::Extend,
            );
        })
    });
    assert_block_stays_rendered(&editor, cx, BlockKind::Math);
}

#[gpui::test]
fn selecting_inside_an_open_math_well_keeps_source_edit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n", cx);
    focus_editor(&editor, cx);
    let (block, leaf) = leaf_of_kind(&editor, cx, BlockKind::Math);
    place_caret(&editor, cx, leaf, 0);
    let st = draw_state(&editor, cx);
    assert_eq!(
        st.frame
            .snapshot
            .texts
            .iter()
            .filter(|t| t.block == block)
            .count(),
        2,
        "clicking should open the source well"
    );
    let end = cx.update(|_, app| {
        editor
            .read(app)
            .state
            .doc
            .caret_text(block)
            .map_or(0, |t| t.len())
    });
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            view.place_cursor(Cursor { block, offset: end }, CursorMotion::Extend);
        })
    });
    let editing = cx.update(|_, app| editor.read(app).state.doc.block_edit());
    assert_eq!(editing, Some(block));
    let st = draw_state(&editor, cx);
    let pieces: Vec<_> = st
        .frame
        .snapshot
        .texts
        .iter()
        .filter(|t| t.block == block)
        .collect();
    assert!(
        pieces.iter().any(|t| t.edit_source),
        "in-well selection should keep the source box"
    );
}

#[test]
fn popover_pushes_left_when_it_would_overflow_the_right_edge() {
    let anchor = Anchor {
        x: 760.0,
        row_top: 300.0,
        row_bottom: 320.0,
    };
    let (left, _) =
        place(anchor, (100.0, 80.0), 6.0, (800.0, 600.0)).expect("the anchor row is visible");
    assert_eq!(
        left, 700.0,
        "when it overflows on the right it should be pushed left to the edge"
    );

    let (left, _) = place(anchor, (900.0, 80.0), 6.0, (800.0, 600.0))
        .expect("the anchor row should be visible");
    assert_eq!(left, 0.0);
}
