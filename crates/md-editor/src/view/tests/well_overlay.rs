use super::support::{editor_with_doc, focus_editor};
use crate::view::paint::{collect_well_hits, overlay_in_well};
use crate::view::{CursorMotion, Direction, EditorElement, EditorView, PendingPage, WellScroll};
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{Modifiers, ScrollWheelEvent, point, px, size};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_render::snapshot::TextPiece;

const NEEDLE_LINE: usize = 100;

fn long_code_md() -> String {
    let mut body = String::new();
    for i in 0..120 {
        if i == NEEDLE_LINE {
            body.push_str("needle here\n");
        } else {
            body.push_str(&format!("let x{i} = 1;\n"));
        }
    }
    format!("```rust\n{body}```\n\n{}", "tail\n\n".repeat(40))
}

fn draw(
    editor: &gpui::Entity<EditorView>,
    cx: &mut VisualTestContext,
) -> crate::view::PrepaintState {
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    )
    .1
}

fn code_piece(snap: &md_render::snapshot::LayoutSnapshot) -> TextPiece {
    snap.texts
        .iter()
        .find(|t| t.kind == BlockKind::CodeBlock && !t.edit_source)
        .cloned()
        .expect("the code block must be in the visible window")
}

fn wheel_over_well(piece: &TextPiece, cx: &mut VisualTestContext) {
    let (ox, oy) = piece.content_origin_device;
    cx.simulate_event(ScrollWheelEvent {
        position: point(
            px((ox + piece.content_width * 0.5) as f32),
            px((oy + piece.view_height * 0.5) as f32),
        ),
        delta: gpui::ScrollDelta::Lines(point(0.0, -10.0)),
        ..Default::default()
    });
}

fn well_y(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext, block: u32) -> Px {
    cx.update(|_, app| {
        editor
            .read(app)
            .well_scroll
            .get(&block)
            .copied()
            .unwrap_or_default()
            .y
    })
}

fn page_scroll(editor: &gpui::Entity<EditorView>, cx: &mut VisualTestContext) -> Px {
    cx.update(|_, app| editor.read(app).state.scroll)
}

fn rects_outside_the_card(
    rects: &[md_render::snapshot::OwnedRect],
    piece: &TextPiece,
    wells: &[crate::view::WellHit],
    scrolls: &std::collections::HashMap<u32, WellScroll>,
) -> Vec<(Px, Px, Px, Px)> {
    let top = piece.content_origin_device.1;
    let bottom = top + piece.view_height;
    rects
        .iter()
        .filter_map(|r| overlay_in_well(r.rect, r.block, wells, scrolls))
        .filter(|r| r.1 < top - 0.5 || r.1 + r.3 > bottom + 0.5)
        .collect()
}

#[gpui::test]
fn clicking_inside_a_scrolled_well_does_not_move_the_page(cx: &mut TestAppContext) {
    let md = long_code_md();
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let first = draw(&editor, cx);
    let piece = code_piece(&first.frame.snapshot);
    wheel_over_well(&piece, cx);
    let _ = draw(&editor, cx);
    let scrolled = well_y(&editor, cx, piece.block);
    assert!(
        scrolled > 0.0,
        "the fixture must scroll the well: {scrolled}"
    );

    let (ox, oy) = piece.content_origin_device;
    cx.simulate_click(
        point(
            px((ox + 40.0) as f32),
            px((oy + piece.view_height * 0.5) as f32),
        ),
        Modifiers::none(),
    );
    let _ = draw(&editor, cx);
    let _ = draw(&editor, cx);

    assert_eq!(
        page_scroll(&editor, cx),
        0.0,
        "the caret sits inside the well, so the page must stay put"
    );
    assert_eq!(
        well_y(&editor, cx, piece.block),
        scrolled,
        "the click must not disturb the well's own scroll"
    );
}

#[gpui::test]
fn select_all_in_a_lone_code_block_paints_nothing_below_the_card(cx: &mut TestAppContext) {
    let md = format!("```rust\n{}```\n", "let x = 1;\n".repeat(120));
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let first = draw(&editor, cx);
    let piece = code_piece(&first.frame.snapshot);
    let leaves = cx.update(|_, app| editor.read(app).state.doc.text_leaves());
    let last = *leaves.last().expect("a trailing leaf");
    cx.update(|_, app| {
        editor.update(app, |view, _| {
            let from = Cursor {
                block: leaves[0],
                offset: 0,
            };
            let len = view.state.doc.text(last).map_or(0, |t| t.len());
            let to = Cursor {
                block: last,
                offset: len,
            };
            view.state.selection = Some((from, to));
            view.state.cursor = to;
        });
    });
    let p = draw(&editor, cx);
    let snap = &p.frame.snapshot;
    let theme = cx.update(|_, app| editor.read(app).state.theme);
    let wells = collect_well_hits(&snap.texts, &snap.decorations, &theme);
    let scrolls = cx.update(|_, app| editor.read(app).well_scroll.clone());
    let leaked = rects_outside_the_card(&snap.selection_device, &piece, &wells, &scrolls);
    assert!(
        leaked.is_empty(),
        "a select-all must not paint selection below the clamped card, leaked {leaked:?}"
    );
}

