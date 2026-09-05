use super::{ArtifactPaint, WellScroll};
use gpui::{App, Bounds, Hsla, Pixels, Point, TextRun, Window, hsla, point, px, size};
use md_content::gpui_theme::{ThemeColorExt, TypeRoleExt};
use md_content::shaper::{ShapeArtifact, ShapePart, band_align_shift};
use md_content::{images, math};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::inline::InlineAlign;
use md_theme::DocumentTheme;
use std::collections::HashMap;

pub(super) fn gpui_align(align: InlineAlign) -> gpui::TextAlign {
    match align {
        InlineAlign::Start => gpui::TextAlign::Left,
        InlineAlign::Center => gpui::TextAlign::Center,
        InlineAlign::End => gpui::TextAlign::Right,
    }
}

pub(super) fn paint_artifact(
    art: &md_content::shaper::ShapeArtifact,
    p: ArtifactPaint<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    if art.bands.is_empty() {
        paint_shaped_lines(art, p.x, p.y, p.align, window, cx);
        return;
    }
    let color = math::color_for(p.theme, p.kind);
    let mut y = p.y;
    for (band_ix, band) in art.bands.iter().enumerate() {
        let band_dx = band_align_shift(art, band_ix as u32, p.inline_align, p.inner) as f32;
        for part in &band.parts {
            match part {
                ShapePart::Text {
                    line,
                    x: px0,
                    dy,
                    byte_start,
                    byte_end,
                    ..
                } => {
                    if byte_start == byte_end {
                        continue;
                    }
                    let origin = point(
                        px(p.x + *px0 as f32 + band_dx),
                        px(y + band.text_dy as f32 + *dy as f32),
                    );
                    let row_h = px(art.row_advance as f32);
                    let _ = line.paint_background(origin, row_h, p.align, None, window, cx);
                    let _ = line.paint(origin, row_h, p.align, None, window, cx);
                }
                ShapePart::Math {
                    x: px0,
                    width,
                    paint_y,
                    latex,
                    display,
                    em,
                    slot_h,
                    fallback,
                    ..
                } => {
                    if let Some(line) = fallback {
                        if p.kind == BlockKind::Math || *display {
                            paint_placeholder_slot(
                                window,
                                cx,
                                p.theme,
                                p.kind,
                                (
                                    p.x + *px0 as f32 + band_dx,
                                    y + *paint_y as f32,
                                    *width as f32,
                                    *slot_h as f32,
                                ),
                                latex,
                            );
                            continue;
                        }
                        let origin =
                            point(px(p.x + *px0 as f32 + band_dx), px(y + *paint_y as f32));
                        let row_h = px(art.row_advance as f32);
                        let _ = line.paint_background(origin, row_h, p.align, None, window, cx);
                        let _ = line.paint(origin, row_h, p.align, None, window, cx);
                        continue;
                    }
                    let key = math::key_for(latex, *display, *em, color, p.dpr);
                    if let Some(ready) = p.lookup.get(&key) {
                        math::paint_ready(
                            window,
                            p.x + *px0 as f32 + band_dx,
                            y + *paint_y as f32,
                            ready,
                        );
                    } else if p.failed_math.contains(&key) {
                        paint_placeholder_slot(
                            window,
                            cx,
                            p.theme,
                            p.kind,
                            (
                                p.x + *px0 as f32 + band_dx,
                                y + *paint_y as f32,
                                *width as f32,
                                *slot_h as f32,
                            ),
                            latex,
                        );
                    }
                }
                ShapePart::Image {
                    x: px0,
                    paint_y,
                    dest,
                    slot_w,
                    slot_h,
                    fallback,
                    ..
                } => {
                    let x = p.x + *px0 as f32 + band_dx;
                    let y0 = y + *paint_y as f32;

                    if let Some(line) = fallback {
                        if p.kind == BlockKind::Image {
                            paint_placeholder_slot(
                                window,
                                cx,
                                p.theme,
                                p.kind,
                                (x, y0, *slot_w as f32, *slot_h as f32),
                                &placeholder_name(dest),
                            );
                            continue;
                        }
                        let origin = point(px(x), px(y0));
                        let row_h = px(art.row_advance as f32);
                        let _ = line.paint_background(origin, row_h, p.align, None, window, cx);
                        let _ = line.paint(origin, row_h, p.align, None, window, cx);
                        continue;
                    }
                    let key = images::display_key(dest, *slot_w as f32, *slot_h as f32, p.dpr);
                    if let Some(ready) = p.images.get(&key) {
                        images::paint_ready(window, x, y0, *slot_w as f32, *slot_h as f32, ready);
                    } else {
                        paint_placeholder_slot(
                            window,
                            cx,
                            p.theme,
                            p.kind,
                            (x, y0, *slot_w as f32, *slot_h as f32),
                            &placeholder_name(dest),
                        );
                    }
                }
            }
        }
        y += band.height as f32;
    }
}

