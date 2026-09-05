use super::Shell;
use crate::ui::icons;
use crate::ui::scrollbar::{Slider, scroll_at};
use crate::ui::theme::{
    OUTLINE_HEAD_H, OUTLINE_ROW_H, OUTLINE_SB, OUTLINE_SB_THUMB, ShellTheme, UI_FONT,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, CursorStyle, Div, Entity, Font, FontStyle, FontWeight,
    InteractiveElement, ListHorizontalSizingBehavior, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, ScrollHandle, Size, Stateful, StatefulInteractiveElement,
    Styled, TextRun, Window, canvas, div, point, px, rgba, svg, uniform_list,
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

    for id in doc.document.preorder() {
        let Some(node) = doc.document.arena.get(id) else {
            continue;
        };
        if let BlockKind::Heading(level) = node.kind {
            rows.push(OutlineRow {
                block: id.index,
                level,
                label: doc.document.display(id).trim().to_string(),
            });
        }
    }

    rows
}

fn outline_row_metrics(row: &OutlineRow) -> (f32, f32, FontWeight) {
    match row.level {
        1 => (12.0, 12.5, FontWeight(600.0)),
        2 => (24.0, 12.5, FontWeight(400.0)),
        _ => (38.0, 12.0, FontWeight(400.0)),
    }
}

fn outline_text_px(window: &Window, label: &str, size: f32, weight: FontWeight) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    let font = Font {
        family: UI_FONT.into(),
        features: Default::default(),
        fallbacks: None,
        weight,
        style: FontStyle::Normal,
    };
    let run = TextRun {
        len: label.len(),
        font,
        color: rgba(0xffffffff).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_text(label.to_string().into(), px(size), &[run], None, None)
        .ok()
        .and_then(|lines| lines.into_iter().next())
        .map(|line| f32::from(line.width()))
        .unwrap_or(0.0)
}

fn outline_row_pixel_width(window: &Window, row: &OutlineRow) -> f32 {
    let (indent, size, weight) = outline_row_metrics(row);
    indent + 8.0 + 6.0 + outline_text_px(window, &row.label, size, weight) + 12.0
}

fn outline_measure_index(window: &Window, rows: &[OutlineRow]) -> Option<usize> {
    let mut ix = None;
    let mut best = 0.0_f32;
    for (i, row) in rows.iter().enumerate() {
        let w = outline_row_pixel_width(window, row);
        if ix.is_none() || w > best {
            best = w;
            ix = Some(i);
        }
    }
    ix
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

const OUTLINE_RESIZE_HIT: f32 = 6.0;

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

    pub(super) measure_ix: Option<usize>,
}

impl Default for OutlineCache {
    fn default() -> Self {
        Self {
            identity: u64::MAX,
            revision: u64::MAX,
            rows: Vec::new(),
            measure_ix: None,
        }
    }
}

impl OutlineCache {
    pub(super) fn refresh_with(
        &mut self,
        window: &Window,
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
        self.measure_ix = outline_measure_index(window, &self.rows);
    }

    pub(super) fn get(&mut self, window: &Window, doc: &Doc) -> bool {
        let identity = doc.identity();
        let switched = self.identity != identity;
        self.refresh_with(window, identity, doc.document.revision(), || {
            outline_rows(doc)
        });
        switched
    }
}

