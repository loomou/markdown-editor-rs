use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use crate::shell::outline::{
    OUTLINE_FOLLOW_ROWS, OUTLINE_MAX_W, OUTLINE_MIN_W, clamp_outline_width, outline_rows,
    outline_thumb,
};
use crate::shell::status::status_counts;
use crate::ui::theme::{OUTLINE_ROW_H, OUTLINE_W};
use gpui::TestAppContext;
use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use gpui::{point, px, size};
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};

#[gpui::test]
fn outline_scrolls_when_headings_overflow_the_panel(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..80 {
        md.push_str(&format!("## Heading {i}\n\nBody {i}\n\n"));
    }
    let (shell, cx) =
        cx.add_window_view(|_, cx| Shell::new(Doc::new(load_markdown(&md, editor_options())), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.outline_open = true;
            cx.notify();
        })
    });
    cx.run_until_parked();

    let (scroll, rows) = cx.update(|_, app| {
        let shell = shell.read(app);
        let editor = shell.editor.read(app);
        (
            shell.outline_scroll.clone(),
            outline_rows(&editor.state.doc).len(),
        )
    });
    assert_eq!(
        rows, 80,
        "all eighty headings should make it into the outline"
    );
    assert_eq!(
        scroll.item_count(),
        80,
        "the list state must be told about every row"
    );
    let room = scroll.max_offset_for_scrollbar().y;
    assert!(
        room > px(0.0),
        "an outline of {rows} rows should scroll: leftover room {room:?}"
    );

    let pos = scroll.viewport_bounds().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: pos,
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-240.0))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let scrolled = cx.update(|_, app| {
        let s = &shell.read(app).outline_scroll;
        let y = s.scroll_px_offset_for_scrollbar().y;
        let top = s.logical_scroll_top().item_ix;
        (y, top)
    });
    assert!(
        scrolled.0 < px(0.0),
        "the wheel did not scroll the outline: offset {:?}",
        scrolled.0
    );
    assert!(
        scrolled.1 > 0,
        "after scrolling it still rests on row {}",
        scrolled.1
    );
}

#[gpui::test]
fn outline_y_scroll_is_auto_when_headings_fit(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.outline_open = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (room, viewport) = cx.update(|_, app| {
        let s = &shell.read(app).outline_scroll;
        (s.max_offset_for_scrollbar().y, s.viewport_bounds().size)
    });
    assert!(
        room <= px(0.5),
        "a few headings should not overflow into a vertical scroll: room {room:?}"
    );
    assert!(
        f32::from(viewport.width) >= OUTLINE_W - 4.0,
        "the outline frame should be a fixed {OUTLINE_W}px, but the list viewport is {:?}",
        viewport.width
    );
}

#[test]
fn outline_thumb_hides_when_content_fits_and_tracks_offset() {
    assert_eq!(outline_thumb(400.0, 200.0, 0.0), None);
    let (y0, h0) = outline_thumb(400.0, 2000.0, 0.0).expect("overflow should show a thumb");
    let (y1, h1) =
        outline_thumb(400.0, 2000.0, -800.0).expect("after scrolling there is still a thumb");
    assert!((h0 - h1).abs() < 0.01);
    assert!(
        y1 > y0,
        "scrolling down should move the thumb down: {y0} -> {y1}"
    );
}

#[gpui::test]
fn outline_wraps_a_wide_heading_instead_of_scrolling_sideways(cx: &mut TestAppContext) {
    let long = "a very long outline heading ".repeat(20);
    let md = format!("# {long}\n\nBody\n");
    let (shell, cx) =
        cx.add_window_view(|_, cx| Shell::new(Doc::new(load_markdown(&md, editor_options())), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.outline_open = true;
            cx.notify();
        })
    });
    cx.run_until_parked();

    let (row, viewport, before) = cx.update(|_, app| {
        let s = &shell.read(app).outline_scroll;
        (
            s.bounds_for_item(0)
                .expect("the first row should have been laid out"),
            s.viewport_bounds(),
            s.scroll_px_offset_for_scrollbar(),
        )
    });
    assert!(
        row.size.height > px(OUTLINE_ROW_H),
        "a heading far wider than the panel must take more than one line: {:?}",
        row.size.height
    );
    assert!(
        row.size.width <= viewport.size.width + px(1.0),
        "the wrapped row must stay inside the panel: row {:?} viewport {:?}",
        row.size.width,
        viewport.size.width
    );
    assert!(
        cx.debug_bounds("outline-sb-x").is_none(),
        "the outline must not grow a horizontal scrollbar"
    );

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(-160.0), px(0.0))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let after = cx.update(|_, app| {
        shell
            .read(app)
            .outline_scroll
            .scroll_px_offset_for_scrollbar()
    });
    assert_eq!(
        after, before,
        "a sideways wheel must not move the outline: {before:?} -> {after:?}"
    );
}