fn paint_shaped_lines(
    art: &ShapeArtifact,
    x: f32,
    y0: f32,
    align: gpui::TextAlign,
    window: &mut Window,
    cx: &mut App,
) {
    let row_h = px(art.row_advance as f32);
    let mut y = y0 as Px;
    for (i, line) in art.lines.iter().enumerate() {
        let origin = point(px(x), px(y as f32));
        let _ = line.paint_background(origin, row_h, align, None, window, cx);
        let colors = art.line_colors.get(i).filter(|c| !c.is_empty());
        if let Some(colors) = colors {
            paint_line_colors(line, origin, row_h, colors, window, cx);
        } else {
            let _ = line.paint(origin, row_h, align, None, window, cx);
        }
        y += (line.wrap_boundaries.len() + 1) as Px * art.row_advance;
    }
}

fn paint_line_colors(
    line: &gpui::WrappedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    colors: &[(u32, Hsla)],
    window: &mut Window,
    cx: &mut App,
) {
    let layout = line.unwrapped_layout.as_ref();
    let wrap_boundaries = line.wrap_boundaries.as_slice();
    let line_bounds = Bounds::new(
        origin,
        size(
            layout.width,
            line_height * (wrap_boundaries.len() as f32 + 1.),
        ),
    );
    window.paint_layer(line_bounds, |window| {
        let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
        let baseline_offset = point(px(0.), padding_top + layout.ascent);
        let text_system = cx.text_system().clone();
        let mut glyph_origin = point(origin.x, origin.y);
        let mut prev_glyph_position = Point::default();
        let mut wraps = wrap_boundaries.iter().peekable();
        let mut color_ix = 0usize;
        let mut color_end = 0usize;
        let mut color = colors.first().map(|c| c.1).unwrap_or_default();
        for (run_ix, run) in layout.runs.iter().enumerate() {
            let max_glyph_size = text_system.bounding_box(run.font_id, layout.font_size).size;
            for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                glyph_origin.x += glyph.position.x - prev_glyph_position.x;
                if wraps
                    .peek()
                    .is_some_and(|b| b.run_ix == run_ix && b.glyph_ix == glyph_ix)
                {
                    wraps.next();
                    glyph_origin.x = origin.x;
                    glyph_origin.y += line_height;
                }
                prev_glyph_position = glyph.position;
                while glyph.index >= color_end && color_ix < colors.len() {
                    color_end += colors[color_ix].0 as usize;
                    color = colors[color_ix].1;
                    color_ix += 1;
                }
                let max_glyph_bounds = Bounds {
                    origin: glyph_origin,
                    size: max_glyph_size,
                };
                if max_glyph_bounds.intersects(&window.content_mask().bounds) {
                    if glyph.is_emoji {
                        let _ = window.paint_emoji(
                            glyph_origin + baseline_offset,
                            run.font_id,
                            glyph.id,
                            layout.font_size,
                        );
                    } else {
                        let _ = window.paint_glyph(
                            glyph_origin + baseline_offset,
                            run.font_id,
                            glyph.id,
                            layout.font_size,
                            color,
                        );
                    }
                }
            }
        }
    });
}

pub(super) fn is_scroll_well(kind: BlockKind, edit_source: bool) -> bool {
    edit_source
        || matches!(
            kind,
            BlockKind::CodeBlock | BlockKind::Math | BlockKind::Mermaid | BlockKind::Image
        )
}

pub(super) fn well_view_h(t: &md_render::snapshot::TextPiece) -> Px {
    t.view_height
}

pub(super) fn well_content_w(t: &md_render::snapshot::TextPiece) -> Px {
    let from_lines = t.art.max_line_width;
    let from_parts = t
        .art
        .bands
        .iter()
        .flat_map(|b| b.parts.iter())
        .map(|p| match p {
            ShapePart::Math { x, width, .. } | ShapePart::Image { x, width, .. } => *x + *width,
            ShapePart::Text { x, line, .. } => *x + f32::from(line.width()) as Px,
        })
        .fold(0.0, f64::max);
    from_lines.max(from_parts)
}

