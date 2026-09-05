use super::draw::{
    ColResizeGuide, ReorderChrome, gpui_align, is_scroll_well, paint_artifact,
    paint_clone_cell_borders, paint_col_resize_guide, paint_diag_overlay, paint_fail_box,
    paint_gutter_label, paint_table_reorder, paint_well_lang, well_content_w, well_lang_for,
    well_padding, well_view_h,
};
use super::popover::{PopoverPaint, paint_math_popover, paint_popover};
use super::scrollbar::{select_autoscroll_can_move, select_autoscroll_delta};
use super::{
    ArtifactPaint, EditorElement, EditorView, PaintFault, PrepaintState, ScrollbarGeom, WellBar,
    WellHit, WellScroll, events, overlay_diag_labels,
};
use crate::ui::theme::ShellTheme;
use gpui::{App, Bounds, ContentMask, ElementInputHandler, Hsla, Pixels, Window, point, px, size};
use md_content::gpui_theme::{ThemeColorExt, TypeRoleExt};
use md_content::{images, math, mermaid};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::inline::InlineAlign;
use md_render::snapshot::Frame;
use md_theme::{ChromeTokens, DocumentTheme};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Copy)]
struct PaintCtx<'a> {
    ox: f32,
    oy: f32,
    theme: &'a DocumentTheme,

    paint: &'a md_theme::PaintTokens,

    scale: f64,
    math_ready: &'a HashMap<math::MathKey, math::ReadyImage>,
    image_ready: &'a HashMap<images::DisplayKey, images::ReadyImage>,
    failed_math: &'a HashSet<math::MathKey>,
    failed_sources: &'a HashSet<images::SourceKey>,
    well_scroll: &'a HashMap<BlockId, WellScroll>,

    code_langs: &'a HashMap<u32, String>,

    caret_block: BlockId,

    mermaid_theme_fp: u64,

    fault: PaintFault,
}

impl PaintCtx<'_> {
    fn bounds(&self, (x, y, w, h): (Px, Px, Px, Px)) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(self.ox + x as f32), px(self.oy + y as f32)),
            size: size(px(w as f32), px(h as f32)),
        }
    }

    fn artifact(
        &self,
        x: f32,
        y: f32,
        kind: BlockKind,
        align: InlineAlign,
        inner: Px,
    ) -> ArtifactPaint<'_> {
        ArtifactPaint {
            x,
            y,
            inner,
            kind,
            inline_align: align,
            align: gpui_align(align),
            theme: self.theme,
            dpr: self.scale,
            lookup: self.math_ready,
            images: self.image_ready,
            failed_math: self.failed_math,
        }
    }
}