#[test]
fn outline_width_clamps_to_min_half_viewport_and_max() {
    assert_eq!(clamp_outline_width(10.0, 1000.0), OUTLINE_MIN_W);
    assert_eq!(clamp_outline_width(OUTLINE_W, 1000.0), OUTLINE_W);
    assert_eq!(clamp_outline_width(999.0, 800.0), 400.0);
    assert_eq!(clamp_outline_width(999.0, 2000.0), OUTLINE_MAX_W);
}

fn open_outline(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.outline_open = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn set_scroll(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext, scroll: md_core::Px) {
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.state.scroll = scroll;
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
}

fn ten_heading_doc() -> Doc {
    let mut md = String::new();
    for i in 0..10 {
        md.push_str(&format!(
            "## Heading {i}\n\n{}\n\n",
            "Body text ".repeat(60)
        ));
    }
    Doc::new(load_markdown(&md, editor_options()))
}

#[gpui::test]
fn outline_highlight_follows_the_scrolled_heading(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(ten_heading_doc(), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);

    let rows = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        outline_rows(&editor.state.doc)
    });
    assert_eq!(rows.len(), 10);

    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[0].block)
    );

    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.jump_to_block(rows[5].block);
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    let top5 = cx.update(|_, app| shell.read(app).editor.read(app).state.resolved_top);
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[5].block),
        "the jump should stop at the top with the clicked row lit"
    );

    set_scroll(&shell, cx, top5 + 2.0);
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[5].block),
        "the lit row should not change before the viewport top reaches the next heading"
    );

    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.jump_to_block(rows[6].block);
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[6].block),
        "scrolling past the sixth heading's top edge should light it"
    );

    set_scroll(&shell, cx, top5 + 2.0);
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[5].block),
        "scrolling back up should light the previous heading again"
    );

    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.state.cursor = md_core::doc::Cursor {
                    block: rows[0].block,
                    offset: 0,
                };
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows[5].block),
        "moving the caret outside the viewport must not change the scroll-followed highlight"
    );
}

#[gpui::test]
fn dragging_the_outline_seam_changes_panel_width(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);

    let hit = cx
        .debug_bounds("outline-resize")
        .expect("the left edge should expose a drag-to-widen zone while the outline is open");
    assert!(
        (shell.read_with(cx, |s, _| s.outline_width) - OUTLINE_W).abs() < 0.5,
        "the width should start at the factory default"
    );

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: hit.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |s, _| s.outline_resize.is_some()),
        "pressing should enter the resize drag"
    );

    cx.simulate_event(MouseMoveEvent {
        position: point(hit.center().x - px(80.0), hit.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    let wide = shell.read_with(cx, |s, _| s.outline_width);
    assert!(
        (wide - (OUTLINE_W + 80.0)).abs() < 1.0,
        "dragging left should widen the panel: got {wide}"
    );

    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(hit.center().x - px(80.0), hit.center().y),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |s, _| s.outline_resize.is_none()),
        "releasing should clear the resize state"
    );

    cx.simulate_event(MouseMoveEvent {
        position: point(hit.center().x - px(160.0), hit.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_width),
        wide,
        "moving after release should not change the width"
    );
}

#[gpui::test]
fn dragging_the_outline_seam_stops_at_min_width(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);
    let hit = cx
        .debug_bounds("outline-resize")
        .expect("the left edge should expose a drag-to-widen zone while the outline is open");

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: hit.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: point(hit.center().x + px(400.0), hit.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_width),
        OUTLINE_MIN_W,
        "dragging right should stop at the minimum width"
    );
}

#[gpui::test]
fn outline_resize_state_does_not_survive_a_lost_mouse_up(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);
    let hit = cx
        .debug_bounds("outline-resize")
        .expect("the left edge should expose a drag-to-widen zone while the outline is open");

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: hit.center(),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: point(hit.center().x - px(80.0), hit.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    let wide = shell.read_with(cx, |s, _| s.outline_width);
    assert!(
        (wide - (OUTLINE_W + 80.0)).abs() < 1.0,
        "pressing and dragging should widen the panel: got {wide}"
    );

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: point(px(50.0), px(50.0)),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |s, _| s.outline_resize.is_none()),
        "a stale resize state should clear on the next press"
    );

    cx.simulate_event(MouseMoveEvent {
        position: point(hit.center().x - px(160.0), hit.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_width),
        wide,
        "a drag after the lost up should not change the width"
    );
}