pub(super) fn well_scroll_xy(scrolls: &HashMap<BlockId, WellScroll>, id: BlockId) -> (Px, Px) {
    scrolls.get(&id).map(|s| (s.x, s.y)).unwrap_or((0.0, 0.0))
}

fn placeholder_name(dest: &str) -> String {
    let name = dest
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(dest);
    if name.is_empty() {
        md_i18n::fmt::image_fallback_label()
    } else {
        format!("▣ {name}")
    }
}

fn paint_placeholder_slot(
    window: &mut Window,
    cx: &mut App,
    theme: &DocumentTheme,
    kind: BlockKind,
    rect: (f32, f32, f32, f32),
    label: &str,
) {
    let pad = match kind {
        BlockKind::Math => theme.boxes.math.padding,
        BlockKind::Mermaid => theme.boxes.mermaid.padding,
        _ => md_layout::style::Edges::ZERO,
    };
    let (x, y, w, h) = rect;
    paint_fail_box(
        window,
        cx,
        theme,
        (
            x - pad.left as f32,
            y - pad.top as f32,
            w + (pad.left + pad.right) as f32,
            h + (pad.top + pad.bottom) as f32,
        ),
        label,
    );
}

#[derive(Clone, Copy)]
struct DashStroke {
    t: f32,
    dash: f32,
    gap: f32,
    color: Hsla,
}

fn dash_h(window: &mut Window, x: f32, y: f32, len: f32, stroke: DashStroke) {
    if len <= 0.0 || stroke.t <= 0.0 {
        return;
    }
    let mut x0 = x;
    let end = x + len;
    while x0 < end {
        let w = stroke.dash.min(end - x0);
        if w > 0.25 {
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(x0), px(y)),
                    size: size(px(w), px(stroke.t)),
                },
                stroke.color,
            ));
        }
        x0 += stroke.dash + stroke.gap;
    }
}

fn dash_v(window: &mut Window, x: f32, y: f32, len: f32, stroke: DashStroke) {
    if len <= 0.0 || stroke.t <= 0.0 {
        return;
    }
    let mut y0 = y;
    let end = y + len;
    while y0 < end {
        let h = stroke.dash.min(end - y0);
        if h > 0.25 {
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(x), px(y0)),
                    size: size(px(stroke.t), px(h)),
                },
                stroke.color,
            ));
        }
        y0 += stroke.dash + stroke.gap;
    }
}

fn paint_corner_arc(
    window: &mut Window,
    center: (f32, f32),
    radius: f32,
    a0: f32,
    a1: f32,
    stroke: DashStroke,
) {
    let n = 5;
    let t = stroke.t;
    for i in 0..n {
        let a = a0 + (a1 - a0) * (i as f32 / (n - 1) as f32);
        let px0 = center.0 + radius * a.cos() - t * 0.5;
        let py0 = center.1 + radius * a.sin() - t * 0.5;
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(px(px0), px(py0)),
                size: size(px(t), px(t)),
            },
            stroke.color,
        ));
    }
}