impl EditorElement {
    pub(super) fn paint_impl(
        &mut self,
        bounds: Bounds<Pixels>,
        st: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let f = &st.frame;
        let ox = f32::from(bounds.origin.x);
        let oy = f32::from(bounds.origin.y);

        let theme = self.state.read(cx).state.theme;
        let paint = theme.paint;
        let scale = f64::from(window.scale_factor());
        let mermaid_theme_fp = mermaid::theme_fingerprint(&theme);
        let (
            math_ready,
            image_ready,
            failed_math,
            failed_sources,
            well_scroll,
            code_langs,
            fault,
            caret_block,
        ) = {
            let v = self.state.read(cx);
            (
                v.math.ready_snapshot(),
                v.images.ready_snapshot(),
                v.math.failed_snapshot(),
                v.images.failed_sources(),
                v.well_scroll.clone(),
                Rc::clone(&v.doc_maps.block_code_lang),
                v.state.paint_fault,
                v.state.cursor.block,
            )
        };
        let ctx = &PaintCtx {
            ox,
            oy,
            theme: &theme,
            paint: &paint,
            scale,
            math_ready: &math_ready,
            image_ready: &image_ready,
            failed_math: &failed_math,
            failed_sources: &failed_sources,
            well_scroll: &well_scroll,
            code_langs: &code_langs,
            caret_block,
            mermaid_theme_fp,
            fault,
        };

        paint_block_chrome(f, ctx, window, cx);

        let well_hits = Rc::new(collect_well_hits(&f.texts, |t| {
            if t.kind != BlockKind::Mermaid || t.edit_source {
                return None;
            }
            let v = self.state.read(cx);
            mermaid::key_for(
                &v.state.doc.document,
                t.block,
                t.content_width,
                theme.decoration.mermaid_max_width,
                theme.decoration.mermaid_max_height,
                scale,
                mermaid_theme_fp,
            )
            .and_then(|key| v.mermaid.image(&key))
            .map(|ready| {
                let (w, h) = ready.css_size();
                (w as Px, h as Px)
            })
        }));

        paint_under_text(f, &well_hits, ctx, window);
        self.paint_texts(f, ctx, window, cx);
        self.paint_table(f, ctx, window, cx);

        if let Some(r) = f.caret_device
            && st.caret_on
            && let Some(r) =
                overlay_in_well(r, &well_hits, &f.texts, &well_scroll, Some(caret_block))
        {
            window.paint_quad(gpui::fill(ctx.bounds(r), paint.caret.hsla()));
        }

        paint_popovers(st, ctx, window);

        let chrome = theme.chrome;
        paint_scrollbars(f, &well_hits, &chrome, ctx, window);

        {
            let v = self.state.read(cx);
            let d = v.state.diag.borrow();
            let labels = overlay_diag_labels(
                v.state.show_fps,
                v.state.stress_redraw,
                d.fps,
                d.frame_ms,
                d.materialized,
                d.last_exact,
            );
            drop(d);
            paint_diag_overlay((ox + 8.0, oy + 8.0), &labels, &theme, window, cx);
        }

        let entity = self.state.clone();
        super::media_zoom::paint_zoom(window, cx, &entity, (ox, oy), f.viewport, scale);

        let next_refresh = {
            let v = entity.read(cx);
            if st.refresh
                || v.state.stress_redraw
                || v.search.reveal.is_some()
                || v.search.refresh > 0
            {
                true
            } else if v.dragging
                && v.scrollbar_drag.is_none()
                && v.well_bar_drag.is_none()
                && let Some((_, y)) = v.drag_pointer
            {
                let dy = select_autoscroll_delta(y, f.viewport.1, &chrome);
                select_autoscroll_can_move(dy, v.state.scroll, f.viewport.1, f.total_height)
            } else {
                false
            }
        };
        if next_refresh {
            window.on_next_frame(|window, _| window.refresh());
        }
        window.handle_input(
            &entity.read(cx).focus.clone(),
            ElementInputHandler::new(bounds, entity.clone()),
            cx,
        );

        let input = events::InputFrame {
            state: self.state.clone(),
            hitbox: st.hitbox.clone(),
            focus: entity.read(cx).focus.clone(),
            origin: (ox, oy),
            viewport: f.viewport,
            total_height: f.total_height,
            painted_scroll: f.scroll,
            chrome,
            snapshot: Rc::clone(&f.snapshot),
            geometry_revision: f.geometry_revision,
            wells: well_hits,
            cells: Rc::new(f.cells.iter().map(|c| (c.block, c.rect_device)).collect()),
            scale,
            media_hits: Rc::new(super::media_zoom::collect_hits(
                &f.texts,
                entity.read(cx).state.doc.block_edit(),
            )),
        };
        events::on_mouse_down(&input, window);
        events::on_mouse_move(&input, window);
        events::on_mouse_up(&input, window);
        events::on_scroll_wheel(&input, window);
    }