impl Shell {
    pub(super) fn follow_outline_row(&mut self, prev: Option<u32>) {
        let Some(current) = self.outline_current else {
            return;
        };
        let Some(ix) = self
            .outline_cache
            .rows
            .iter()
            .position(|r| r.block == current)
        else {
            return;
        };
        let going_down = prev
            .and_then(|p| self.outline_cache.rows.iter().position(|r| r.block == p))
            .is_some_and(|prev_ix| prev_ix < ix);

        let st = self.outline_scroll.0.borrow();
        let Some(sz) = st.last_item_size else {
            return;
        };
        let view_h = f32::from(sz.item.height);
        let content_h = f32::from(sz.contents.height);
        let top = (-f32::from(st.base_handle.offset().y)).max(0.0);
        let handle = st.base_handle.clone();
        drop(st);

        let pad = OUTLINE_FOLLOW_ROWS as f32 * OUTLINE_ROW_H;
        let next = if going_down {
            let item_bottom = (ix as f32 + 1.0) * OUTLINE_ROW_H;
            if top + view_h - item_bottom >= pad {
                return;
            }

            item_bottom + pad - view_h
        } else {
            let item_top = ix as f32 * OUTLINE_ROW_H;
            if item_top - top >= pad {
                return;
            }

            item_top - pad
        };
        let next = next.clamp(0.0, (content_h - view_h).max(0.0));
        let cur = handle.offset();
        handle.set_offset(point(cur.x, px(-next)));
    }