pub(super) fn paint_fail_box(
    window: &mut Window,
    cx: &mut App,
    theme: &DocumentTheme,
    rect: (f32, f32, f32, f32),
    label: &str,
) {
    let (x, y, w, h) = rect;
    let w = w.max(1.0);
    let h = h.max(1.0);
    let bounds = Bounds {
        origin: point(px(x), px(y)),
        size: size(px(w), px(h)),
    };
    let fill = theme.inline.image_fill.hsla();
    let stroke = DashStroke {
        t: 1.0,
        dash: theme.decoration.placeholder_dash as f32,
        gap: theme.decoration.placeholder_gap as f32,
        color: theme.inline.image_border.hsla(),
    };
    let radius = theme.decoration.code_radius.min(w * 0.5).min(h * 0.5);
    window.paint_quad(gpui::fill(bounds, fill).corner_radii(px(radius)));
    let inner_w = (w - 2.0 * radius).max(0.0);
    let inner_h = (h - 2.0 * radius).max(0.0);
    dash_h(window, x + radius, y, inner_w, stroke);
    dash_h(window, x + radius, y + h - stroke.t, inner_w, stroke);
    dash_v(window, x, y + radius, inner_h, stroke);
    dash_v(window, x + w - stroke.t, y + radius, inner_h, stroke);
    let pi = std::f32::consts::PI;
    paint_corner_arc(
        window,
        (x + radius, y + radius),
        radius,
        pi,
        1.5 * pi,
        stroke,
    );
    paint_corner_arc(
        window,
        (x + w - radius, y + radius),
        radius,
        1.5 * pi,
        2.0 * pi,
        stroke,
    );
    paint_corner_arc(
        window,
        (x + w - radius, y + h - radius),
        radius,
        0.0,
        0.5 * pi,
        stroke,
    );
    paint_corner_arc(
        window,
        (x + radius, y + h - radius),
        radius,
        0.5 * pi,
        pi,
        stroke,
    );
    if h < 16.0 || w < 36.0 || label.is_empty() {
        return;
    }
    let size_px = theme.decoration.placeholder_label_size;
    let color = theme.type_scale.image.color.hsla();
    let run = TextRun {
        len: label.len(),
        font: theme.type_scale.image.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Ok(lines) = window.text_system().shape_text(
        label.to_string().into(),
        px(size_px),
        &[run],
        Some(px((w - 16.0).max(8.0))),
        None,
    ) else {
        return;
    };
    if let Some(line) = lines.into_iter().next() {
        let lw = f32::from(line.width());
        let lh = size_px * theme.type_scale.image.line_height_em;
        let _ = line.paint(
            point(
                px(x + ((w - lw) * 0.5).max(8.0)),
                px(y + ((h - lh) * 0.5).max(0.0)),
            ),
            px(lh),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

pub(super) fn well_lang_for(kind: BlockKind, fence_lang: Option<&str>) -> Option<&str> {
    match kind {
        BlockKind::CodeBlock => fence_lang.filter(|s| !s.is_empty()),
        BlockKind::Math => Some("math"),
        BlockKind::Mermaid => Some("mermaid"),
        _ => None,
    }
}

pub(super) fn well_padding(theme: &DocumentTheme, kind: BlockKind) -> md_layout::style::Edges {
    match kind {
        BlockKind::CodeBlock => theme.boxes.code.padding,
        BlockKind::Math => theme.boxes.math.padding,
        BlockKind::Mermaid => theme.boxes.mermaid.padding,
        _ => md_layout::style::Edges::ZERO,
    }
}

pub(super) fn well_lang_anchor(
    content_origin: (f32, f32),
    content_width: f32,
    padding: md_layout::style::Edges,
    right: f32,
    top: f32,
) -> (f32, f32) {
    (
        content_origin.0 + content_width + padding.right as f32 - right,
        content_origin.1 - padding.top as f32 + top,
    )
}

pub(super) fn paint_well_lang(
    lang: &str,
    content_origin: (f32, f32),
    content_width: f32,
    padding: md_layout::style::Edges,
    theme: &DocumentTheme,
    window: &mut Window,
    cx: &mut App,
) {
    if lang.is_empty() {
        return;
    }
    let size = theme.decoration.well_lang_size;
    let run = TextRun {
        len: lang.len(),
        font: theme.type_scale.code.font(),
        color: theme.decoration.well_lang.hsla(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Ok(lines) =
        window
            .text_system()
            .shape_text(lang.to_string().into(), px(size), &[run], None, None)
    else {
        return;
    };
    let Some(line) = lines.into_iter().next() else {
        return;
    };
    let (x, y) = well_lang_anchor(
        content_origin,
        content_width,
        padding,
        theme.decoration.well_lang_right as f32,
        theme.decoration.well_lang_top as f32,
    );
    let w = f32::from(line.width());
    let _ = line.paint(
        point(px(x - w), px(y)),
        px(size + 3.5),
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    );
}

pub(super) fn paint_gutter_label(
    label: &str,
    origin: (f32, f32),
    align_end: bool,
    style: (gpui::Font, f32, Hsla),
    window: &mut Window,
    cx: &mut App,
) {
    if label.is_empty() {
        return;
    }
    let (font, size_px, color) = style;
    let run = TextRun {
        len: label.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Ok(lines) =
        window
            .text_system()
            .shape_text(label.to_string().into(), px(size_px), &[run], None, None)
    else {
        return;
    };
    let row_h = px(size_px * 1.75);
    for line in lines {
        let w = f32::from(line.width());
        let px_x = if align_end { origin.0 - w } else { origin.0 };
        let _ = line.paint(
            point(px(px_x), px(origin.1)),
            row_h,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

pub(super) fn paint_diag_overlay(
    origin: (f32, f32),
    labels: &[String],
    theme: &DocumentTheme,
    window: &mut Window,
    cx: &mut App,
) {
    if labels.is_empty() {
        return;
    }
    let mut y = origin.1;
    for label in labels {
        paint_gutter_label(
            label,
            (origin.0, y),
            false,
            (theme.type_scale.code.font(), 12.0, theme.app.ok.hsla()),
            window,
            cx,
        );
        y += 16.0;
    }
}

pub(super) struct ColResizeGuide {
    pub origin: (f32, f32),
    pub table_y: f32,
    pub table_bottom: f32,
    pub seam_x: f32,
    pub col: usize,
    pub width_px: i32,
    pub accent: Hsla,
    pub panel_bg: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub font: gpui::Font,
}

pub(super) fn paint_col_resize_guide(window: &mut Window, cx: &mut App, g: &ColResizeGuide) {
    let ox = g.origin.0;
    let oy = g.origin.1;
    let line_top = g.table_y - 14.0;
    let line_h = (g.table_bottom - line_top).max(0.0);
    dash_v(
        window,
        ox + g.seam_x,
        oy + line_top,
        line_h,
        DashStroke {
            t: 1.0,
            dash: 4.0,
            gap: 3.0,
            color: g.accent,
        },
    );

    let hw = 22.0;
    let hh = 14.0;
    let hx = ox + g.seam_x - hw * 0.5;
    let hy = oy + line_top;
    let handle = Bounds {
        origin: point(px(hx), px(hy)),
        size: size(px(hw), px(hh)),
    };
    window.paint_quad(
        gpui::fill(handle, g.panel_bg)
            .corner_radii(px(3.0))
            .border_widths(px(1.0))
            .border_color(g.border),
    );

    let bar_w = 10.0;
    let bar_h = 1.5;
    let bx = hx + (hw - bar_w) * 0.5;
    let by = hy + hh * 0.5 - bar_h * 0.5;
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(px(bx), px(by)),
            size: size(px(bar_w), px(bar_h)),
        },
        g.accent,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(px(bx), px(by - 3.0)),
            size: size(px(1.5), px(8.0)),
        },
        g.accent,
    ));
    window.paint_quad(gpui::fill(
        Bounds {
            origin: point(px(bx + bar_w - 1.5), px(by - 3.0)),
            size: size(px(1.5), px(8.0)),
        },
        g.accent,
    ));

    let label = md_i18n::fmt::col_width(g.col, g.width_px);
    let unit = " px";
    let full = format!("{label}{unit}");
    let size_px = 11.0;
    let run_label = TextRun {
        len: label.len(),
        font: g.font.clone(),
        color: g.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let run_unit = TextRun {
        len: unit.len(),
        font: g.font.clone(),
        color: g.muted,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Ok(lines) = window.text_system().shape_text(
        full.into(),
        px(size_px),
        &[run_label, run_unit],
        None,
        None,
    ) else {
        return;
    };
    let Some(line) = lines.into_iter().next() else {
        return;
    };
    let tw = f32::from(line.width());
    let pad_x = 6.0;
    let pad_y = 3.0;
    let bh = size_px + pad_y * 2.0;
    let bw = tw + pad_x * 2.0;
    let badge_x = hx + hw + 6.0;
    let badge_y = hy + (hh - bh) * 0.5;
    window.paint_quad(
        gpui::fill(
            Bounds {
                origin: point(px(badge_x), px(badge_y)),
                size: size(px(bw), px(bh)),
            },
            g.panel_bg,
        )
        .corner_radii(px(5.0))
        .border_widths(px(1.0))
        .border_color(g.border),
    );
    let _ = line.paint(
        point(px(badge_x + pad_x), px(badge_y + pad_y * 0.3)),
        px(size_px * 1.4),
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    );
}

pub(super) struct ReorderChrome {
    pub origin: (f32, f32),
    pub visual: super::table_reorder::ReorderVisual,
    pub accent: Hsla,
    pub panel_bg: Hsla,
    pub border: Hsla,
    pub border_variant: Hsla,
    pub editor_bg: Hsla,
    pub muted: Hsla,
}

pub(super) fn paint_table_reorder(window: &mut Window, g: &ReorderChrome) {
    let ox = g.origin.0;
    let oy = g.origin.1;
    if let Some((x, y, w, h)) = g.visual.ghost {
        let mut fill = g.accent;
        fill.a *= 0.07;
        let bounds = Bounds {
            origin: point(px(ox + x as f32), px(oy + y as f32)),
            size: size(px(w as f32), px(h as f32)),
        };
        window.paint_quad(
            gpui::fill(bounds, fill)
                .corner_radii(px(3.0))
                .border_widths(px(1.0))
                .border_color(g.accent),
        );
        let stroke = DashStroke {
            t: 1.0,
            dash: 4.0,
            gap: 3.0,
            color: g.accent,
        };
        dash_h(window, ox + x as f32, oy + y as f32, w as f32, stroke);
        dash_h(
            window,
            ox + x as f32,
            oy + (y + h) as f32 - 1.0,
            w as f32,
            stroke,
        );
        dash_v(window, ox + x as f32, oy + y as f32, h as f32, stroke);
        dash_v(
            window,
            ox + (x + w) as f32 - 1.0,
            oy + y as f32,
            h as f32,
            stroke,
        );
    }
    if let Some(drop) = g.visual.drop {
        paint_drop_line(window, ox, oy, drop, g.accent);
    }
    if let Some(clone) = g.visual.clone {
        let shadow = Bounds {
            origin: point(px(ox + clone.x as f32 + 2.0), px(oy + clone.y as f32 + 6.0)),
            size: size(px(clone.w as f32), px(clone.h as f32)),
        };
        window.paint_quad(gpui::fill(shadow, hsla(0.0, 0.0, 0.0, 0.28)).corner_radii(px(5.0)));
        let bounds = Bounds {
            origin: point(px(ox + clone.x as f32), px(oy + clone.y as f32)),
            size: size(px(clone.w as f32), px(clone.h as f32)),
        };
        window.paint_quad(
            gpui::fill(bounds, g.editor_bg)
                .corner_radii(px(5.0))
                .border_widths(px(1.0))
                .border_color(g.border),
        );
        if clone.row {
            let gx = ox + clone.x as f32 + 3.5;
            let gy = oy + clone.y as f32 + (clone.h as f32 - 20.0) * 0.5;
            paint_grip_dots(window, gx, gy, true, g.muted);
        } else {
            let gx = ox + clone.x as f32 + (clone.w as f32 - 20.0) * 0.5;
            let gy = oy + clone.y as f32 + (clone.cap as f32 - 20.0).max(0.0) * 0.5;
            paint_grip_dots(window, gx, gy, false, g.muted);
        }
    }
    if let Some((x, y)) = g.visual.row_grip {
        paint_grip_chip(window, ox + x as f32, oy + y as f32, true, g);
    }
    if let Some((x, y)) = g.visual.col_grip {
        paint_grip_chip(window, ox + x as f32, oy + y as f32, false, g);
    }
}

pub(super) fn paint_clone_cell_borders(
    window: &mut Window,
    origin: (f32, f32),
    clone: super::table_reorder::CloneGuide,
    rects: &[(Px, Px, Px, Px)],
    color: Hsla,
) {
    let ox = origin.0;
    let oy = origin.1;
    let on_outer = |a: Px, b: Px| (a - b).abs() < 0.75;
    let cx0 = clone.x;
    let cy0 = clone.y;
    for &(x, y, w, h) in rects {
        let x = x + clone.dx;
        let y = y + clone.dy;
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        if !on_outer(x, cx0) {
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(ox + x as f32), px(oy + y as f32)),
                    size: size(px(1.0), px(h as f32)),
                },
                color,
            ));
        }
        if !on_outer(y, cy0) {
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(ox + x as f32), px(oy + y as f32)),
                    size: size(px(w as f32), px(1.0)),
                },
                color,
            ));
        }
    }
}

