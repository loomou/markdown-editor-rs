use super::Shell;
use crate::ui::icons;
use crate::ui::scrollbar::{Slider, scroll_at};
use crate::ui::theme::{OUTLINE_HEAD_H, OUTLINE_ROW_H, OUTLINE_SB, OUTLINE_SB_THUMB, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, CursorStyle, Div, Entity, FontWeight, InteractiveElement,
    IntoElement, ListOffset, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Stateful, StatefulInteractiveElement, Styled, Window, canvas, div, list,
    point, px, svg,
};
use md_core::block::BlockKind;
use md_core::doc::Doc;
use md_i18n::{Key, t as t18};

#[derive(Clone)]
pub(super) struct OutlineRow {
    pub(super) block: u32,
    pub(super) level: u8,
    pub(super) label: String,
}

pub(super) fn outline_rows(doc: &Doc) -> Vec<OutlineRow> {
    let mut rows: Vec<OutlineRow> = Vec::new();

    doc.document.for_each_collapsed_text_leaf(|block, text| {
        if let Some(BlockKind::Heading(level)) = doc.document.kind(block) {
            rows.push(OutlineRow {
                block,
                level,
                label: text.trim().to_string(),
            });
        }
        true
    });

    rows
}

fn outline_rows_match(doc: &Doc, rows: &[OutlineRow]) -> bool {
    let mut seen = 0;
    let mut same = true;

    doc.document.for_each_collapsed_text_leaf(|block, text| {
        if let Some(BlockKind::Heading(level)) = doc.document.kind(block) {
            let Some(row) = rows.get(seen) else {
                same = false;
                return false;
            };
            if row.block != block || row.level != level || row.label != text.trim() {
                same = false;
                return false;
            }
            seen += 1;
        }
        true
    });

    same && seen == rows.len()
}

fn outline_row_metrics(row: &OutlineRow) -> (f32, f32, FontWeight) {
    let indent = OUTLINE_INDENT * f32::from(row.level.clamp(1, 6));
    match row.level {
        1 => (indent, 12.5, FontWeight(600.0)),
        2 => (indent, 12.5, FontWeight(400.0)),
        _ => (indent, 12.0, FontWeight(400.0)),
    }
}

fn outline_slider(view: f32, content: f32, offset: f32) -> Option<Slider> {
    if content <= view + 0.5 {
        return None;
    }
    Slider::new(
        f64::from(view),
        f64::from(content),
        f64::from(-offset),
        f64::from(OUTLINE_SB_PAD),
        f64::from(OUTLINE_SB_MIN_THUMB),
    )
}

pub(super) fn outline_thumb(view: f32, content: f32, offset: f32) -> Option<(f32, f32)> {
    let s = outline_slider(view, content, offset)?;
    Some((s.thumb_start as f32, s.thumb_len as f32))
}

fn outline_scroll_from_pointer(
    pointer: f32,
    grab: f32,
    origin: f32,
    view: f32,
    content: f32,
    thumb: f32,
) -> f32 {
    let track = (view - OUTLINE_SB_PAD * 2.0).max(0.0);
    scroll_at(
        f64::from(pointer),
        f64::from(grab),
        f64::from(origin + OUTLINE_SB_PAD),
        f64::from((track - thumb).max(0.0)),
        f64::from((content - view).max(0.0)),
    ) as f32
}

pub(super) const OUTLINE_MIN_VIEWPORT: f32 = 720.0;

pub(super) const OUTLINE_MIN_W: f32 = 140.0;

pub(super) const OUTLINE_MAX_W: f32 = 480.0;

pub(super) const OUTLINE_OVERDRAW: f32 = 120.0;

const OUTLINE_RESIZE_HIT: f32 = 6.0;

const OUTLINE_PAD_X: f32 = 10.0;

const OUTLINE_INDENT: f32 = 12.0;

const OUTLINE_SB_PAD: f32 = 4.0;

const OUTLINE_SB_MIN_THUMB: f32 = 24.0;

pub(super) const OUTLINE_FOLLOW_ROWS: usize = 8;

pub(super) fn clamp_outline_width(width: f32, viewport_w: f32) -> f32 {
    let max = OUTLINE_MAX_W.min((viewport_w * 0.5).max(OUTLINE_MIN_W));
    width.clamp(OUTLINE_MIN_W, max)
}

