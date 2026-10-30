use super::DisplayKey;
use super::MAX_RASTER_PIXELS;
use super::SourceMedia;
use super::fit::contain_fit;
use super::source::image_format;
use crate::pixels::{ReadyImage, capped_dims, from_bgra, paint_at, snap_css};
use gpui::{ImageFormat, Window};
use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use std::io::Cursor;

/// Build the raster a display slot paints. A bitmap source decodes here, since
/// the cache only keeps its encoded bytes.
pub fn raster_display(source: &SourceMedia, key: &DisplayKey) -> Result<ReadyImage, crate::Error> {
    match source {
        SourceMedia::Bitmap {
            bytes,
            format,
            width,
            height,
        } => {
            let (slot_w, slot_h) = key.raster_slot();
            let (fw, fh) = contain_fit(*width as f32, *height as f32, slot_w, slot_h);
            let dpr = key.raster_dpr();
            let css = (snap_css(fw, dpr), snap_css(fh, dpr));
            raster_bitmap_at(bytes, *format, css, dpr)
        }
        SourceMedia::Vector { tree, .. } => raster_display_svg(tree, key),
    }
}

/// Build the raster the zoom overlay paints at an explicit css size and pixel
/// density.
pub fn raster_at(
    source: &SourceMedia,
    css: (f32, f32),
    density: f32,
) -> Result<ReadyImage, crate::Error> {
    let density = density.max(0.25);
    match source {
        SourceMedia::Bitmap { bytes, format, .. } => raster_bitmap_at(bytes, *format, css, density),
        SourceMedia::Vector { tree, .. } => raster_svg_at(tree, css, density),
    }
}

fn raster_bitmap_at(
    bytes: &[u8],
    format: ImageFormat,
    css: (f32, f32),
    density: f32,
) -> Result<ReadyImage, crate::Error> {
    let css_w = css.0.max(1.0);
    let css_h = css.1.max(1.0);
    let want_w = (css_w * density).ceil().max(1.0) as u32;
    let want_h = (css_h * density).ceil().max(1.0) as u32;
    let (px_w, px_h) = capped_dims(want_w, want_h, MAX_RASTER_PIXELS);
    let effective = if px_w == want_w {
        density
    } else {
        (px_w as f32 / css_w).min(px_h as f32 / css_h)
    };
    let img = decode_bitmap(bytes, format)?;
    let img = if img.dimensions() == (px_w, px_h) {
        img
    } else {
        imageops::resize(&img, px_w, px_h, FilterType::Triangle)
    };
    from_bgra(img, effective)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageRasterFailed.into()))
}

/// Decode frame 0 of a raster source into BGRA. Only the first frame is ever
/// painted, so an animation pays for its first frame rather than for every
/// frame it declares. EXIF orientation is applied here for the same reason
/// gpui applied it at load: a rotated phone photo must not come back sideways
/// now that decoding moved into this path. The caller has already bounded the
/// target, so the decoded buffer is never resampled twice.
fn decode_bitmap(bytes: &[u8], format: ImageFormat) -> Result<RgbaImage, crate::Error> {
    let format = image_format(format)
        .ok_or_else(|| crate::Error::Image(md_i18n::Key::ImageBadPath.into()))?;
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), format)
        .into_decoder()
        .map_err(|e| crate::Error::Image(e.to_string().into()))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| crate::Error::Image(e.to_string().into()))?;
    let mut decoded = DynamicImage::from_decoder(decoder)
        .map_err(|e| crate::Error::Image(e.to_string().into()))?;
    decoded.apply_orientation(orientation);
    let mut img = decoded.into_rgba8();
    for pixel in img.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(img)
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
fn raster_display_svg(tree: &usvg::Tree, key: &DisplayKey) -> Result<ReadyImage, crate::Error> {
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
fn raster_svg_at(
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