fn paint_grip_chip(window: &mut Window, x: f32, y: f32, row: bool, g: &ReorderChrome) {
    window.paint_quad(
        gpui::fill(
            Bounds {
                origin: point(px(x), px(y)),
                size: size(px(20.0), px(20.0)),
            },
            g.panel_bg,
        )
        .corner_radii(px(5.0))
        .border_widths(px(1.0))
        .border_color(g.border),
    );
    paint_grip_dots(window, x, y, row, g.muted);
}

fn paint_grip_dots(window: &mut Window, x: f32, y: f32, row: bool, color: Hsla) {
    let dots: [(f32, f32); 6] = if row {
        [
            (7.7, 4.5),
            (12.3, 4.5),
            (7.7, 9.5),
            (12.3, 9.5),
            (7.7, 14.5),
            (12.3, 14.5),
        ]
    } else {
        [
            (4.5, 7.7),
            (4.5, 12.3),
            (9.5, 7.7),
            (9.5, 12.3),
            (14.5, 7.7),
            (14.5, 12.3),
        ]
    };
    for (dx, dy) in dots {
        window.paint_quad(
            gpui::fill(
                Bounds {
                    origin: point(px(x + dx - 1.2), px(y + dy - 1.2)),
                    size: size(px(2.4), px(2.4)),
                },
                color,
            )
            .corner_radii(px(1.2)),
        );
    }
}