    fn paint_texts(&mut self, f: &Frame, ctx: &PaintCtx<'_>, window: &mut Window, cx: &mut App) {
        let PaintCtx {
            ox,
            oy,
            theme,
            paint,
            scale,
            well_scroll,
            code_langs,
            mermaid_theme_fp,
            fault,
            ..
        } = *ctx;
        for (ti, t) in f.texts.iter().enumerate() {
            let (cx0, cy0) = t.content_origin_device;
            if fault == PaintFault::GlyphPaintError && ti == 0 {
                window.paint_quad(gpui::fill(
                    Bounds {
                        origin: point(px(ox + cx0 as f32), px(oy + cy0 as f32)),
                        size: gpui::size(px(t.content_width as f32), px(t.art.height as f32)),
                    },
                    paint.glyph_fault.hsla(),
                ));
                continue;
            }
            if t.kind == BlockKind::Mermaid && !t.edit_source {
                let max_w = theme.decoration.mermaid_max_width;
                let max_h = theme.decoration.mermaid_max_height;
                let (ready, fail_label) = {
                    let v = self.state.read(cx);
                    match mermaid::key_for(
                        &v.state.doc.document,
                        t.block,
                        t.content_width,
                        max_w,
                        max_h,
                        scale,
                        mermaid_theme_fp,
                    ) {
                        Some(key) => (
                            v.mermaid.image(&key),
                            v.mermaid.failed_error(&key).map(mermaid::fail_label),
                        ),
                        None => (None, None),
                    }
                };
                let view_h = well_view_h(t);
                let s = well_scroll.get(&t.block).copied().unwrap_or_default();
                let clip = Bounds {
                    origin: point(px(ox + cx0 as f32), px(oy + cy0 as f32)),
                    size: size(px(t.content_width as f32), px(view_h as f32)),
                };
                if let Some(ready) = ready {
                    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                        mermaid::paint_ready(
                            window,
                            ox + cx0 as f32 - s.x as f32,
                            oy + cy0 as f32 - s.y as f32,
                            t.content_width,
                            t.art.height,
                            &ready,
                        );
                    });
                } else if let Some(label) = fail_label {
                    let pad = theme.boxes.mermaid.padding;
                    paint_fail_box(
                        window,
                        cx,
                        theme,
                        (
                            ox + cx0 as f32 - pad.left as f32,
                            oy + cy0 as f32 - pad.top as f32,
                            (t.content_width + pad.left + pad.right) as f32,
                            (view_h + pad.top + pad.bottom) as f32,
                        ),
                        &label,
                    );
                }
            } else {
                let well = is_scroll_well(t.kind, t.edit_source);
                let s = if well {
                    well_scroll.get(&t.block).copied().unwrap_or_default()
                } else {
                    WellScroll::default()
                };
                let view_h = if well { well_view_h(t) } else { t.art.height };
                let clip = Bounds {
                    origin: point(px(ox + cx0 as f32), px(oy + cy0 as f32)),
                    size: size(px(t.content_width as f32), px(view_h as f32)),
                };
                let paint_args = ctx.artifact(
                    ox + cx0 as f32 - s.x as f32,
                    oy + cy0 as f32 - s.y as f32,
                    t.kind,
                    t.align,
                    t.content_width,
                );
                if well {
                    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                        paint_artifact(&t.art, paint_args, window, cx);
                    });
                } else {
                    paint_artifact(&t.art, paint_args, window, cx);
                }
            }
            if let Some(label) = well_lang_for(t.kind, code_langs.get(&t.block).map(String::as_str))
            {
                paint_well_lang(
                    label,
                    (ox + cx0 as f32, oy + cy0 as f32),
                    t.content_width as f32,
                    well_padding(theme, t.kind),
                    theme,
                    window,
                    cx,
                );
            }
        }
    }

    fn paint_table(&mut self, f: &Frame, ctx: &PaintCtx<'_>, window: &mut Window, cx: &mut App) {
        let PaintCtx { ox, oy, theme, .. } = *ctx;
        let align_preview = {
            let v = self.state.read(cx);
            if v.table_ui.align_hover.is_some()
                && let Some(chrome) = v.table_ui.chrome
            {
                let mut fill = ShellTheme::from_app(&v.state.theme.app).selected_bg;
                fill.a *= 0.55;
                let rects: Vec<_> = f
                    .cells
                    .iter()
                    .filter_map(|c| {
                        let loc = v.state.doc.table_loc(c.block)?;
                        if loc.table == chrome.loc.table && loc.col == chrome.loc.col {
                            Some(c.rect_device)
                        } else {
                            None
                        }
                    })
                    .collect();
                Some((rects, fill))
            } else {
                None
            }
        };
        if let Some((rects, fill)) = align_preview {
            for r in rects {
                window.paint_quad(gpui::fill(ctx.bounds(r), fill));
            }
        }
        for c in &f.cells {
            let (cx0, cy0) = c.content_origin_device;
            paint_artifact(
                &c.art,
                ctx.artifact(
                    ox + cx0 as f32,
                    oy + cy0 as f32,
                    BlockKind::TableCell,
                    c.align,
                    c.content_width,
                ),
                window,
                cx,
            );
        }

        if let Some(guide) = col_resize_paint_args(self.state.read(cx), f, (ox, oy), theme) {
            paint_col_resize_guide(window, cx, &guide);
        }

        if let Some(chrome) = table_reorder_paint_args(self.state.read(cx), f, (ox, oy)) {
            paint_table_reorder(window, &chrome);
            if let Some(clone) = chrome.visual.clone {
                let clone_cells: Vec<_> = {
                    let v = self.state.read(cx);
                    f.cells
                        .iter()
                        .filter_map(|c| {
                            let loc = v.state.doc.table_loc(c.block)?;
                            if loc.table != clone.table {
                                return None;
                            }
                            let keep = if clone.row {
                                loc.row == clone.from
                            } else {
                                loc.col == clone.from
                            };
                            keep.then_some((
                                c.art.clone(),
                                c.content_origin_device,
                                c.align,
                                c.rect_device,
                            ))
                        })
                        .collect()
                };
                let rects: Vec<_> = clone_cells.iter().map(|c| c.3).collect();
                paint_clone_cell_borders(window, (ox, oy), clone, &rects, chrome.border_variant);
                for (art, (cx0, cy0), align, rect) in clone_cells {
                    paint_artifact(
                        &art,
                        ctx.artifact(
                            ox + (cx0 + clone.dx) as f32,
                            oy + (cy0 + clone.dy) as f32,
                            BlockKind::TableCell,
                            align,
                            rect.2,
                        ),
                        window,
                        cx,
                    );
                }
            }
        }
    }
}

