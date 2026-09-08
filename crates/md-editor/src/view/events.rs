use super::media_zoom::{self, MediaHit};
use super::{CursorMotion, EditorView, PendingClick, ScrollbarGeom, WellBar, WellHit};
use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent, Window};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_core::document::{Command, Sel};
use md_render::query::hit_list_item_slot;
use md_render::snapshot::LayoutSnapshot;
use md_theme::ChromeTokens;
use std::rc::Rc;

pub(super) struct InputFrame {
    pub(super) state: gpui::Entity<EditorView>,
    pub(super) hitbox: gpui::Hitbox,
    pub(super) focus: gpui::FocusHandle,
    pub(super) origin: (f32, f32),
    pub(super) viewport: (Px, Px),
    pub(super) total_height: Px,
    pub(super) painted_scroll: Px,
    pub(super) chrome: ChromeTokens,
    pub(super) snapshot: Rc<LayoutSnapshot>,
    pub(super) geometry_revision: u64,
    pub(super) wells: Rc<Vec<WellHit>>,
    pub(super) cells: Rc<Vec<super::table_cols::CellHit>>,
    pub(super) scale: f64,
    pub(super) media_hits: Rc<Vec<MediaHit>>,
}

pub(super) fn on_mouse_down(f: &InputFrame, window: &mut Window) {
    let hb = f.hitbox.clone();
    let ent = f.state.clone();
    let fh = f.focus.clone();
    let (ox, oy) = f.origin;
    let vw = f.viewport.0;
    let vh = f.viewport.1;
    let total = f.total_height;
    let painted_scroll = f.painted_scroll;
    let bar_chrome = f.chrome;
    let slot_snap = Rc::clone(&f.snapshot);
    let slot_rev = f.geometry_revision;
    let wells = Rc::clone(&f.wells);
    let cell_hits_down = Rc::clone(&f.cells);
    let scale = f.scale;
    window.on_mouse_event(move |ev: &MouseDownEvent, phase, win, cx| {
        if !phase.bubble() {
            return;
        }
        if ev.button != MouseButton::Left && ev.button != MouseButton::Right {
            return;
        }
        let local = (
            (f32::from(ev.position.x) - ox) as Px,
            (f32::from(ev.position.y) - oy) as Px,
        );
        let mut zooming = false;
        ent.update(cx, |v, cx| {
            if v.media_zoom.is_some() {
                zooming = true;
                if ev.button == MouseButton::Left {
                    v.media_zoom_press(local, (vw, vh), scale, cx);
                } else {
                    v.close_media_zoom(cx);
                }
                cx.stop_propagation();
            }
        });
        if zooming {
            return;
        }
        if ev.button == MouseButton::Right {
            let mut overlay_hit = false;
            ent.update(cx, |v, cx| {
                if let Some(chrome) = v.table_ui.chrome
                    && super::table_toolbar::overlay_contains(
                        chrome,
                        v.state.scroll,
                        v.table_ui.more_open,
                        v.table_ui.picker_open,
                        local,
                    )
                {
                    overlay_hit = true;
                    v.dismiss_table_panels(cx);
                    cx.stop_propagation();
                }
            });
            if overlay_hit {
                return;
            }
            if !hb.is_hovered(win) {
                return;
            }
            win.focus(&fh);
            ent.update(cx, |v, cx| {
                v.dismiss_table_panels(cx);
                let Some(c) = v.hit_cursor_at(&slot_snap, slot_rev, local, win, cx) else {
                    return;
                };
                if !v.selection_covers(c) {
                    v.apply_click_hit(
                        c,
                        PendingClick {
                            x: local.0,
                            y: local.1,
                            motion: CursorMotion::Move,
                            follow_link: false,
                            select_word: false,
                        },
                    );
                }
                v.pending_click = None;
                cx.notify();
            });
            return;
        }
        if !hb.is_hovered(win) {
            return;
        }

        win.focus(&fh);
        let shift = ev.modifiers.shift;
        ent.update(cx, |v, cx| {
            if let Some(chrome) = v.table_ui.chrome
                && super::table_toolbar::overlay_contains(
                    chrome,
                    v.state.scroll,
                    v.table_ui.more_open,
                    v.table_ui.picker_open,
                    local,
                )
            {
                cx.stop_propagation();
                return;
            }
            if let Some(chrome) = v.media_chrome
                && media_zoom::chrome_contains(chrome, v.state.scroll, local)
            {
                cx.stop_propagation();
                return;
            }
            if v.table_ui.more_open {
                v.set_table_more_open(false, cx);
            }
            if v.table_ui.picker_open && !v.table_ui.picker_drag {
                v.close_table_picker(true, cx);
            }
            for hit in wells.iter() {
                let s = v.well_scroll.get(&hit.id).copied().unwrap_or_default();
                let lx = local.0 - hit.x;
                let ly = local.1 - hit.y;
                for vertical in [true, false] {
                    if let Some(bar) = WellBar::on_axis(vertical, hit, s, &bar_chrome)
                        && bar.contains(lx, ly)
                    {
                        let along = bar.along(lx, ly);
                        let on_thumb = bar.thumb_contains(lx, ly);
                        let grab = if on_thumb {
                            along - bar.thumb_start()
                        } else {
                            bar.thumb_len() * 0.5
                        };
                        let pos = if on_thumb {
                            s.along_axis(vertical)
                        } else {
                            bar.scroll_for_pointer(along, grab)
                        };
                        v.well_scroll.insert(hit.id, s.with_axis(vertical, pos));
                        v.well_bar_drag = Some((hit.id, vertical, grab));
                        v.dragging = false;
                        cx.notify();
                        return;
                    }
                }
            }
            if let Some(bar) = ScrollbarGeom::layout(vw, vh, painted_scroll, total, &bar_chrome)
                && bar.contains(local.0, local.1)
            {
                let on_thumb = bar.thumb_contains(local.0, local.1);
                let grab = if on_thumb {
                    local.1 - bar.thumb_y
                } else {
                    bar.thumb_h * 0.5
                };
                v.state.scroll = if on_thumb {
                    painted_scroll
                } else {
                    bar.scroll_for_pointer(local.1, grab)
                };
                v.scrollbar_drag = Some(grab);
                v.dragging = false;
                cx.notify();
                return;
            }
            if !v.table_ui.picker_open
                && v.table_ui.col_resize.is_none()
                && v.table_ui.reorder.is_none()
                && let Some(hit) = super::table_reorder::hit_grip(
                    &v.state.doc,
                    &cell_hits_down,
                    v.state.cursor.block,
                    local,
                )
            {
                v.begin_table_reorder(hit, local, &cell_hits_down, cx);
                return;
            }
            if !v.table_ui.picker_open
                && v.table_ui.col_resize.is_none()
                && let Some(hit) =
                    super::table_cols::hit_col_seam(&v.state.doc, &cell_hits_down, local)
            {
                v.begin_col_resize(hit, cx);
                return;
            }
            if !shift && let Some(block) = hit_list_item_slot(&slot_snap, slot_rev, local) {
                let keep = v.state.cursor;
                let _ = v.state.doc.apply(
                    Sel::collapsed(Cursor { block, offset: 0 }),
                    Command::ToggleTask,
                );
                v.state.cursor = keep;
                v.pending_open = None;
                v.pending_click = None;
                v.dragging = false;
                cx.notify();
                return;
            }
            let click = PendingClick {
                x: local.0,
                y: local.1,
                motion: CursorMotion::from_shift(shift),
                follow_link: ev.modifiers.control,
                select_word: ev.click_count == 2 && !shift,
            };
            if !v.place_caret_at_hit(&slot_snap, slot_rev, click, win, cx) {
                v.pending_click = Some(click);
            }
            v.dragging = true;
            v.scrollbar_drag = None;
            v.well_bar_drag = None;
            cx.notify();
        });
    });
}

