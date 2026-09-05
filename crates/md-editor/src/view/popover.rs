use super::{ImagePopover, MathPopover, MathPopoverPlan, PopoverPlan};
use gpui::{Bounds, Window, point, px, size};
use md_content::gpui_theme::ThemeColorExt;
use md_content::shaper::GpuiShaper;
use md_content::{images, math};
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::{Cursor, Doc};
use md_layout::style::BoxLayoutEnvironment;
use md_render::frame::caret_logical;
use md_render::snap::SnapOperator;
use md_render::snapshot::Frame;
use md_theme::{DocumentTheme, ThemeColor};
use std::collections::{HashMap, HashSet};

pub(super) struct PopoverInput<'a> {
    pub theme: &'a DocumentTheme,
    pub shaper: &'a GpuiShaper,
    pub env: BoxLayoutEnvironment,
    pub snap: &'a SnapOperator,
    pub scroll: Px,
    pub viewport: (Px, Px),
    pub dpr: f64,

    pub sizes: &'a HashMap<images::SourceKey, (u32, u32)>,

    pub link_dests: &'a HashMap<u32, String>,

    pub math_metrics: &'a math::MathMetrics,
}

pub(super) fn plan(doc: &Doc, frame: &Frame, input: &PopoverInput<'_>) -> Option<ImagePopover> {
    let img = doc.revealed_image()?;
    if img.dest.trim().is_empty() {
        return None;
    }
    let dest = input.link_dests.get(&img.link)?.clone();
    let plan = geometry(&dest, img.display.start, img.block, frame, input);
    Some(ImagePopover { dest, plan })
}

pub(super) fn plan_math(doc: &Doc, frame: &Frame, input: &PopoverInput<'_>) -> Option<MathPopover> {
    let m = doc.revealed_math()?;
    if m.display_math {
        return None;
    }
    let kind = doc.kind(m.block)?;
    let role = input.theme.type_role(kind);
    let font_size = role.size_px;
    let color = math::color_for(input.theme, kind);

    let recorded = math::metric(input.math_metrics, m.latex, m.display_math);
    if matches!(recorded, Some(None)) {
        return None;
    }

    let em = recorded
        .flatten()
        .unwrap_or_else(|| math::MathEm::estimate(m.latex));

    let key = math::key_for(m.latex, m.display_math, font_size, color, input.dpr);
    let plan = math_geometry(
        em,
        font_size,
        key,
        color,
        m.display.start,
        m.block,
        frame,
        input,
    );
    Some(MathPopover { plan })
}

fn geometry(
    dest: &str,
    offset: usize,
    block: BlockId,
    frame: &Frame,
    input: &PopoverInput<'_>,
) -> Option<PopoverPlan> {
    let &(iw, ih) = input.sizes.get(dest)?;
    let d = &input.theme.decoration;
    let (fit_w, fit_h) = images::contain_fit(
        iw as f32,
        ih as f32,
        d.image_popover_max_width as f32,
        d.image_popover_max_height as f32,
    );
    let dpr = images::dpr_from_q(images::dpr_q(input.dpr));
    let slot_w = images::snap_css(fit_w, dpr);
    let slot_h = images::snap_css(fit_h, dpr);

    let pad = d.popover_pad;
    let plate_w = slot_w as Px + pad * 2.0;
    let plate_h = slot_h as Px + pad * 2.0;

    let anchor = anchor_at(offset, block, frame, input)?;
    let (left, top) = place(anchor, (plate_w, plate_h), d.popover_gap, input.viewport)?;

    Some(PopoverPlan {
        plate: (left, top, plate_w, plate_h),
        image_at: (left + pad, top + pad),
        image_size: (slot_w as Px, slot_h as Px),
        key: images::display_key(dest, slot_w, slot_h, input.dpr),
    })
}