fn paint_block_chrome(f: &Frame, ctx: &PaintCtx<'_>, window: &mut Window, cx: &mut App) {
    let PaintCtx { ox, oy, theme, .. } = *ctx;
    for d in &f.decorations {
        let ops = md_render::blocks::for_kind(d.kind).paint_decoration(d, theme);
        if let Some(clip) = d.clip_device {
            let bounds = ctx.bounds(clip);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for op in ops {
                    paint_block_op(op, ctx, window);
                }
            });
        } else {
            for op in ops {
                paint_block_op(op, ctx, window);
            }
        }
        let origin = d.gutter_label_at.or(d.gutter_dot);
        if let (Some(label), Some((lx, ly))) = (&d.gutter_label, origin) {
            let align_end = d.role == md_layout::box_tree::BoxRole::Slot;
            let size = if d.gutter_label_size > 0.0 {
                d.gutter_label_size
            } else {
                theme.type_scale.body.size_px
            };
            let color = if d.role == md_layout::box_tree::BoxRole::Slot {
                theme.paint.list_marker.hsla()
            } else {
                theme.type_role(BlockKind::Paragraph).color.hsla()
            };
            paint_gutter_label(
                label,
                (ox + lx as f32, oy + ly as f32),
                align_end,
                (theme.type_role(BlockKind::Paragraph).font(), size, color),
                window,
                cx,
            );
        }
    }

    let cell_grid: Vec<_> = f
        .cells
        .iter()
        .map(md_render::blocks::table::CellRect::of)
        .collect();
    for op in md_render::blocks::table::paint_cell_grid(&cell_grid, theme) {
        paint_block_op(op, ctx, window);
    }
}

