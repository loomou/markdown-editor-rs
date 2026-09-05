use super::support::{chrome, editor_with_doc};
use crate::view::scrollbar::{
    park_block_top_margin, park_block_top_scroll, search_reveal_scroll, select_autoscroll_can_move,
    select_autoscroll_delta,
};
use crate::view::{EditorElement, EditorView, ScrollbarGeom, WellBar, WellHit, WellScroll};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{point, px, size};
use md_core::block::BlockKind;

#[test]
fn scrollbar_hidden_when_content_fits() {
    let c = chrome();
    assert!(ScrollbarGeom::layout(800.0, 100.0, 0.0, 80.0, &c).is_none());
    assert!(ScrollbarGeom::layout(800.0, 100.0, 0.0, 100.0, &c).is_none());
}

#[test]
fn scrollbar_rejects_non_finite_geometry() {
    let c = chrome();
    assert!(ScrollbarGeom::layout(f64::NAN, 100.0, 0.0, 400.0, &c).is_none());
    assert!(ScrollbarGeom::layout(800.0, f64::NAN, 0.0, 400.0, &c).is_none());
    assert!(ScrollbarGeom::layout(800.0, 100.0, 0.0, f64::NAN, &c).is_none());
}

#[test]
fn scrollbar_thumb_tracks_scroll_ends() {
    let c = chrome();
    let top = ScrollbarGeom::layout(800.0, 100.0, 0.0, 400.0, &c).expect("overflow");
    assert!((top.thumb_y - top.track_y).abs() < 1e-4);
    let bottom = ScrollbarGeom::layout(800.0, 100.0, 300.0, 400.0, &c).expect("overflow");
    let bottom_edge = bottom.thumb_y + bottom.thumb_h;
    let track_edge = bottom.track_y + bottom.track_h;
    assert!((bottom_edge - track_edge).abs() < 1e-4);
    assert!((top.track_y - c.scrollbar_pad).abs() < 1e-4);
}

#[test]
fn scrollbar_pointer_maps_back_to_scroll() {
    let c = chrome();
    let bar = ScrollbarGeom::layout(800.0, 100.0, 150.0, 400.0, &c).expect("overflow");
    let recovered = bar.scroll_for_pointer(bar.thumb_y + bar.thumb_h * 0.5, bar.thumb_h * 0.5);
    assert!((recovered - 150.0).abs() < 0.5);
    let at_top = bar.scroll_for_pointer(bar.track_y, 0.0);
    assert!(at_top.abs() < 1e-4);
    let at_bottom = bar.scroll_for_pointer(bar.track_y + bar.track_h, bar.thumb_h);
    assert!((at_bottom - bar.max_scroll).abs() < 1e-4);
}

#[test]
fn well_bar_hidden_when_content_fits() {
    let c = chrome();
    assert!(WellBar::vertical(400.0, 200.0, 0.0, 200.0, &c).is_none());
    assert!(WellBar::vertical(400.0, 200.0, 0.0, 180.0, &c).is_none());
    assert!(WellBar::horizontal(400.0, 200.0, 0.0, 400.0, &c).is_none());
    assert!(WellBar::horizontal(400.0, 200.0, 0.0, 300.0, &c).is_none());
}

#[test]
fn well_bar_shown_when_content_overflows() {
    let c = chrome();
    let v = WellBar::vertical(400.0, 200.0, 0.0, 500.0, &c).expect("y overflow");
    assert!(v.vertical);
    let h = WellBar::horizontal(400.0, 200.0, 0.0, 800.0, &c).expect("x overflow");
    assert!(!h.vertical);
}