pub(super) struct OutlineCache {
    identity: u64,
    revision: u64,
    pub(super) rows: Vec<OutlineRow>,
}

impl Default for OutlineCache {
    fn default() -> Self {
        Self {
            identity: u64::MAX,
            revision: u64::MAX,
            rows: Vec::new(),
        }
    }
}

impl OutlineCache {
    pub(super) fn refresh_with(
        &mut self,
        identity: u64,
        revision: u64,
        build: impl FnOnce() -> Vec<OutlineRow>,
    ) {
        if self.identity == identity && self.revision == revision {
            return;
        }
        self.identity = identity;
        self.revision = revision;
        self.rows = build();
    }

    pub(super) fn get(&mut self, doc: &Doc) -> bool {
        let identity = doc.identity();
        let switched = self.identity != identity;
        let revision = doc.document.revision();

        if switched || self.revision != revision {
            if switched || !outline_rows_match(doc, &self.rows) {
                self.refresh_with(identity, revision, || outline_rows(doc));
            } else {
                self.revision = revision;
            }
        }

        switched
    }
}

impl Shell {
    pub(super) fn row_ix(&self, block: u32) -> Option<usize> {
        self.outline_cache
            .rows
            .iter()
            .position(|r| r.block == block)
    }

    pub(super) fn note_outline_row(&mut self, current: Option<u32>) {
        if current == self.outline_current {
            return;
        }
        let prev = self.outline_current;
        self.outline_current = current;
        let Some(ix) = current.and_then(|block| self.row_ix(block)) else {
            return;
        };
        let going_down = prev
            .and_then(|block| self.row_ix(block))
            .is_some_and(|prev_ix| prev_ix < ix);
        self.outline_follow = Some((ix, going_down));
    }

    pub(super) fn follow_outline_row(&self, ix: usize, going_down: bool) -> bool {
        if ix >= self.outline_cache.rows.len() {
            return false;
        }
        let state = &self.outline_scroll;
        let viewport = state.viewport_bounds();
        if viewport.size.height <= px(0.0) {
            return false;
        }
        let pad = px(OUTLINE_FOLLOW_ROWS as f32 * OUTLINE_ROW_H);
        let Some(bounds) = state.bounds_for_item(ix) else {
            state.scroll_to(ListOffset {
                item_ix: ix,
                offset_in_item: px(0.0),
            });
            state.scroll_by(-pad);
            return true;
        };
        let slack = if going_down {
            viewport.bottom() - bounds.bottom()
        } else {
            bounds.top() - viewport.top()
        };
        if slack >= pad {
            return false;
        }
        let delta = pad - slack;
        state.scroll_by(if going_down { delta } else { -delta });
        true
    }