#[allow(clippy::too_many_arguments)]
fn math_geometry(
    em: math::MathEm,
    font_size: f32,
    key: math::MathKey,
    color: ThemeColor,
    offset: usize,
    block: BlockId,
    frame: &Frame,
    input: &PopoverInput<'_>,
) -> Option<MathPopoverPlan> {
    let d = &input.theme.decoration;
    let dpr = images::dpr_from_q(images::dpr_q(input.dpr));
    let slot_w = em.box_width(font_size, dpr) as Px;
    let slot_h = em.box_height(font_size, dpr) as Px;

    let pad = d.popover_pad;
    let plate_w = slot_w + pad * 2.0;
    let plate_h = slot_h + pad * 2.0;

    let anchor = anchor_at(offset, block, frame, input)?;
    let (left, top) = place(anchor, (plate_w, plate_h), d.popover_gap, input.viewport)?;

    Some(MathPopoverPlan {
        plate: (left, top, plate_w, plate_h),
        math_at: (left + pad, top + pad),
        key,
        color,
    })
}

fn anchor_at(
    offset: usize,
    block: BlockId,
    frame: &Frame,
    input: &PopoverInput<'_>,
) -> Option<Anchor> {
    let cur = Cursor { block, offset };
    let (ax, ay, ah) = caret_logical(&frame.assembly, &frame.spans, input.shaper, input.env, cur)?;

    let row_top = input.snap.snap(ay - input.scroll);
    let row_bottom = input.snap.snap(ay + ah - input.scroll);
    Some(Anchor {
        x: ax,
        row_top,
        row_bottom,
    })
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Anchor {
    pub x: Px,
    pub row_top: Px,
    pub row_bottom: Px,
}

pub(super) fn place(
    anchor: Anchor,
    plate: (Px, Px),
    gap: Px,
    viewport: (Px, Px),
) -> Option<(Px, Px)> {
    if anchor.row_bottom <= 0.0 || anchor.row_top >= viewport.1 {
        return None;
    }
    let (plate_w, plate_h) = plate;
    let above_top = anchor.row_top - gap - plate_h;
    let below_top = anchor.row_bottom + gap;
    let mut top = if above_top >= 0.0 {
        above_top
    } else if below_top + plate_h <= viewport.1 {
        below_top
    } else {
        above_top
    };
    top = top.clamp(0.0, (viewport.1 - plate_h).max(0.0));
    let left = anchor.x.clamp(0.0, (viewport.0 - plate_w).max(0.0));
    Some((left, top))
}

pub(super) struct PopoverPaint<'a> {
    pub origin: (f32, f32),
    pub theme: &'a DocumentTheme,
    pub images: &'a HashMap<images::DisplayKey, images::ReadyImage>,
    pub failed_sources: &'a HashSet<images::SourceKey>,
}

fn paint_plate(
    plate: (Px, Px, Px, Px),
    origin: (f32, f32),
    theme: &DocumentTheme,
    window: &mut Window,
) {
    let (ox, oy) = origin;
    let (x, y, w, h) = plate;
    let app = theme.app;
    window.paint_quad(
        gpui::fill(
            Bounds {
                origin: point(px(ox + x as f32), px(oy + y as f32)),
                size: size(px(w as f32), px(h as f32)),
            },
            app.bar_bg.hsla(),
        )
        .corner_radii(px(4.0))
        .border_widths(px(1.0))
        .border_color(app.border.hsla()),
    );
}

pub(super) fn paint_popover(popover: &ImagePopover, p: PopoverPaint<'_>, window: &mut Window) {
    if p.failed_sources.contains(popover.dest.as_str()) {
        return;
    }

    let Some(plan) = popover.plan.as_ref() else {
        return;
    };
    let Some(ready) = p.images.get(&plan.key) else {
        return;
    };

    let (ox, oy) = p.origin;
    paint_plate(plan.plate, p.origin, p.theme, window);
    images::paint_ready(
        window,
        ox + plan.image_at.0 as f32,
        oy + plan.image_at.1 as f32,
        plan.image_size.0 as f32,
        plan.image_size.1 as f32,
        ready,
    );
}

pub(super) fn paint_math_popover(
    popover: &MathPopover,
    p: PopoverPaint<'_>,
    math: &HashMap<math::MathKey, math::ReadyImage>,
    window: &mut Window,
) {
    let Some(plan) = popover.plan.as_ref() else {
        return;
    };
    let Some(ready) = math.get(&plan.key) else {
        return;
    };
    let (ox, oy) = p.origin;
    paint_plate(plan.plate, p.origin, p.theme, window);
    math::paint_ready(
        window,
        ox + plan.math_at.0 as f32,
        oy + plan.math_at.1 as f32,
        ready,
    );
}