#[test]
fn well_scroll_axis_write_leaves_the_other_axis_alone() {
    let s = WellScroll { x: 3.0, y: 7.0 };
    let moved_y = s.with_axis(true, 99.0);
    assert_eq!(
        moved_y.y, 99.0,
        "the vertical axis should have been replaced"
    );
    assert_eq!(
        moved_y.x, 3.0,
        "writing the vertical axis should not touch the horizontal axis"
    );
    let moved_x = s.with_axis(false, 99.0);
    assert_eq!(
        moved_x.x, 99.0,
        "the horizontal axis should have been replaced"
    );
    assert_eq!(
        moved_x.y, 7.0,
        "writing the horizontal axis should not touch the vertical axis"
    );
    assert_eq!(s.along_axis(true), 7.0);
    assert_eq!(s.along_axis(false), 3.0);

    assert_eq!(s.with_axis(true, 42.0).along_axis(true), 42.0);
    assert_eq!(s.with_axis(false, 42.0).along_axis(false), 42.0);
}

#[test]
fn well_bar_axis_pickers_follow_the_bar_direction() {
    let c = chrome();
    let v = WellBar::vertical(400.0, 200.0, 0.0, 500.0, &c).expect("y overflow");
    assert_eq!(v.along(11.0, 22.0), 22.0, "the vertical bar should read y");
    assert_eq!(v.thumb_start(), v.thumb_y);
    assert_eq!(v.thumb_len(), v.thumb_h);
    let h = WellBar::horizontal(400.0, 200.0, 0.0, 800.0, &c).expect("x overflow");
    assert_eq!(
        h.along(11.0, 22.0),
        11.0,
        "the horizontal bar should read x"
    );
    assert_eq!(h.thumb_start(), h.thumb_x);
    assert_eq!(h.thumb_len(), h.thumb_w);
}

#[test]
fn well_bar_on_axis_matches_the_explicit_constructors() {
    let c = chrome();

    let hit = WellHit {
        id: 0,
        x: 0.0,
        y: 0.0,
        view_w: 400.0,
        view_h: 200.0,
        content_w: 800.0,
        content_h: 500.0,
    };
    let s = WellScroll { x: 120.0, y: 100.0 };
    let dispatched_v = WellBar::on_axis(true, &hit, s, &c).expect("y overflow");
    let explicit_v = WellBar::vertical(400.0, 200.0, 100.0, 500.0, &c).expect("y overflow");
    assert!(dispatched_v.vertical);
    assert_eq!(dispatched_v.thumb_y, explicit_v.thumb_y);
    assert_eq!(dispatched_v.thumb_h, explicit_v.thumb_h);
    let dispatched_h = WellBar::on_axis(false, &hit, s, &c).expect("x overflow");
    let explicit_h = WellBar::horizontal(400.0, 200.0, 120.0, 800.0, &c).expect("x overflow");
    assert!(!dispatched_h.vertical);
    assert_eq!(dispatched_h.thumb_x, explicit_h.thumb_x);
    assert_eq!(dispatched_h.thumb_w, explicit_h.thumb_w);
}

#[test]
fn well_bar_pointer_maps_back_to_scroll() {
    let c = chrome();
    let v = WellBar::vertical(400.0, 200.0, 100.0, 500.0, &c).expect("y");
    let recovered = v.scroll_for_pointer(v.thumb_y + v.thumb_h * 0.5, v.thumb_h * 0.5);
    assert!((recovered - 100.0).abs() < 0.5);
    let h = WellBar::horizontal(400.0, 200.0, 120.0, 800.0, &c).expect("x");
    let recovered = h.scroll_for_pointer(h.thumb_x + h.thumb_w * 0.5, h.thumb_w * 0.5);
    assert!((recovered - 120.0).abs() < 0.5);
}

#[test]
fn select_autoscroll_idle_in_middle() {
    let c = chrome();
    assert_eq!(select_autoscroll_delta(200.0, 400.0, &c), 0.0);
    assert_eq!(
        select_autoscroll_delta(c.select_autoscroll_edge, 400.0, &c),
        0.0
    );
    assert_eq!(
        select_autoscroll_delta(400.0 - c.select_autoscroll_edge, 400.0, &c),
        0.0
    );
}

