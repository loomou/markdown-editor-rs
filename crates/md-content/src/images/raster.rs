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

/// Where a raster is aimed: the css box it will paint into, the density it
/// carries, and the pixels it will allocate. Every raster path derives the
/// pixels here and only here, so a job can be sized before it runs without
/// repeating the fit arithmetic in the scheduler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RasterTarget {
    pub css: (f32, f32),
    pub density: f32,
    pub px_w: u32,
    pub px_h: u32,
}

impl RasterTarget {
    fn new(css: (f32, f32), density: f32, px: (u32, u32)) -> Self {
        RasterTarget {
            css,
            density,
            px_w: px.0,
            px_h: px.1,
        }
    }
}

/// Target for a css box at a density, capped at `MAX_RASTER_PIXELS`. The
/// density that is reported back is the one the capped pixels actually
/// deliver, so a downscaled raster still paints at its css size.
fn capped_target(css: (f32, f32), density: f32) -> RasterTarget {
    let density = density.max(0.25);
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
    RasterTarget::new((css_w, css_h), effective, (px_w, px_h))
}

/// Target for a css box at a density with no pixel ceiling. Only vector
/// sources take this path: resvg already refuses a pixmap it cannot allocate,
/// and a vector zoom has no encoded size to fall back to.
fn plain_target(css: (f32, f32), density: f32) -> RasterTarget {
    let density = density.max(0.25);
    let css_w = css.0.max(1.0);
    let css_h = css.1.max(1.0);
    let px_w = (css_w * density).ceil().max(1.0) as u32;
    let px_h = (css_h * density).ceil().max(1.0) as u32;
    RasterTarget::new((css_w, css_h), density, (px_w, px_h))
}

pub(crate) fn bitmap_display_target(width: u32, height: u32, key: &DisplayKey) -> RasterTarget {
    let (slot_w, slot_h) = key.raster_slot();
    let (fw, fh) = contain_fit(width as f32, height as f32, slot_w, slot_h);
    let dpr = key.raster_dpr();
    capped_target((snap_css(fw, dpr), snap_css(fh, dpr)), dpr)
}

pub(crate) fn bitmap_zoom_target(css: (f32, f32), density: f32) -> RasterTarget {
    capped_target(css, density)
}

/// A vector source fits in device pixels, so a diagram stays as sharp as the
/// pre-rasterised bitmap path was.
pub(crate) fn svg_display_target(tree: &usvg::Tree, key: &DisplayKey) -> RasterTarget {
    let sz = tree.size();
    let iw = sz.width().max(1.0);
    let ih = sz.height().max(1.0);
    let (slot_w, slot_h) = key.raster_slot();
    let dpr = key.raster_dpr();
    let (fit_w, fit_h) = contain_fit(iw * dpr, ih * dpr, slot_w * dpr, slot_h * dpr);
    let px_w = fit_w.ceil().max(1.0) as u32;
    let px_h = fit_h.ceil().max(1.0) as u32;
    RasterTarget::new((px_w as f32 / dpr, px_h as f32 / dpr), dpr, (px_w, px_h))
}

pub(crate) fn svg_zoom_target(css: (f32, f32), density: f32) -> RasterTarget {
    plain_target(css, density)
}

/// Working bytes the display job for this source will hold at its peak.
pub fn estimate_display_job(source: &SourceMedia, key: &DisplayKey) -> usize {
    match source {
        SourceMedia::Bitmap { width, height, .. } => {
            let target = bitmap_display_target(*width, *height, key);
            let (decoded_w, decoded_h) = source.decoded_dims();
            crate::media::estimate_bitmap_peak(decoded_w, decoded_h, target.px_w, target.px_h)
        }
        SourceMedia::Vector { tree, .. } => {
            let target = svg_display_target(tree, key);
            crate::media::estimate_raster_peak(target.px_w, target.px_h)
        }
    }
}

/// Working bytes the zoom job for this source will hold at its peak.
pub fn estimate_zoom_job(source: &SourceMedia, css: (f32, f32), density: f32) -> usize {
    match source {
        SourceMedia::Bitmap { .. } => {
            let target = bitmap_zoom_target(css, density);
            let (decoded_w, decoded_h) = source.decoded_dims();
            crate::media::estimate_bitmap_peak(decoded_w, decoded_h, target.px_w, target.px_h)
        }
        SourceMedia::Vector { .. } => {
            let target = svg_zoom_target(css, density);
            crate::media::estimate_raster_peak(target.px_w, target.px_h)
        }
    }
}

/// Build the raster a display slot paints. A bitmap source decodes here, since
/// the cache only keeps its encoded bytes.
pub fn raster_display(source: &SourceMedia, key: &DisplayKey) -> Result<ReadyImage, crate::Error> {
    match source {
        SourceMedia::Bitmap {
            bytes,
            format,
            width,
            height,
            ..
        } => {
            let target = bitmap_display_target(*width, *height, key);
            raster_bitmap_at(bytes, *format, target)
        }
        SourceMedia::Vector { tree, .. } => raster_svg_at(tree, svg_display_target(tree, key)),
    }
}

/// Build the raster the zoom overlay paints at an explicit css size and pixel
/// density.
pub fn raster_at(
    source: &SourceMedia,
    css: (f32, f32),
    density: f32,
) -> Result<ReadyImage, crate::Error> {
    match source {
        SourceMedia::Bitmap { bytes, format, .. } => {
            raster_bitmap_at(bytes, *format, bitmap_zoom_target(css, density))
        }
        SourceMedia::Vector { tree, .. } => raster_svg_at(tree, svg_zoom_target(css, density)),
    }
}

fn raster_bitmap_at(
    bytes: &[u8],
    format: ImageFormat,
    target: RasterTarget,
) -> Result<ReadyImage, crate::Error> {
    let img = decode_bitmap(bytes, format)?;
    let img = if img.dimensions() == (target.px_w, target.px_h) {
        img
    } else {
        imageops::resize(&img, target.px_w, target.px_h, FilterType::Triangle)
    };
    from_bgra(img, target.density)
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

/// Rasterise a vector source into the pixels a target asks for. A display
/// target fits the tree into its slot in device pixels; a zoom target is the
/// css box scaled by density, mirroring `mermaid::raster_svg`.
fn raster_svg_at(tree: &usvg::Tree, target: RasterTarget) -> Result<ReadyImage, crate::Error> {
    let img = render_tree_bgra(tree, target.px_w, target.px_h)?;
    from_bgra(img, target.density)
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
