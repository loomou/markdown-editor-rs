use super::metrics::MathEm;
use super::{MATH_PAD, RasterOut, RasterSpec};
use crate::pixels::{ReadyImage, decode_png, paint_at};
use gpui::Window;
use md_theme::ThemeColor;
use ratex_layout::engine::layout;
use ratex_layout::layout_options::LayoutOptions;
use ratex_layout::to_display::to_display_list;
use ratex_parser::parse;
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::color::Color;
use ratex_types::display_item::DisplayList;
use ratex_types::math_style::MathStyle;

const MAX_RASTER_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_RASTER_BYTES: u64 = MAX_RASTER_PIXELS * 4;

struct LaidOut {
    em: MathEm,
    list: DisplayList,
}

fn layout_formula(latex: &str, display: bool, color: ThemeColor) -> Option<LaidOut> {
    let nodes = parse(latex).ok()?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let ratex_color = Color::from_hex(&color.to_css_hex()).unwrap_or(Color::BLACK);
    let opts = LayoutOptions::default()
        .with_style(style)
        .with_color(ratex_color);
    let root = layout(&nodes, &opts);
    let list = to_display_list(&root);
    let em = if nodes.is_empty() {
        MathEm {
            width: 0.5,
            height: 0.7,
            depth: 0.25,
        }
    } else {
        MathEm {
            width: list.width as f32,
            height: list.height as f32,
            depth: list.depth as f32,
        }
    };
    Some(LaidOut { em, list })
}

#[allow(dead_code)]
pub(super) fn layout_em(latex: &str, display: bool, color: ThemeColor) -> Option<MathEm> {
    layout_formula(latex, display, color).map(|laid_out| laid_out.em)
}

pub(super) fn raster_dimensions(
    list: &DisplayList,
    spec: &super::RasterSpec,
) -> Option<(u32, u32)> {
    if !spec.font_size.is_finite() || spec.font_size <= 0.0 {
        return None;
    }
    let dpr = f64::from(spec.dpr.max(0.25));
    let em_px = f64::from(spec.font_size) * dpr;
    let pad_px = f64::from(MATH_PAD) * dpr;
    let width = (list.width * em_px + 2.0 * pad_px).ceil();
    let height = ((list.height + list.depth) * em_px + 2.0 * pad_px).ceil();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let width = width as u64;
    let height = height as u64;
    let pixels = width.checked_mul(height)?;
    let bytes = pixels.checked_mul(4)?;
    if pixels > MAX_RASTER_PIXELS || bytes > MAX_RASTER_BYTES {
        return None;
    }
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

pub fn raster(latex: &str, spec: &RasterSpec) -> Result<RasterOut, crate::Error> {
    let laid_out = layout_formula(latex, spec.display, spec.color)
        .ok_or_else(|| crate::Error::Math(md_i18n::Key::FormulaParseFailed.into()))?;
    if raster_dimensions(&laid_out.list, spec).is_none() {
        return Err(crate::Error::Math(md_i18n::Key::FormulaRenderFailed.into()));
    }
    let png_opts = RenderOptions {
        font_size: spec.font_size,
        padding: MATH_PAD,
        background_color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
        font_dir: String::new(),
        device_pixel_ratio: spec.dpr.max(0.25),
    };
    let image = match render_to_png(&laid_out.list, &png_opts) {
        Ok(bytes) if !bytes.is_empty() => decode_png(&bytes, spec.dpr.max(0.25)),
        _ => None,
    };
    Ok(RasterOut {
        em: laid_out.em,
        image,
    })
}

pub fn paint_ready(window: &mut Window, x: f32, y: f32, ready: &ReadyImage) {
    paint_at(
        window,
        x,
        y,
        &ready.image,
        ready.px_w,
        ready.px_h,
        ready.dpr,
    );
}