fn paint_drop_line(
    window: &mut Window,
    ox: f32,
    oy: f32,
    drop: super::table_reorder::DropGuide,
    accent: Hsla,
) {
    match drop {
        super::table_reorder::DropGuide::H { y, x0, x1 } => {
            let x = ox + x0 as f32;
            let yy = oy + y as f32 - 1.0;
            let w = (x1 - x0) as f32;
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(x), px(yy)),
                    size: size(px(w.max(1.0)), px(2.0)),
                },
                accent,
            ));
            paint_drop_dot(window, x - 4.0, yy - 3.0, accent);
            paint_drop_dot(window, x + w - 4.0, yy - 3.0, accent);
        }
        super::table_reorder::DropGuide::V { x, y0, y1 } => {
            let xx = ox + x as f32 - 1.0;
            let y = oy + y0 as f32;
            let h = (y1 - y0) as f32;
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: point(px(xx), px(y)),
                    size: size(px(2.0), px(h.max(1.0))),
                },
                accent,
            ));
            paint_drop_dot(window, xx - 3.0, y - 4.0, accent);
            paint_drop_dot(window, xx - 3.0, y + h - 4.0, accent);
        }
    }
}

fn paint_drop_dot(window: &mut Window, x: f32, y: f32, accent: Hsla) {
    window.paint_quad(
        gpui::fill(
            Bounds {
                origin: point(px(x), px(y)),
                size: size(px(8.0), px(8.0)),
            },
            accent,
        )
        .corner_radii(px(4.0)),
    );
}

