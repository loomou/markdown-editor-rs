use super::quantize::snap_px;
use gpui::{Bounds, Corners, RenderImage, Window, point, px, size};
use std::sync::Arc;

fn blit_rect(x: f32, y: f32, px_w: u32, px_h: u32, dpr: f32) -> (f32, f32, f32, f32) {
    let dpr = dpr.max(0.25);
    let dw = px_w as f32 / dpr;
    let dh = px_h as f32 / dpr;
    (snap_px(x, dpr), snap_px(y, dpr), dw, dh)
}

pub fn paint_at(
    window: &mut Window,
    x: f32,
    y: f32,
    image: &Arc<RenderImage>,
    px_w: u32,
    px_h: u32,
    raster_dpr: f32,
) {
    if px_w == 0 || px_h == 0 {
        return;
    }
    let (ox, oy, dw, dh) = blit_rect(x, y, px_w, px_h, raster_dpr);
    let bounds = Bounds {
        origin: point(px(ox), px(oy)),
        size: size(px(dw), px(dh)),
    };
    let _ = window.paint_image(bounds, Corners::all(px(0.0)), image.clone(), 0, false);
}

#[cfg(test)]
mod tests {
    use super::blit_rect;

    #[test]
    fn blit_maps_bitmap_pixels_one_to_one() {
        for dpr in [1.0f32, 1.5, 2.0] {
            let (_x, _y, dw, dh) = blit_rect(0.0, 0.0, 60, 30, dpr);
            assert!(
                (dw * dpr - 60.0).abs() < 1e-4,
                "dpr={dpr}: the width does not round-trip"
            );
            assert!(
                (dh * dpr - 30.0).abs() < 1e-4,
                "dpr={dpr}: the height does not round-trip"
            );
        }
        let (_x, _y, dw, dh) = blit_rect(0.0, 0.0, 60, 30, 1.5);
        assert!((dw - 40.0).abs() < 1e-5);
        assert!((dh - 20.0).abs() < 1e-5);
    }

    #[test]
    fn blit_snaps_origin_but_not_size() {
        let (x, y, dw, dh) = blit_rect(10.3, 7.7, 60, 30, 2.0);
        assert!(
            ((x * 2.0) - (x * 2.0).round()).abs() < 1e-4,
            "x was not snapped"
        );
        assert!(
            ((y * 2.0) - (y * 2.0).round()).abs() < 1e-4,
            "y was not snapped"
        );
        assert!((dw - 30.0).abs() < 1e-5);
        assert!((dh - 15.0).abs() < 1e-5);
    }
}