    pub(super) fn outline_panel(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        viewport_w: Pixels,
        cx: &Context<'_, Self>,
    ) -> Div {
        let count = self.outline_cache.rows.len();
        let measure_ix = self.outline_cache.measure_ix;
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
                            .id("outline-list")
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                uniform_list("outline-rows", count, {
                                    cx.processor(
                                        |this, range: std::ops::Range<usize>, window, cx| {
                                            let t = this.theme();
                                            let current = this.outline_follow_current(cx);

                                            if current != this.outline_current {
                                                let prev = this.outline_current;
                                                this.outline_current = current;
                                                this.follow_outline_row(prev);
                                                window.on_next_frame(|window, _| window.refresh());
                                            }
                                            let rows = this.outline_cache.rows[range].to_vec();
                                            rows.into_iter()
                                                .map(|row| {
                                                    let hit = current == Some(row.block);
                                                    this.outline_row(t, row, hit, window)
                                                })
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                })
                                .size_full()
                                .with_horizontal_sizing_behavior(
                                    ListHorizontalSizingBehavior::Unconstrained,
                                )
                                .with_width_from_item(measure_ix)
                                .track_scroll(self.outline_scroll.clone()),
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
            .pl(px(14.))
            .pr(px(8.))
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

    fn outline_follow_current(&self, cx: &Context<'_, Self>) -> Option<u32> {
        let editor = self.editor.read(cx);
        let engine = editor.state.incremental.as_ref()?;
        let slack = editor.follow_line_slack();
        let blocks: Vec<u32> = self.outline_cache.rows.iter().map(|r| r.block).collect();
        let line = editor.state.resolved_top + slack;
        engine
            .heading_at_or_above(&editor.state.doc.document, &blocks, line)
            .or_else(|| self.outline_cache.rows.first().map(|r| r.block))
    }

    fn outline_row(
        &self,
        t: ShellTheme,
        row: OutlineRow,
        current: bool,
        window: &Window,
    ) -> Stateful<Div> {
        let editor = self.editor.clone();
        let block = row.block;
        let (indent, size, weight) = outline_row_metrics(&row);
        let row_w = outline_row_pixel_width(window, &row);
        let color = match row.level {
            1 => t.text,
            2 => t.text_muted,
            _ => t.text_disabled,
        };
        let dot = match row.level {
            1 => t.accent,
            2 => t.syn_cyan,
            _ => t.text_disabled,
        };
        div()
            .id(("outline", block))
            .w_full()
            .min_w(px(row_w.max(1.0)))
            .h(px(OUTLINE_ROW_H))
            .flex_none()
            .flex()
            .items_center()
            .whitespace_nowrap()
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
                    window.focus(&view.focus);
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(indent))
                    .pr(px(12.))
                    .flex_none()
                    .child(div().size(px(6.)).rounded_full().bg(dot).flex_none())
                    .child(div().flex_none().child(row.label)),
            )
    }

    fn outline_scrollbars(&self, t: ShellTheme, this: Entity<Self>) -> Vec<Stateful<Div>> {
        let st = self.outline_scroll.0.borrow();
        let Some(sz) = st.last_item_size else {
            return Vec::new();
        };
        let offset = st.base_handle.offset();
        let handle = st.base_handle.clone();
        drop(st);

        let mut bars = Vec::new();
        if let Some((pos, len)) = outline_thumb(
            f32::from(sz.item.height),
            f32::from(sz.contents.height),
            f32::from(offset.y),
        ) {
            bars.push(self.outline_scrollbar_thumb(
                "outline-sb-y",
                true,
                pos,
                len,
                t,
                this.clone(),
                handle.clone(),
                sz.item,
                sz.contents,
            ));
        }
        if let Some((pos, len)) = outline_thumb(
            f32::from(sz.item.width),
            f32::from(sz.contents.width),
            f32::from(offset.x),
        ) {
            bars.push(self.outline_scrollbar_thumb(
                "outline-sb-x",
                false,
                pos,
                len,
                t,
                this,
                handle,
                sz.item,
                sz.contents,
            ));
        }
        bars
    }

    #[allow(clippy::too_many_arguments)]
    fn outline_scrollbar_thumb(
        &self,
        id: &'static str,
        vertical: bool,
        pos: f32,
        len: f32,
        t: ShellTheme,
        this: Entity<Self>,
        handle: ScrollHandle,
        view: Size<Pixels>,
        content: Size<Pixels>,
    ) -> Stateful<Div> {
        let gap = (OUTLINE_SB - OUTLINE_SB_THUMB) * 0.5;
        let bar = if vertical {
            div()
                .id(id)
                .absolute()
                .top(px(pos))
                .right(px(gap))
                .w(px(OUTLINE_SB_THUMB))
                .h(px(len))
        } else {
            div()
                .id(id)
                .absolute()
                .left(px(pos))
                .bottom(px(gap))
                .h(px(OUTLINE_SB_THUMB))
                .w(px(len))
        };
        bar.rounded_full()
            .bg(t.border.opacity(0.75))
            .hover(move |s| s.bg(t.text_disabled))
            .child(
                canvas(
                    |_, _, _| (),
                    move |thumb_bounds, _, window, _| {
                        window.on_mouse_event({
                            let this = this.clone();
                            move |ev: &MouseDownEvent, _, _, cx| {
                                if ev.button != MouseButton::Left
                                    || !thumb_bounds.contains(&ev.position)
                                {
                                    return;
                                }
                                let grab = if vertical {
                                    ev.position.y - thumb_bounds.origin.y
                                } else {
                                    ev.position.x - thumb_bounds.origin.x
                                };
                                this.update(cx, |shell, _| {
                                    shell.outline_sb_drag = Some((vertical, grab));
                                });
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            move |_: &MouseUpEvent, _, _, cx| {
                                this.update(cx, |shell, cx| {
                                    if shell.outline_sb_drag.take().is_some() {
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            let handle = handle.clone();
                            move |ev: &MouseMoveEvent, _, _, cx| {
                                if !ev.dragging() {
                                    return;
                                }
                                let Some((axis, grab)) = this.read(cx).outline_sb_drag else {
                                    return;
                                };
                                if axis != vertical {
                                    return;
                                }
                                let bounds = handle.bounds();
                                let next = if vertical {
                                    outline_scroll_from_pointer(
                                        f32::from(ev.position.y),
                                        f32::from(grab),
                                        f32::from(bounds.origin.y),
                                        f32::from(view.height),
                                        f32::from(content.height),
                                        len,
                                    )
                                } else {
                                    outline_scroll_from_pointer(
                                        f32::from(ev.position.x),
                                        f32::from(grab),
                                        f32::from(bounds.origin.x),
                                        f32::from(view.width),
                                        f32::from(content.width),
                                        len,
                                    )
                                };
                                let cur = handle.offset();
                                if vertical {
                                    handle.set_offset(point(cur.x, px(-next)));
                                } else {
                                    handle.set_offset(point(px(-next), cur.y));
                                }
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
                });
            })
            .child(
                svg()
                    .size(px(13.))
                    .path(icons::OUTLINE)
                    .text_color(icon_color),
            )
    }
}