    pub(super) fn outline_panel(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        viewport_w: Pixels,
        cx: &Context<'_, Self>,
    ) -> Div {
        let width = clamp_outline_width(self.outline_width, f32::from(viewport_w));
        div()
            .relative()
            .w(px(width))
            .flex_none()
            .flex()
            .flex_col()
            .bg(t.panel_bg)
            .border_l_1()
            .border_color(t.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.outline_head(t, this.clone()))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .on_children_prepainted({
                                let this = this.clone();
                                move |_, window, cx| {
                                    let scrolled = this.update(cx, |shell, cx| {
                                        let Some((ix, going_down)) = shell.outline_follow.take()
                                        else {
                                            return false;
                                        };
                                        if !shell.follow_outline_row(ix, going_down) {
                                            return false;
                                        }
                                        cx.notify();
                                        true
                                    });
                                    if scrolled {
                                        window.on_next_frame(|window, _| window.refresh());
                                    }
                                }
                            })
                            .id("outline-list")
                            .child(
                                list(
                                    self.outline_scroll.clone(),
                                    cx.processor(|this, ix: usize, _window, cx| {
                                        let t = this.theme();
                                        let current = this.outline_follow_current(cx);
                                        if current != this.outline_current {
                                            this.note_outline_row(current);
                                        }
                                        let Some(row) = this.outline_cache.rows.get(ix).cloned()
                                        else {
                                            return div().into_any_element();
                                        };
                                        let hit = this.outline_current == Some(row.block);
                                        this.outline_row(t, row, hit).into_any_element()
                                    }),
                                )
                                .size_full(),
                            )
                            .children(self.outline_scrollbars(t, this.clone())),
                    ),
            )
            .child(self.outline_resize_handle(this))
    }

    fn begin_outline_resize(&mut self, x: Pixels, cx: &mut Context<'_, Self>) {
        self.outline_resize = Some((x, self.outline_width));
        cx.notify();
    }

    fn outline_resize_to(&mut self, x: Pixels, viewport_w: f32, cx: &mut Context<'_, Self>) {
        let Some((start_x, start_w)) = self.outline_resize else {
            return;
        };
        let next = start_w + f32::from(start_x - x);
        let next = clamp_outline_width(next, viewport_w);
        if (next - self.outline_width).abs() < 0.5 {
            return;
        }
        self.outline_width = next;
        cx.notify();
    }

    fn end_outline_resize(&mut self, cx: &mut Context<'_, Self>) {
        if self.outline_resize.take().is_some() {
            cx.notify();
        }
    }

    fn outline_resize_handle(&self, this: Entity<Self>) -> Stateful<Div> {
        div()
            .id("outline-resize")
            .absolute()
            .left(px(-(OUTLINE_RESIZE_HIT + 1.0) * 0.5))
            .top_0()
            .h_full()
            .w(px(OUTLINE_RESIZE_HIT))
            .cursor(CursorStyle::ResizeLeftRight)
            .debug_selector(|| "outline-resize".into())
            .child(
                canvas(
                    |_, _, _| (),
                    move |hit, _, window, _| {
                        window.on_mouse_event({
                            let this = this.clone();
                            move |ev: &MouseDownEvent, phase, _, cx| {
                                if !phase.bubble() || ev.button != MouseButton::Left {
                                    return;
                                }
                                if hit.contains(&ev.position) {
                                    this.update(cx, |shell, cx| {
                                        shell.begin_outline_resize(ev.position.x, cx);
                                        cx.stop_propagation();
                                    });
                                } else {
                                    this.update(cx, |shell, cx| shell.end_outline_resize(cx));
                                }
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            move |ev: &MouseMoveEvent, _, window, cx| {
                                if !ev.dragging() {
                                    return;
                                }
                                if this.read(cx).outline_resize.is_none() {
                                    return;
                                }
                                let vw = f32::from(window.viewport_size().width);
                                this.update(cx, |shell, cx| {
                                    shell.outline_resize_to(ev.position.x, vw, cx);
                                });
                            }
                        });
                        window.on_mouse_event({
                            move |_: &MouseUpEvent, _, _, cx| {
                                this.update(cx, |shell, cx| shell.end_outline_resize(cx));
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    fn outline_head(&self, t: ShellTheme, this: Entity<Self>) -> Div {
        div()
            .h(px(OUTLINE_HEAD_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(OUTLINE_PAD_X + OUTLINE_INDENT))
            .pr(px(OUTLINE_PAD_X + OUTLINE_INDENT))
            .text_size(px(10.5))
            .font_weight(FontWeight(700.0))
            .text_color(t.text_disabled)
            .child(t18(Key::OutlineTitle))
            .child(div().flex_1())
            .child(
                div()
                    .id("btn-collapse-outline")
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(move |s| s.bg(t.hover))
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        this.update(cx, |shell, cx| {
                            shell.outline_open = false;
                            cx.notify();
                        });
                    })
                    .child(
                        svg()
                            .size(px(12.))
                            .path(icons::OUTLINE_MINI)
                            .text_color(t.text_disabled),
                    ),
            )
    }

    pub(super) fn outline_follow_current(&self, cx: &Context<'_, Self>) -> Option<u32> {
        let editor = self.editor.read(cx);
        let engine = editor.state.incremental.as_ref()?;
        let slack = editor.follow_line_slack();
        let blocks: Vec<u32> = self.outline_cache.rows.iter().map(|r| r.block).collect();
        let line = editor.state.resolved_top + slack;
        engine
            .heading_at_or_above(&editor.state.doc.document, &blocks, line)
            .or_else(|| self.outline_cache.rows.first().map(|r| r.block))
    }

    fn outline_row(&self, t: ShellTheme, row: OutlineRow, current: bool) -> Stateful<Div> {
        let editor = self.editor.clone();
        let block = row.block;
        let (indent, size, weight) = outline_row_metrics(&row);
        let color = match row.level {
            1 => t.text,
            2 => t.text_muted,
            _ => t.text_disabled,
        };
        div()
            .id(("outline", block))
            .w_full()
            .min_h(px(OUTLINE_ROW_H))
            .flex()
            .items_center()
            .py(px(2.))
            .text_size(px(size))
            .font_weight(weight)
            .when(current, |d| d.bg(t.selected_bg).text_color(t.text))
            .when(!current, |d| {
                d.text_color(color)
                    .hover(move |s| s.bg(t.hover).text_color(t.text))
            })
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                editor.update(cx, |view, cx| {
                    view.jump_to_block(block);
                    cx.notify();
                    window.focus(&view.focus, cx);
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .pl(px(OUTLINE_PAD_X + indent))
                    .pr(px(OUTLINE_PAD_X + OUTLINE_INDENT))
                    .w_full()
                    .min_w_0()
                    .child(div().flex_1().min_w_0().child(row.label)),
            )
    }

    fn outline_scrollbars(&self, t: ShellTheme, this: Entity<Self>) -> Vec<Stateful<Div>> {
        let state = &self.outline_scroll;
        let view = f32::from(state.viewport_bounds().size.height);
        let max = f32::from(state.max_offset_for_scrollbar().y);
        if view <= 0.0 || max <= 0.5 {
            return Vec::new();
        }
        let offset = f32::from(state.scroll_px_offset_for_scrollbar().y);
        let Some((pos, len)) = outline_thumb(view, view + max, offset) else {
            return Vec::new();
        };
        vec![self.outline_scrollbar_thumb("outline-sb-y", pos, len, t, this, state.clone())]
    }

    fn outline_scrollbar_thumb(
        &self,
        id: &'static str,
        pos: f32,
        len: f32,
        t: ShellTheme,
        this: Entity<Self>,
        state: ListState,
    ) -> Stateful<Div> {
        let gap = (OUTLINE_SB - OUTLINE_SB_THUMB) * 0.5;
        div()
            .id(id)
            .absolute()
            .top(px(pos))
            .right(px(gap))
            .w(px(OUTLINE_SB_THUMB))
            .h(px(len))
            .rounded_full()
            .bg(t.border.opacity(0.75))
            .hover(move |s| s.bg(t.text_disabled))
            .child(
                canvas(
                    |_, _, _| (),
                    move |thumb_bounds, _, window, _| {
                        window.on_mouse_event({
                            let this = this.clone();
                            let state = state.clone();
                            move |ev: &MouseDownEvent, _, _, cx| {
                                if ev.button != MouseButton::Left
                                    || !thumb_bounds.contains(&ev.position)
                                {
                                    return;
                                }
                                let grab = ev.position.y - thumb_bounds.origin.y;
                                state.scrollbar_drag_started();
                                this.update(cx, |shell, _| {
                                    shell.outline_sb_drag = Some(grab);
                                });
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            let state = state.clone();
                            move |_: &MouseUpEvent, _, _, cx| {
                                state.scrollbar_drag_ended();
                                this.update(cx, |shell, cx| {
                                    if shell.outline_sb_drag.take().is_some() {
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            let state = state.clone();
                            move |ev: &MouseMoveEvent, _, _, cx| {
                                if !ev.dragging() {
                                    return;
                                }
                                let Some(grab) = this.read(cx).outline_sb_drag else {
                                    return;
                                };
                                let viewport = state.viewport_bounds();
                                let view = f32::from(viewport.size.height);
                                let max = f32::from(state.max_offset_for_scrollbar().y);
                                let next = outline_scroll_from_pointer(
                                    f32::from(ev.position.y),
                                    f32::from(grab),
                                    f32::from(viewport.top()),
                                    view,
                                    view + max,
                                    len,
                                );
                                state.set_offset_from_scrollbar(point(px(0.), px(-next)));
                                this.update(cx, |_, cx| cx.notify());
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    pub(super) fn outline_toggle(&self, t: ShellTheme, this: Entity<Self>) -> Stateful<Div> {
        let icon_color = if self.outline_open {
            t.text
        } else {
            t.text_muted
        };
        div()
            .id("btn-outline")
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .hover(move |s| s.bg(t.hover))
            .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    shell.outline_open = !shell.outline_open;
                    cx.notify();
                })
            })
            .child(
                svg()
                    .size(px(13.))
                    .path(icons::OUTLINE)
                    .text_color(icon_color),
            )
    }
}
