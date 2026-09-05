use super::support::{editor_with_doc, place_caret};
use crate::view::{EditorElement, EditorView};
use gpui::Entity;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::{EntityInputHandler, point, px, size};
use md_render::snapshot::DeviceRect;

fn revealed_search_rects(
    editor: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    query: &str,
) -> (Vec<DeviceRect>, Vec<DeviceRect>) {
    place_caret(editor, cx, 0, 1);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.set_search_query(query));
    });
    cx.run_until_parked();
    let (_, prepaint) = cx.draw(
        point(px(0.0), px(0.0)),
        size(px(800.0), px(600.0)),
        |_, _| EditorElement {
            state: editor.clone(),
        },
    );
    let snap = &prepaint.frame.snapshot;
    (
        snap.search_device.clone(),
        snap.search_active_device.clone(),
    )
}

#[gpui::test]
fn active_hit_on_a_revealed_block_gets_the_active_color(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("a**b**c\n", cx);
    let (rest, active) = revealed_search_rects(&editor, cx, "b");

    assert_eq!(
        active.len(),
        1,
        "the current hit must carry the active color"
    );
    assert!(
        rest.is_empty(),
        "the only hit is the current one, so the plain color must be empty"
    );
    let (x, y, w, h) = active[0];
    assert!(
        w > 0.0 && h > 0.0,
        "the active rect must really have an area, not a stack of zeros"
    );
    let _ = (x, y);
}

#[gpui::test]
fn empty_collapsed_scan_paints_no_highlights(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("a**b**c\n", cx);
    let (rest, active) = revealed_search_rects(&editor, cx, "**");

    assert!(
        rest.is_empty(),
        "a collapsed region must have zero hits: the ** in revealed text is markup, not a match"
    );
    assert!(active.is_empty(), "no hits, so there is no current hit");
}

#[gpui::test]
fn stepping_resumes_from_the_cursor_when_the_selection_left_the_hit(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("needle needle needle\n", cx);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.set_search_query("needle"));
    });
    cx.run_until_parked();
    let active = |cx: &mut VisualTestContext| cx.update(|_, app| editor.read(app).search.active);
    assert_eq!(
        active(cx),
        Some(0),
        "precondition: the first seeding stops at the first hit"
    );

    place_caret(&editor, cx, 0, 14);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.search_step(1));
    });
    cx.run_until_parked();
    assert_eq!(
        active(cx),
        Some(2),
        "\"next\" must take the first hit after the caret, not the one after the old active"
    );

    place_caret(&editor, cx, 0, 14);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.search_step(-1));
    });
    cx.run_until_parked();
    assert_eq!(
        active(cx),
        Some(1),
        "\"prev\" must step back to the second hit"
    );
}

#[gpui::test]
fn jumping_to_a_revealed_hit_selects_only_the_match(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("a**b**c\n", cx);

    place_caret(&editor, cx, 0, 1);
    cx.update(|_, app| {
        editor.update(app, |v, _| v.set_search_query("b"));
    });
    cx.run_until_parked();
    cx.update(|_, app| {
        editor.update(app, |v, _| {
            let leaf = v.state.doc.text_leaves()[0];
            assert_eq!(
                v.state.doc.text(leaf).unwrap(),
                "a**b**c",
                "precondition: after the jump the focused leaf must expose revealed text"
            );
            let (a, b) = v
                .state
                .selection
                .expect("the jump must select the hit word, so the selection is non-empty");
            assert_eq!(a.block, leaf);
            assert_eq!(b.block, leaf);
            let (lo, hi) = if a.offset <= b.offset {
                (a.offset, b.offset)
            } else {
                (b.offset, a.offset)
            };
            assert_eq!(
                v.state.doc.text(leaf).unwrap().get(lo..hi),
                Some("b"),
                "the selection must cover only the hit word, without the closing **"
            );
        })
    });
}

#[gpui::test]
fn a_search_jump_during_ime_interrupts_the_composition(cx: &mut TestAppContext) {
    let (editor, cx) = editor_with_doc("abc\n\nxyz\n", cx);
    cx.update(|window, app| {
        editor.update(app, |view, cx| {
            let leaves = view.state.doc.text_leaves();
            let (a, b) = (leaves[0], leaves[1]);
            view.place_cursor(
                md_core::doc::Cursor {
                    block: a,
                    offset: 3,
                },
                crate::view::CursorMotion::Move,
            );
            EntityInputHandler::replace_and_mark_text_in_range(view, None, "ni", None, window, cx);
            assert!(
                view.state.marked.is_some(),
                "precondition: the composition must be active"
            );
            assert_eq!(view.state.doc.text(a).unwrap(), "abcni");

            view.set_search_query("xyz");

            assert_eq!(
                view.state.marked, None,
                "a search jump must interrupt the composition"
            );
            assert!(!view.state.doc.is_composing());
            assert_eq!(
                view.state.doc.text(a).unwrap(),
                "abc",
                "the composition string must be dismissed"
            );
            assert_eq!(
                view.state.cursor.block, b,
                "the jump must carry the caret to the hit block"
            );
        })
    });
}