pub(super) fn on_mouse_move(f: &InputFrame, window: &mut Window) {
    let (ox, oy) = f.origin;
    let vw = f.viewport.0;
    let vh = f.viewport.1;
    let total = f.total_height;
    let ent_m = f.state.clone();
    let bar_chrome_m = f.chrome;
    let wells_m = Rc::clone(&f.wells);
    let cell_hits_m = Rc::clone(&f.cells);
    let media_hits_m = Rc::clone(&f.media_hits);
    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _win, cx| {
        if !phase.bubble() {
            return;
        }
        let local = (
            (f32::from(ev.position.x) - ox) as Px,
            (f32::from(ev.position.y) - oy) as Px,
        );
        ent_m.update(cx, |v, cx| {
            if v.media_zoom.is_some() {
                v.media_zoom_move(local, cx);
                return;
            }
            if v.table_ui.reorder.is_some() {
                v.table_reorder_to(&cell_hits_m, local, cx);
                return;
            }
            if v.table_ui.col_resize.is_some() {
                v.col_resize_to(local.0, cx);
                return;
            }
            if let Some((id, vertical, grab)) = v.well_bar_drag
                && let Some(hit) = wells_m.iter().find(|hit| hit.id == id)
            {
                let s = v.well_scroll.get(&id).copied().unwrap_or_default();
                if let Some(bar) = WellBar::on_axis(vertical, hit, s, &bar_chrome_m) {
                    let along = bar.along(local.0 - hit.x, local.1 - hit.y);
                    let pos = bar.scroll_for_pointer(along, grab);
                    v.well_scroll.insert(id, s.with_axis(vertical, pos));
                    cx.notify();
                }
                return;
            }
            if let Some(grab) = v.scrollbar_drag
                && let Some(bar) =
                    ScrollbarGeom::layout(vw, vh, v.state.scroll, total, &bar_chrome_m)
            {
                v.state.scroll = bar.scroll_for_pointer(local.1, grab);
                cx.notify();
                return;
            }
            if !ev.dragging() || !v.dragging {
                v.update_table_grip_hover(&cell_hits_m, local, cx);
                v.hover_media(&media_hits_m, local, cx);
                return;
            }
            v.drag_pointer = Some(local);
            cx.notify();
        });
    });
}

