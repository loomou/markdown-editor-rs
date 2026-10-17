use super::DisplayKey;
use super::fit::contain_fit;
use crate::pixels::{ReadyImage, from_bgra, paint_at, snap_css};
use gpui::{RenderImage, Window};
use image::RgbaImage;
use image::imageops::{self, FilterType};
use std::sync::Arc;

pub fn raster_display(
    source: &Arc<RenderImage>,
    key: &DisplayKey,
) -> Result<ReadyImage, crate::Error> {
    if source.frame_count() == 0 {
        return Err(crate::Error::Image(md_i18n::Key::ImageEmpty.into()));
    }
    let sz = source.size(0);
    let iw = sz.width.0.max(0) as u32;
    let ih = sz.height.0.max(0) as u32;
    if iw == 0 || ih == 0 {
        return Err(crate::Error::Image(md_i18n::Key::ImageEmpty.into()));
    }
    let (slot_w, slot_h) = key.raster_slot();
    let (fw, fh) = contain_fit(iw as f32, ih as f32, slot_w, slot_h);
    let dpr = key.raster_dpr();
    let px_w = (snap_css(fw, dpr) * dpr).ceil().max(1.0) as u32;
    let px_h = (snap_css(fh, dpr) * dpr).ceil().max(1.0) as u32;
    if px_w == iw && px_h == ih {
        return Ok(ReadyImage {
            image: Arc::clone(source),
            bytes: 0,
            dpr,
            px_w,
            px_h,
            shared_with_source: true,
        });
    }
    let raw = source
        .as_bytes(0)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageNoPixels.into()))?
        .to_vec();
    let img = RgbaImage::from_raw(iw, ih, raw)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageBadPixels.into()))?;
    let out = imageops::resize(&img, px_w, px_h, FilterType::Triangle);
    from_bgra(out, dpr).ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageRasterFailed.into()))
}

pub fn paint_ready(
    window: &mut Window,
    x: f32,
    y: f32,
    slot_w: f32,
    slot_h: f32,
    ready: &ReadyImage,
) {
    let (image_w, image_h) = ready.css_size();
    let dx = ((slot_w - image_w).max(0.0)) * 0.5;
    let dy = ((slot_h - image_h).max(0.0)) * 0.5;
    paint_at(
        window,
        x + dx,
        y + dy,
        &ready.image,
        ready.px_w,
        ready.px_h,
        ready.dpr,
    );
}

/// Rasterise an SVG source for a display slot. The fit runs in device pixels
/// so a vector source stays as sharp as the pre-rasterised bitmap path was.
pub fn raster_display_svg(tree: &usvg::Tree, key: &DisplayKey) -> Result<ReadyImage, crate::Error> {
    let sz = tree.size();
    let iw = sz.width().max(1.0);
    let ih = sz.height().max(1.0);
    let (slot_w, slot_h) = key.raster_slot();
    let dpr = key.raster_dpr();
    let (fit_w, fit_h) = contain_fit(iw * dpr, ih * dpr, slot_w * dpr, slot_h * dpr);
    let px_w = fit_w.ceil().max(1.0) as u32;
    let px_h = fit_h.ceil().max(1.0) as u32;
    let img = render_tree_bgra(tree, px_w, px_h)?;
    from_bgra(img, dpr).ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageRasterFailed.into()))
}

/// Rasterise an SVG source at an explicit css size and pixel density for the
/// zoom view, mirroring `mermaid::raster_svg`.
pub fn raster_svg_at(
    tree: &usvg::Tree,
    css: (f32, f32),
    density: f32,
) -> Result<ReadyImage, crate::Error> {
    let density = density.max(0.25);
    let px_w = (css.0.max(1.0) * density).ceil() as u32;
    let px_h = (css.1.max(1.0) * density).ceil() as u32;
    let img = render_tree_bgra(tree, px_w, px_h)?;
    from_bgra(img, density)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageRasterFailed.into()))
}

fn render_tree_bgra(tree: &usvg::Tree, px_w: u32, px_h: u32) -> Result<RgbaImage, crate::Error> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px_w, px_h)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageRasterFailed.into()))?;
    let sz = tree.size();
    let sx = px_w as f32 / sz.width().max(1.0);
    let sy = px_h as f32 / sz.height().max(1.0);
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    let mut out = RgbaImage::new(px_w, px_h);
    for (dst, src) in out
        .as_mut()
        .chunks_exact_mut(4)
        .zip(pixmap.data().chunks_exact(4))
    {
        let Some(pixel) =
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(src[0], src[1], src[2], src[3])
        else {
            continue;
        };
        let color = pixel.demultiply();
        dst[0] = color.blue();
        dst[1] = color.green();
        dst[2] = color.red();
        dst[3] = color.alpha();
    }
    Ok(out)
}

pub(super) fn native_dims(img: Arc<RenderImage>) -> Option<(Arc<RenderImage>, u32, u32)> {
    if img.frame_count() == 0 {
        return None;
    }
    let sz = img.size(0);
    let width = sz.width.0.max(0) as u32;
    let height = sz.height.0.max(0) as u32;
    if width == 0 || height == 0 {
        None
    } else {
        Some((img, width, height))
    }
}