fn paint_under_text(f: &Frame, wells: &[WellHit], ctx: &PaintCtx<'_>, window: &mut Window) {
    let PaintCtx {
        theme,
        paint,
        well_scroll,
        caret_block,
        ..
    } = *ctx;
    for r in &f.inline_code_device {
        if let Some(r) = overlay_in_well(*r, wells, &f.texts, well_scroll, None) {
            window.paint_quad(
                gpui::fill(ctx.bounds(r), theme.inline.inline_code_fill.hsla())
                    .corner_radii(px(theme.inline.inline_code_radius)),
            );
        }
    }

    for r in &f.search_device {
        if let Some(r) = overlay_in_well(*r, wells, &f.texts, well_scroll, None) {
            window.paint_quad(gpui::fill(ctx.bounds(r), paint.search_match.hsla()));
        }
    }

    for r in &f.search_active_device {
        if let Some(r) = overlay_in_well(*r, wells, &f.texts, well_scroll, None) {
            window.paint_quad(gpui::fill(ctx.bounds(r), paint.search_match_active.hsla()));
        }
    }

    for r in &f.selection_device {
        if let Some(r) = overlay_in_well(*r, wells, &f.texts, well_scroll, None) {
            window.paint_quad(gpui::fill(ctx.bounds(r), paint.selection.hsla()));
        }
    }

    for r in &f.ime_device {
        if let Some(r) = overlay_in_well(*r, wells, &f.texts, well_scroll, Some(caret_block)) {
            window.paint_quad(gpui::fill(ctx.bounds(r), paint.ime.hsla()));
        }
    }
}

fn paint_popovers(st: &PrepaintState, ctx: &PaintCtx<'_>, window: &mut Window) {
    let PaintCtx {
        ox,
        oy,
        theme,
        image_ready,
        failed_sources,
        math_ready,
        ..
    } = *ctx;
    if let Some(p) = st.popover.as_ref() {
        paint_popover(
            p,
            PopoverPaint {
                origin: (ox, oy),
                theme,
                images: image_ready,
                failed_sources,
            },
            window,
        );
    }
    if let Some(p) = st.math_popover.as_ref() {
        paint_math_popover(
            p,
            PopoverPaint {
                origin: (ox, oy),
                theme,
                images: image_ready,
                failed_sources,
            },
            math_ready,
            window,
        );
    }
}

fn paint_scrollbars(
    f: &Frame,
    wells: &[WellHit],
    chrome: &ChromeTokens,
    ctx: &PaintCtx<'_>,
    window: &mut Window,
) {
    for hit in wells {
        let s = ctx.well_scroll.get(&hit.id).copied().unwrap_or_default();
        if let Some(bar) = WellBar::vertical(hit.view_w, hit.view_h, s.y, hit.content_h, chrome) {
            paint_well_bar(window, ctx, chrome, hit.x, hit.y, bar);
        }
        if let Some(bar) = WellBar::horizontal(hit.view_w, hit.view_h, s.x, hit.content_w, chrome) {
            paint_well_bar(window, ctx, chrome, hit.x, hit.y, bar);
        }
    }
    if let Some(bar) =
        ScrollbarGeom::layout(f.viewport.0, f.viewport.1, f.scroll, f.total_height, chrome)
    {
        let radius = px(bar.thumb_w as f32 * 0.5);
        window.paint_quad(
            gpui::fill(
                ctx.bounds((bar.thumb_x, bar.track_y, bar.thumb_w, bar.track_h)),
                chrome.scrollbar_track.hsla(),
            )
            .corner_radii(radius),
        );
        window.paint_quad(
            gpui::fill(
                ctx.bounds((bar.thumb_x, bar.thumb_y, bar.thumb_w, bar.thumb_h)),
                chrome.scrollbar_thumb.hsla(),
            )
            .corner_radii(radius),
        );
    }
}