pub(super) fn on_mouse_up(f: &InputFrame, window: &mut Window) {
    let (ox, oy) = f.origin;
    let cell_hits_u = Rc::clone(&f.cells);
    let ent_u = f.state.clone();
    window.on_mouse_event(move |ev: &MouseUpEvent, phase, win, cx| {
        if ev.button != MouseButton::Left {
            return;
        }
        let local = (
            (f32::from(ev.position.x) - ox) as Px,
            (f32::from(ev.position.y) - oy) as Px,
        );
        let mut ended_reorder = false;
        ent_u.update(cx, |v, cx| {
            if v.table_ui.reorder.is_some() {
                v.table_reorder_to(&cell_hits_u, local, cx);
                v.end_table_reorder(win, cx);
                ended_reorder = true;
            }
        });
        if ended_reorder || !phase.bubble() {
            return;
        }
        ent_u.update(cx, |v, cx| {
            if v.media_zoom.is_some() {
                v.media_zoom_release();
                return;
            }
            if v.table_ui.picker_drag || v.state.doc.is_composing() && v.table_ui.picker_open {
                v.finish_table_picker(win, cx);
                return;
            }
            if v.table_ui.col_resize.is_some() {
                v.end_col_resize(cx);
                return;
            }
            let dragged = v.drag_pointer.is_some();
            let text_drag = v.dragging;
            v.dragging = false;
            v.scrollbar_drag = None;
            v.well_bar_drag = None;
            v.drag_pointer = None;
            if text_drag {
                v.follow_caret = true;
                cx.notify();
            }
            if dragged {
                v.pending_open = None;
                v.enter_block_edit_on_click = false;
            } else if let Some(dest) = v.pending_open.take() {
                use crate::platform::open_markdown::{OpenTarget, classify_dest};
                match classify_dest(&dest, v.state.doc.source_path.as_deref()) {
                    OpenTarget::Ignore => {}
                    OpenTarget::Url(url) => cx.open_url(&url),
                    OpenTarget::Local(path) => cx.open_with_system(&path),
                    OpenTarget::ConfirmLocal(path) => {
                        let detail = path.display().to_string();
                        let answers = [
                            md_i18n::t(md_i18n::Key::DlgCancel),
                            md_i18n::t(md_i18n::Key::DlgOpenAnyway),
                        ];
                        let prompt = win.prompt(
                            gpui::PromptLevel::Warning,
                            md_i18n::t(md_i18n::Key::DlgExternalFile),
                            Some(&detail),
                            &answers,
                            cx,
                        );
                        let handle = win.window_handle();
                        cx.spawn(async move |_, cx| {
                            if !matches!(prompt.await, Ok(1)) {
                                return;
                            }
                            let _ = handle.update(cx, |_, _, cx| cx.open_with_system(&path));
                        })
                        .detach();
                    }
                }
            } else if text_drag {
                v.commit_block_edit_on_click();
                v.enter_block_edit_on_click = v.pending_click.is_some();
                cx.notify();
            }
        });
    });
}

