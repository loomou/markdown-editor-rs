use gpui::RenderImage;
use image::RgbaImage;
use smallvec::smallvec;
use std::sync::Arc;

#[derive(Clone)]
pub struct ReadyImage {
    pub image: Arc<RenderImage>,
    pub bytes: usize,

    pub dpr: f32,
    pub px_w: u32,
    pub px_h: u32,

    pub shared_with_source: bool,
}

impl ReadyImage {
    pub fn css_size(&self) -> (f32, f32) {
        let dpr = self.dpr.max(f32::MIN_POSITIVE);
        (
            (self.px_w as f32 / dpr).max(1.0),
            (self.px_h as f32 / dpr).max(1.0),
        )
    }
}

pub fn from_bgra(img: RgbaImage, dpr: f32) -> Option<ReadyImage> {
    let (px_w, px_h) = img.dimensions();
    if px_w == 0 || px_h == 0 {
        return None;
    }
    let bytes = (px_w as usize)
        .saturating_mul(px_h as usize)
        .saturating_mul(4);
    let frame = image::Frame::new(img);
    Some(ReadyImage {
        image: Arc::new(RenderImage::new(smallvec![frame])),
        bytes,
        dpr,
        px_w,
        px_h,
        shared_with_source: false,
    })
}

pub fn decode_png(png: &[u8], dpr: f32) -> Option<ReadyImage> {
    let mut img = image::load_from_memory(png).ok()?.into_rgba8();
    for pixel in img.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    from_bgra(img, dpr)
}

#[cfg(test)]
mod tests {
    use super::{decode_png, from_bgra};
    use image::RgbaImage;

    fn red_png(w: u32, h: u32) -> Vec<u8> {
        use std::io::Cursor;
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 0, 0]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .expect("png");
        buf
    }

    #[test]
    fn decode_png_swaps_red_and_blue() {
        let ready = decode_png(&red_png(2, 2), 1.0).expect("decode");
        assert_eq!((ready.px_w, ready.px_h), (2, 2));
        let bytes = ready.image.as_bytes(0).expect("pixels");
        assert_eq!(&bytes[..4], &[0, 0, 255, 255], "expected pure red in BGRA");
    }

    #[test]
    fn empty_bitmap_is_not_ready() {
        assert!(from_bgra(RgbaImage::new(0, 4), 1.0).is_none());
        assert!(from_bgra(RgbaImage::new(4, 0), 1.0).is_none());
        assert!(decode_png(b"not a png", 1.0).is_none());
    }

    #[test]
    fn css_size_divides_by_the_raster_dpr() {
        let ready = decode_png(&red_png(60, 30), 1.5).expect("decode");
        let (w, h) = ready.css_size();
        assert!((w - 40.0).abs() < 1e-4);
        assert!((h - 20.0).abs() < 1e-4);
    }

    #[test]
    fn css_size_keeps_a_budget_reduced_dpr() {
        let ready = decode_png(&red_png(2, 1), 0.125).expect("decode");
        assert_eq!(ready.css_size(), (16.0, 8.0));
    }
}