fn col_resize_paint_args(
    v: &EditorView,
    f: &Frame,
    origin: (f32, f32),
    theme: &DocumentTheme,
) -> Option<ColResizeGuide> {
    let drag = v.table_ui.col_resize?;
    let tracks = v.table_ui.col_widths.get(&drag.table)?;
    let left = *tracks.get(drag.col)?;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for c in &f.cells {
        let Some(loc) = v.state.doc.table_loc(c.block) else {
            continue;
        };
        if loc.table != drag.table {
            continue;
        }
        let (x, y, _, h) = c.rect_device;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y + h);
        any = true;
    }
    if !any {
        return None;
    }
    let seam_x = min_x + tracks.iter().take(drag.col + 1).sum::<Px>();
    let shell = ShellTheme::from_app(&v.state.theme.app);
    Some(ColResizeGuide {
        origin,
        table_y: min_y as f32,
        table_bottom: max_y as f32,
        seam_x: seam_x as f32,
        col: drag.col + 1,
        width_px: left.round() as i32,
        accent: shell.accent,
        panel_bg: shell.panel_bg,
        border: shell.border,
        text: shell.text,
        muted: shell.text_muted,
        font: theme.type_scale.code.font(),
    })
}

fn table_reorder_paint_args(
    v: &EditorView,
    f: &Frame,
    origin: (f32, f32),
) -> Option<ReorderChrome> {
    let cells: Vec<super::table_cols::CellHit> =
        f.cells.iter().map(|c| (c.block, c.rect_device)).collect();
    let visual = super::table_reorder::reorder_visual(
        &v.state.doc,
        &cells,
        v.table_ui.grip_hover,
        v.table_ui.reorder,
        v.table_ui.picker_open,
        v.table_ui.col_resize.is_some(),
    )?;
    let shell = ShellTheme::from_app(&v.state.theme.app);
    Some(ReorderChrome {
        origin,
        visual,
        accent: shell.accent,
        panel_bg: shell.panel_bg,
        border: shell.border,
        border_variant: shell.border_variant,
        editor_bg: shell.editor_bg,
        muted: shell.text_muted,
    })
}

fn paint_block_op(op: md_render::blocks::PaintOp, ctx: &PaintCtx<'_>, window: &mut Window) {
    match op {
        md_render::blocks::PaintOp::Fill { rect, color } => {
            window.paint_quad(gpui::fill(ctx.bounds(rect), color));
        }
        md_render::blocks::PaintOp::Round {
            rect,
            color,
            radius,
        } => {
            window.paint_quad(gpui::fill(ctx.bounds(rect), color).corner_radii(px(radius)));
        }
        md_render::blocks::PaintOp::RoundBorder {
            rect,
            fill,
            radius,
            border_width,
            border_color,
        } => {
            window.paint_quad(
                gpui::fill(ctx.bounds(rect), fill)
                    .corner_radii(px(radius))
                    .border_widths(px(border_width))
                    .border_color(border_color),
            );
        }
        md_render::blocks::PaintOp::Check {
            origin,
            size,
            color,
        } => {
            paint_task_check(window, ctx.bounds((origin.0, origin.1, size, size)), color);
        }
    }
}