#[gpui::test]
fn a_search_hit_below_the_well_view_paints_nothing(cx: &mut TestAppContext) {
    let mut body = String::new();
    for i in 0..120 {
        if i == 20 {
            body.push_str("needle here\n");
        } else {
            body.push_str(&format!("let x{i} = 1;\n"));
        }
    }
    let md = format!("```rust\n{body}```\n");
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let first = draw(&editor, cx);
    let piece = code_piece(&first.frame.snapshot);
    let (block, start) = cx.update(|_, app| {
        let v = editor.read(app);
        let text = v.state.doc.text(piece.block).unwrap_or("");
        (
            piece.block,
            text.find("needle").expect("needle in the code block"),
        )
    });
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            v.search.query = "needle".into();
            v.search.matches = vec![md_render::search::SearchMatch {
                block,
                start,
                end: start + 6,
            }];
            v.search.active = Some(0);
            v.search.reveal = None;
        });
    });
    let p = draw(&editor, cx);
    let snap = &p.frame.snapshot;
    let theme = cx.update(|_, app| editor.read(app).state.theme);
    let wells = collect_well_hits(&snap.texts, &snap.decorations, &theme);
    let scrolls = cx.update(|_, app| editor.read(app).well_scroll.clone());
    let leaked = rects_outside_the_card(&snap.search_active_device, &piece, &wells, &scrolls);
    assert!(
        leaked.is_empty(),
        "a hit scrolled out of the well must not paint outside it, leaked {leaked:?}"
    );
}

#[gpui::test]
fn revealing_a_search_hit_inside_a_well_scrolls_the_well_not_the_page(cx: &mut TestAppContext) {
    let md = long_code_md();
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let first = draw(&editor, cx);
    let piece = code_piece(&first.frame.snapshot);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.set_search_query("needle"));
    });
    let _ = draw(&editor, cx);
    let _ = draw(&editor, cx);
    let p = draw(&editor, cx);
    let snap = &p.frame.snapshot;
    let well = well_y(&editor, cx, piece.block);
    let page = page_scroll(&editor, cx);
    assert!(
        well > 0.0,
        "the hit sits past the well's first page, so revealing it must scroll the well"
    );
    let piece_now = code_piece(snap);
    assert!(
        piece_now.content_origin_device.1 < 600.0
            && piece_now.content_origin_device.1 + piece_now.view_height > 0.0,
        "the card must stay in the viewport, page scrolled to {page}"
    );
    let theme = cx.update(|_, app| editor.read(app).state.theme);
    let wells = collect_well_hits(&snap.texts, &snap.decorations, &theme);
    let scrolls = cx.update(|_, app| editor.read(app).well_scroll.clone());
    let leaked = rects_outside_the_card(&snap.search_active_device, &piece_now, &wells, &scrolls);
    assert!(leaked.is_empty(), "the revealed hit leaked {leaked:?}");
}

#[gpui::test]
fn page_down_inside_a_well_keeps_the_caret_in_the_code_block(cx: &mut TestAppContext) {
    let md = long_code_md();
    let (editor, cx) = editor_with_doc(&md, cx);
    focus_editor(&editor, cx);
    let first = draw(&editor, cx);
    let piece = code_piece(&first.frame.snapshot);
    wheel_over_well(&piece, cx);
    let _ = draw(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            let block = v.state.doc.text_leaves()[0];
            v.state.cursor = Cursor { block, offset: 410 };
            v.follow_caret = false;
            v.state.scroll = 0.0;
        });
    });
    let _ = draw(&editor, cx);
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            v.pending_page = Some(PendingPage {
                dir: Direction::Next,
                motion: CursorMotion::Move,
                anchor: None,
            });
        });
    });
    let _ = draw(&editor, cx);
    let _ = draw(&editor, cx);
    let landed = cx.update(|_, app| editor.read(app).state.cursor.block);
    assert_eq!(
        landed, piece.block,
        "page down must keep the caret in the code block it started in"
    );
}