fn outline_view(
    cx: &mut gpui::VisualTestContext,
    shell: &gpui::Entity<Shell>,
    rows: &[u32],
) -> (usize, std::ops::Range<usize>) {
    let (current, state) = cx.update(|_, app| {
        let s = shell.read(app);
        (s.outline_current, s.outline_scroll.clone())
    });
    let ix = rows
        .iter()
        .position(|&b| Some(b) == current)
        .expect("the current row must be in the list");
    let viewport = state.viewport_bounds();
    let start = state.logical_scroll_top().item_ix;
    let mut end = start;
    for i in start..rows.len() {
        let Some(bounds) = state.bounds_for_item(i) else {
            break;
        };
        if bounds.top() >= viewport.bottom() {
            break;
        }
        end = i + 1;
    }
    (ix, start..end.max(start + 1))
}

fn jump_to(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext, block: u32) {
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.jump_to_block(block);
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn outline_panel_keeps_the_current_row_visible(cx: &mut TestAppContext) {
    let mut md = String::new();
    for i in 0..80 {
        md.push_str(&format!(
            "## Heading {i}\n\n{}\n\n",
            "Body text ".repeat(30)
        ));
    }
    let (shell, cx) =
        cx.add_window_view(|_, cx| Shell::new(Doc::new(load_markdown(&md, editor_options())), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);

    let rows = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        outline_rows(&editor.state.doc)
    });
    let blocks: Vec<u32> = rows.iter().map(|r| r.block).collect();

    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 0);
    assert!(
        view.contains(&ix),
        "the first row should be in the panel's view at the start: {view:?}"
    );

    jump_to(&shell, cx, blocks[60]);
    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 60);
    assert!(
        view.contains(&ix),
        "the document scrolled to the middle, so the panel must follow the current row: {ix} is not in {view:?}"
    );

    assert_eq!(
        view.end - 1 - ix,
        OUTLINE_FOLLOW_ROWS,
        "following down should park the current row {OUTLINE_FOLLOW_ROWS} rows from the bottom: view {view:?}"
    );
    let top_at_60 = view.start;
    jump_to(&shell, cx, blocks[61]);
    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 61);
    assert_eq!(
        view.start,
        top_at_60 + 1,
        "continuing down should scroll one row at a time: view moved from {top_at_60:?} to {:?}",
        view.start
    );
    jump_to(&shell, cx, blocks[30]);
    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 30);
    assert_eq!(
        ix - view.start,
        OUTLINE_FOLLOW_ROWS,
        "following up should park the current row {OUTLINE_FOLLOW_ROWS} rows from the top: view {view:?}"
    );
    let top_at_30 = view.start;
    jump_to(&shell, cx, blocks[29]);
    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 29);
    assert_eq!(
        view.start,
        top_at_30 - 1,
        "continuing up should scroll one row at a time: view moved from {top_at_30:?} to {:?}",
        view.start
    );
    let top_at_29 = view.start;
    jump_to(&shell, cx, blocks[31]);
    let (ix, view) = outline_view(cx, &shell, &blocks);
    assert_eq!(ix, 31);
    assert_eq!(
        view.start, top_at_29,
        "the one-row scroll should be spared when the current row sits far from both edges: view {view:?}"
    );

    let pos = shell.read_with(cx, |s, _| s.outline_scroll.viewport_bounds().center());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: pos,
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(96.0))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    cx.run_until_parked();
    let after_user = shell.read_with(cx, |s, _| {
        f32::from(s.outline_scroll.scroll_px_offset_for_scrollbar().y)
    });
    let (_, view) = outline_view(cx, &shell, &blocks);
    assert!(
        view.contains(&31),
        "after scrolling up a few rows the current row should still be visible: {view:?}"
    );
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.state.cursor = md_core::doc::Cursor {
                    block: blocks[0],
                    offset: 0,
                };
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |s, _| f32::from(
            s.outline_scroll.scroll_px_offset_for_scrollbar().y
        )),
        after_user,
        "with the current row visible, a repaint should not reclaim the user-scrolled panel"
    );
}

fn three_heading_doc(label: &str, body_reps: usize) -> Doc {
    let body = "Body text ".repeat(body_reps);
    let md = format!(
        "# {label} No. 1\n\n{body}\n\n## {label} No. 2\n\n{body}\n\n## {label} No. 3\n\n{body}\n\n"
    );
    Doc::new(load_markdown(&md, editor_options()))
}