#[cfg(test)]
mod tests {
    use super::{placeholder_name, well_lang_anchor, well_lang_for};
    use md_core::block::BlockKind;
    use md_layout::style::Edges;

    #[test]
    fn well_lang_labels_match_v2_corner_tags() {
        assert_eq!(well_lang_for(BlockKind::CodeBlock, None), None);
        assert_eq!(well_lang_for(BlockKind::CodeBlock, Some("")), None);
        assert_eq!(
            well_lang_for(BlockKind::CodeBlock, Some("rust")),
            Some("rust")
        );
        assert_eq!(well_lang_for(BlockKind::Math, None), Some("math"));
        assert_eq!(well_lang_for(BlockKind::Mermaid, None), Some("mermaid"));
        assert_eq!(well_lang_for(BlockKind::Paragraph, Some("rust")), None);
    }

    #[test]
    fn well_lang_anchor_sits_inset_from_well_edges() {
        let pad = Edges {
            top: 24.0,
            right: 16.0,
            bottom: 12.0,
            left: 16.0,
        };
        let (x, y) = well_lang_anchor((40.0, 30.0), 200.0, pad, 10.0, 6.0);
        assert_eq!(x, 40.0 + 200.0 + 16.0 - 10.0);
        assert_eq!(y, 30.0 - 24.0 + 6.0);
    }

    #[test]
    fn placeholder_names_stay_verbatim_until_the_empty_fallback() {
        assert_eq!(placeholder_name("C:\\docs\\cat.png"), "▣ cat.png");
        assert_eq!(placeholder_name("img/cat.png"), "▣ cat.png");
        assert_eq!(placeholder_name("cat.png"), "▣ cat.png");
        assert_eq!(
            placeholder_name(""),
            md_i18n::fmt::image_fallback_label_in(md_i18n::current())
        );
    }
}