pub(super) fn on_scroll_wheel(f: &InputFrame, window: &mut Window) {
    let (ox, oy) = f.origin;
    let vw = f.viewport.0;
    let vh = f.viewport.1;
    let total = f.total_height;
    let hb2 = f.hitbox.clone();
    let ent2 = f.state.clone();
    let wells_w = Rc::clone(&f.wells);
    let scale_w = f.scale;
    window.on_mouse_event(move |ev: &ScrollWheelEvent, phase, win, cx| {
        if !phase.bubble() {
            return;
        }
        if ent2.read(cx).media_zoom.is_none() && !hb2.should_handle_scroll(win) {
            return;
        }
        let step = {
            let role = ent2.read(cx).state.theme.type_role(BlockKind::Paragraph);
            role.size_px as Px * role.line_height_em as Px
        };
        let (mut dx, mut dy) = match ev.delta {
            gpui::ScrollDelta::Pixels(p) => (f32::from(p.x) as Px, f32::from(p.y) as Px),
            gpui::ScrollDelta::Lines(l) => (l.x as Px * step, l.y as Px * step),
        };
        if ev.modifiers.shift && dx.abs() <= f64::EPSILON {
            dx = dy;
            dy = 0.0;
        }
        let local = (
            (f32::from(ev.position.x) - ox) as Px,
            (f32::from(ev.position.y) - oy) as Px,
        );
        ent2.update(cx, |v, cx| {
            if v.media_zoom.is_some() {
                v.media_zoom_wheel(local, dy, (vw, vh), scale_w, cx);
                return;
            }
            let mut used = false;
            if let Some(hit) = wells_w.iter().copied().find(|hit| {
                local.0 >= hit.x
                    && local.0 < hit.x + hit.view_w
                    && local.1 >= hit.y
                    && local.1 < hit.y + hit.view_h
            }) {
                let mut s = v.well_scroll.get(&hit.id).copied().unwrap_or_default();
                let max_x = (hit.content_w - hit.view_w).max(0.0);
                let max_y = (hit.content_h - hit.view_h).max(0.0);
                if max_x > 0.0 && dx.abs() > 0.0 {
                    let next = (s.x - dx).clamp(0.0, max_x);
                    if (next - s.x).abs() > f64::EPSILON {
                        s.x = next;
                        used = true;
                    }
                }
                if max_y > 0.0 && dy.abs() > 0.0 {
                    let next = (s.y - dy).clamp(0.0, max_y);
                    if (next - s.y).abs() > f64::EPSILON {
                        s.y = next;
                        used = true;
                        dy = 0.0;
                    }
                }
                v.well_scroll.insert(hit.id, s);
            }
            if dy.abs() > 0.0 {
                let max = (total - vh).max(0.0);
                v.state.scroll = (v.state.scroll - dy).clamp(0.0, max);
                used = true;
            }
            if used {
                cx.notify();
            }
        });
    });
}
