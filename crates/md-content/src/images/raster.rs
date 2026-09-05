use super::DisplayKey;
use super::fit::contain_fit;
use crate::pixels::{ReadyImage, from_bgra, paint_at, snap_css};
use gpui::{RenderImage, Window};
use image::imageops::FilterType;
use image::{RgbaImage, imageops};
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

pub(super) fn decoded_len(img: &RenderImage) -> usize {
    (0..img.frame_count()).fold(0, |total, frame| {
        total.saturating_add(img.as_bytes(frame).map_or(0, |bytes| bytes.len()))
    })
}