fn paint_task_check(window: &mut Window, bounds: Bounds<Pixels>, color: Hsla) {
    let x = f32::from(bounds.origin.x);
    let y = f32::from(bounds.origin.y);
    let s = f32::from(bounds.size.width) / 16.0;
    let mut path = gpui::PathBuilder::stroke(px(2.0 * s));
    path.move_to(point(px(x + 4.0 * s), px(y + 8.5 * s)));
    path.line_to(point(px(x + 6.5 * s), px(y + 11.0 * s)));
    path.line_to(point(px(x + 12.0 * s), px(y + 4.5 * s)));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn collect_well_hits(
    texts: &[md_render::snapshot::TextPiece],
    mermaid_size: impl Fn(&md_render::snapshot::TextPiece) -> Option<(Px, Px)>,
) -> Vec<WellHit> {
    texts
        .iter()
        .filter(|t| is_scroll_well(t.kind, t.edit_source))
        .map(|t| {
            let (x, y) = t.content_origin_device;
            let (content_w, content_h) =
                mermaid_size(t).unwrap_or_else(|| (well_content_w(t), t.art.height));
            WellHit {
                id: t.block,
                x,
                y,
                view_w: t.content_width,
                view_h: well_view_h(t),
                content_w,
                content_h,
            }
        })
        .collect()
}

fn overlay_in_well(
    r: (Px, Px, Px, Px),
    wells: &[WellHit],
    texts: &[md_render::snapshot::TextPiece],
    scrolls: &HashMap<BlockId, WellScroll>,
    prefer: Option<BlockId>,
) -> Option<(Px, Px, Px, Px)> {
    let well = if let Some(id) = prefer {
        wells.iter().find(|w| w.id == id)
    } else if let Some(w) = wells
        .iter()
        .find(|w| origin_in(r, w.x, w.y, w.view_w, w.view_h))
    {
        Some(w)
    } else if texts.iter().any(|t| {
        !is_scroll_well(t.kind, t.edit_source)
            && origin_in(
                r,
                t.content_origin_device.0,
                t.content_origin_device.1,
                t.content_width,
                t.view_height,
            )
    }) {
        None
    } else {
        wells.iter().find(|w| {
            origin_in(
                r,
                w.x,
                w.y,
                w.view_w.max(w.content_w),
                w.view_h.max(w.content_h),
            )
        })
    };
    match well {
        Some(w) => {
            let s = scrolls.get(&w.id).copied().unwrap_or_default();
            let x0 = (r.0 - s.x).max(w.x);
            let y0 = (r.1 - s.y).max(w.y);
            let x1 = (r.0 - s.x + r.2).min(w.x + w.view_w);
            let y1 = (r.1 - s.y + r.3).min(w.y + w.view_h);
            if x1 <= x0 || y1 <= y0 {
                None
            } else {
                Some((x0, y0, x1 - x0, y1 - y0))
            }
        }
        None => Some(r),
    }
}

fn origin_in(r: (Px, Px, Px, Px), x: Px, y: Px, w: Px, h: Px) -> bool {
    r.0 >= x && r.0 < x + w && r.1 >= y && r.1 < y + h
}

fn paint_well_bar(
    window: &mut Window,
    ctx: &PaintCtx<'_>,
    chrome: &ChromeTokens,
    ox: Px,
    oy: Px,
    bar: WellBar,
) {
    let (track, thumb, thick) = if bar.vertical {
        (
            (ox + bar.thumb_x, oy + bar.hit_y, bar.thumb_w, bar.hit_h),
            (ox + bar.thumb_x, oy + bar.thumb_y, bar.thumb_w, bar.thumb_h),
            bar.thumb_w,
        )
    } else {
        (
            (ox + bar.hit_x, oy + bar.thumb_y, bar.hit_w, bar.thumb_h),
            (ox + bar.thumb_x, oy + bar.thumb_y, bar.thumb_w, bar.thumb_h),
            bar.thumb_h,
        )
    };
    let radius = px(thick as f32 * 0.5);
    window.paint_quad(
        gpui::fill(ctx.bounds(track), chrome.scrollbar_track.hsla()).corner_radii(radius),
    );
    window.paint_quad(
        gpui::fill(ctx.bounds(thumb), chrome.scrollbar_thumb.hsla()).corner_radii(radius),
    );
}