fn replace_with(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext, doc: Doc) {
    cx.update(|_, app| {
        shell.update(app, |shell, cx| {
            shell.editor.update(cx, |view, cx| {
                view.replace_document(doc, cx);
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn outline_switches_to_the_newly_opened_document(cx: &mut TestAppContext) {
    let mut md_a = String::new();
    for i in 0..80 {
        md_a.push_str(&format!(
            "## Heading A {i}\n\n{}\n\n",
            "Body text ".repeat(30)
        ));
    }
    let (shell, cx) = cx
        .add_window_view(|_, cx| Shell::new(Doc::new(load_markdown(&md_a, editor_options())), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);

    let rows_a = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        outline_rows(&editor.state.doc)
    });
    jump_to(&shell, cx, rows_a[60].block);
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows_a[60].block)
    );
    let scrolled = shell.read_with(cx, |s, _| {
        f32::from(s.outline_scroll.scroll_px_offset_for_scrollbar().y)
    });
    assert!(
        scrolled < -OUTLINE_ROW_H * 8.0,
        "the panel should be scrolled deep before the document switch: {scrolled}"
    );

    let mut md_b = String::from("A lead-in paragraph before the next document.\n\n");
    for i in 0..80 {
        md_b.push_str(&format!(
            "## Heading B {i}\n\n{}\n\n",
            "Body text ".repeat(30)
        ));
    }
    replace_with(&shell, cx, Doc::new(load_markdown(&md_b, editor_options())));

    let rows_b = cx.update(|_, app| {
        let editor = shell.read(app).editor.read(app);
        outline_rows(&editor.state.doc)
    });
    assert_eq!(rows_b.len(), 80);
    assert_ne!(
        rows_b[0].block, rows_a[0].block,
        "the first heading blocks of the two documents must have distinct ids"
    );
    let labels: Vec<String> = shell.read_with(cx, |s, _| {
        s.outline_cache
            .rows
            .iter()
            .map(|r| r.label.clone())
            .collect()
    });
    assert_eq!(labels.len(), 80);
    assert!(
        labels.iter().all(|l| l.starts_with("Heading B")),
        "the panel still holds rows of the previous document: {labels:?}"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.outline_current),
        Some(rows_b[0].block),
        "the highlight should move to the new document's first row"
    );
    let top = shell.read_with(cx, |s, _| {
        f32::from(s.outline_scroll.scroll_px_offset_for_scrollbar().y)
    });
    assert_eq!(
        top, 0.0,
        "after switching documents the panel should return to the top"
    );
}

#[gpui::test]
fn shell_caches_follow_the_replaced_document_when_the_revision_collides(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(three_heading_doc("A", 10), cx));
    stop_blink(&shell, cx);
    open_outline(&shell, cx);

    let counts_a = cx.update(|_, app| {
        let s = shell.read(app);
        let editor = s.editor.read(app);
        status_counts(&editor.state.doc, editor.state.cursor)
    });
    assert_eq!(shell.read_with(cx, |s, _| s.status_cache.counts), counts_a);
    assert!(shell.read_with(cx, |s, _| {
        s.outline_cache.rows.iter().all(|r| r.label.contains("A"))
    }));

    replace_with(&shell, cx, three_heading_doc("B", 20));

    let counts_b = cx.update(|_, app| {
        let s = shell.read(app);
        let editor = s.editor.read(app);
        status_counts(&editor.state.doc, editor.state.cursor)
    });
    assert!(
        counts_b.chars > counts_a.chars,
        "the two body lengths must differ clearly for the assertion to discriminate"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.status_cache.counts),
        counts_b,
        "the status bar still shows the previous document's character count"
    );
    let labels: Vec<String> = shell.read_with(cx, |s, _| {
        s.outline_cache
            .rows
            .iter()
            .map(|r| r.label.clone())
            .collect()
    });
    assert!(
        labels.iter().all(|l| l.contains("B")),
        "the outline still holds rows of the previous document: {labels:?}"
    );
}

#[test]
fn outline_label_ignores_the_revealed_form_under_the_caret() {
    let mut doc = Doc::new(load_markdown(
        "## 用 `cargo fmt` 格式化\n\nbody\n",
        editor_options(),
    ));
    let block = doc.text_leaves()[0];
    let before = doc.document.revision();
    doc.retarget_focus(Cursor { block, offset: 4 });
    assert_ne!(
        doc.document.revision(),
        before,
        "the fixture must bump the revision for this test to discriminate"
    );
    let id = doc.document.live_id(block).expect("live heading");
    assert!(
        doc.document.display(id).contains('`'),
        "the caret should reveal the code span: {:?}",
        doc.document.display(id)
    );
    assert_eq!(outline_rows(&doc)[0].label, "用 cargo fmt 格式化");
}