#[test]
fn select_autoscroll_grows_toward_and_past_edges() {
    let c = chrome();
    let vh = 400.0;
    let top_inner = select_autoscroll_delta(c.select_autoscroll_edge * 0.5, vh, &c);
    let top_edge = select_autoscroll_delta(0.0, vh, &c);
    let top_out = select_autoscroll_delta(-c.select_autoscroll_outside * 0.5, vh, &c);
    let top_far = select_autoscroll_delta(-c.select_autoscroll_outside, vh, &c);
    let top_past = select_autoscroll_delta(-c.select_autoscroll_outside - 40.0, vh, &c);
    assert!(top_inner < 0.0);
    assert!(top_edge < top_inner);
    assert!((top_inner + c.select_autoscroll_edge_px * 0.25).abs() < 1e-9);
    assert!((top_edge + c.select_autoscroll_edge_px).abs() < 1e-9);
    assert!(top_out < top_edge);
    assert_eq!(top_far, top_past);
    assert!((top_far + c.select_autoscroll_max_px).abs() < 1e-9);
    let bot_inner = select_autoscroll_delta(vh - c.select_autoscroll_edge * 0.5, vh, &c);
    let bot_edge = select_autoscroll_delta(vh, vh, &c);
    let bot_out = select_autoscroll_delta(vh + c.select_autoscroll_outside * 0.5, vh, &c);
    let bot_far = select_autoscroll_delta(vh + c.select_autoscroll_outside, vh, &c);
    assert!(bot_inner > 0.0);
    assert!(bot_edge > bot_inner);
    assert!((bot_inner - c.select_autoscroll_edge_px * 0.25).abs() < 1e-9);
    assert!((bot_edge - c.select_autoscroll_edge_px).abs() < 1e-9);
    assert!(bot_out > bot_edge);
    assert!((bot_far - c.select_autoscroll_max_px).abs() < 1e-9);
}

#[test]
fn select_autoscroll_stops_at_document_ends() {
    assert!(!select_autoscroll_can_move(-4.0, 0.0, 100.0, 400.0));
    assert!(select_autoscroll_can_move(4.0, 0.0, 100.0, 400.0));
    assert!(!select_autoscroll_can_move(4.0, 300.0, 100.0, 400.0));
    assert!(select_autoscroll_can_move(-4.0, 300.0, 100.0, 400.0));
    assert!(!select_autoscroll_can_move(4.0, 0.0, 100.0, 80.0));
}

#[test]
fn search_reveal_keeps_scroll_when_match_is_comfortable() {
    let c = chrome();
    let vh = 400.0;
    let lh = 20.0;
    let scroll = 200.0;
    let margin = (c.search_comfort_lines * lh).max(c.search_comfort_vh * vh);
    let y0 = scroll + margin + 10.0;
    let y1 = y0 + lh;
    assert_eq!(
        search_reveal_scroll(y0, y1, scroll, vh, 4000.0, lh, 1, &c),
        None
    );
    assert_eq!(
        search_reveal_scroll(y0, y1, scroll, vh, 4000.0, lh, -1, &c),
        None
    );
}

#[test]
fn search_reveal_parks_by_direction_when_outside_comfort() {
    let c = chrome();
    let vh = 400.0;
    let lh = 20.0;
    let total = 4000.0;
    let y0 = 2000.0;
    let y1 = 2020.0;
    let down = search_reveal_scroll(y0, y1, 0.0, vh, total, lh, 1, &c).expect("next");
    assert!((down - (y0 - c.search_park_next * vh)).abs() < 1e-9);
    let up = search_reveal_scroll(y0, y1, 0.0, vh, total, lh, -1, &c).expect("prev");
    assert!((up - (y1 - c.search_park_prev * vh)).abs() < 1e-9);
}

#[test]
fn search_reveal_scrolls_edge_band_even_if_visible() {
    let c = chrome();
    let vh = 400.0;
    let lh = 20.0;
    let scroll = 0.0;
    let y0 = vh - 10.0;
    let y1 = y0 + lh;
    let next = search_reveal_scroll(y0, y1, scroll, vh, 4000.0, lh, 1, &c);
    assert_eq!(next, Some(y0 - c.search_park_next * vh));
}

#[test]
fn search_reveal_clamps_to_document() {
    let c = chrome();
    let vh = 400.0;
    let lh = 20.0;
    let total = 450.0;
    let y0 = 420.0;
    let y1 = 440.0;
    let next = search_reveal_scroll(y0, y1, 0.0, vh, total, lh, 1, &c).expect("clamp");
    assert!((next - (total - vh)).abs() < 1e-9);
    let prev = search_reveal_scroll(10.0, 30.0, 50.0, vh, total, lh, -1, &c).expect("top");
    assert!(prev.abs() < 1e-9);
}

#[test]
fn search_reveal_rejects_non_finite_geometry() {
    let c = chrome();
    assert_eq!(
        search_reveal_scroll(f64::NAN, 20.0, 0.0, 400.0, 4000.0, 20.0, 1, &c),
        None
    );
    assert_eq!(
        search_reveal_scroll(20.0, 40.0, f64::NAN, 400.0, 4000.0, 20.0, 1, &c),
        None
    );
    assert_eq!(
        search_reveal_scroll(20.0, 40.0, 0.0, 400.0, f64::NAN, 20.0, 1, &c),
        None
    );
}

#[test]
fn park_block_top_margin_is_a_bit_less_than_paragraph_gap() {
    assert_eq!(park_block_top_margin(20.0), 16.0);
    assert_eq!(park_block_top_margin(0.0), 0.0);
    assert_eq!(park_block_top_margin(f64::NAN), 0.0);
}

#[test]
fn park_block_top_scrolls_heading_to_viewport_top() {
    assert_eq!(
        park_block_top_scroll(2000.0, 0.0, 400.0, 4000.0, 40.0),
        Some(1960.0)
    );
    assert_eq!(
        park_block_top_scroll(350.0, 0.0, 400.0, 4000.0, 40.0),
        Some(310.0)
    );
    assert_eq!(park_block_top_scroll(0.0, 0.0, 400.0, 4000.0, 40.0), None);
    assert_eq!(
        park_block_top_scroll(3900.0, 0.0, 400.0, 4000.0, 40.0),
        Some(3600.0)
    );
    assert_eq!(
        park_block_top_scroll(f64::NAN, 0.0, 400.0, 4000.0, 40.0),
        None
    );
    assert_eq!(
        park_block_top_scroll(2000.0, 0.0, 400.0, 4000.0, f64::NAN),
        None
    );
}

#[gpui::test]
fn outline_jump_parks_heading_at_viewport_top(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..40 {
        md.push_str(&format!("## Heading {i}\n\n{}\n\n", "word ".repeat(40)));
    }
    let (editor, cx) = editor_with_doc(&md, cx);
    let headings = cx.update(|_, app| {
        let doc = &editor.read(app).state.doc.document;
        doc.preorder()
            .into_iter()
            .filter_map(|id| match doc.arena.get(id)?.kind {
                BlockKind::Heading(_) => Some(id.index),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert!(headings.len() >= 8);
    let target = headings[7];
    let draw = |cx: &mut VisualTestContext, editor: &gpui::Entity<EditorView>| {
        cx.draw(
            point(px(0.0), px(0.0)),
            size(px(800.0), px(480.0)),
            |_, _| EditorElement {
                state: editor.clone(),
            },
        )
        .1
        .frame
        .snapshot
    };
    let first = draw(cx, &editor);
    let before_y = first.caret_device.expect("caret").1;
    cx.update(|_, app| {
        editor.update(app, |view, _| view.jump_to_block(target));
    });
    let snap = draw(cx, &editor);
    let caret_y = snap.caret_device.expect("caret").1;
    assert!(
        (8.0..20.0).contains(&caret_y),
        "the heading should sit at the top after an outline jump, but the caret is at y={caret_y} (before the jump {before_y})"
    );
}
