pub const GPUI_MAX_EDGE: u32 = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterTarget {
    pub css: (f32, f32),
    pub density: f32,
    pub px_w: u32,
    pub px_h: u32,
}

pub fn target_for_css(
    css: (f32, f32),
    density: f32,
    max_pixels: u64,
    max_edge: u32,
) -> RasterTarget {
    let density = density.max(0.25);
    let css_w = css.0.max(1.0);
    let css_h = css.1.max(1.0);
    let want_w = (css_w * density).ceil().max(1.0) as u32;
    let want_h = (css_h * density).ceil().max(1.0) as u32;
    let edge = max_edge.max(1);
    let (px_w, px_h) = capped(want_w.min(edge), want_h.min(edge), max_pixels);
    let effective = if px_w == want_w && px_h == want_h {
        density
    } else {
        (px_w as f32 / css_w)
            .min(px_h as f32 / css_h)
            .max(f32::MIN_POSITIVE)
    };
    RasterTarget {
        css: (css_w, css_h),
        density: effective,
        px_w,
        px_h,
    }
}

fn capped(w: u32, h: u32, max_pixels: u64) -> (u32, u32) {
    let pixels = u64::from(w).saturating_mul(u64::from(h));
    if max_pixels == 0 || pixels <= max_pixels {
        return (w.max(1), h.max(1));
    }
    let scale = (max_pixels as f64 / pixels as f64).sqrt();
    (
        ((f64::from(w) * scale).floor() as u32).max(1),
        ((f64::from(h) * scale).floor() as u32).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::{GPUI_MAX_EDGE, target_for_css};

    const BUDGET: u64 = 8 * 1024 * 1024;

    #[test]
    fn a_target_at_one_to_one_keeps_the_requested_density() {
        let target = target_for_css((200.0, 100.0), 2.0, BUDGET, GPUI_MAX_EDGE);
        assert_eq!((target.px_w, target.px_h), (400, 200));
        assert_eq!(target.density, 2.0);
        assert_eq!(target.css, (200.0, 100.0));
    }

    #[test]
    fn a_target_over_the_pixel_budget_reports_the_density_it_can_deliver() {
        let target = target_for_css((10_000.0, 10_000.0), 4.0, BUDGET, GPUI_MAX_EDGE);
        let pixels = u64::from(target.px_w) * u64::from(target.px_h);
        assert!(pixels <= BUDGET, "pixels={pixels}");
        assert!(target.density < 4.0, "density={}", target.density);
        let (w, h) = (
            target.px_w as f32 / target.density,
            target.px_h as f32 / target.density,
        );
        assert!((w - 10_000.0).abs() < 1.0, "css width drifted to {w}");
        assert!((h - 10_000.0).abs() < 1.0, "css height drifted to {h}");
    }

    #[test]
    fn a_target_caps_a_long_edge_before_the_pixel_budget() {
        let target = target_for_css((40_000.0, 10.0), 1.0, u64::MAX, GPUI_MAX_EDGE);
        assert_eq!(target.px_w, GPUI_MAX_EDGE);
        assert_eq!(target.px_h, 10);
        assert!((target.density - GPUI_MAX_EDGE as f32 / 40_000.0).abs() < 1e-6);
    }

    #[test]
    fn a_target_never_goes_below_a_quarter_density() {
        let target = target_for_css((100.0, 100.0), 0.0, BUDGET, GPUI_MAX_EDGE);
        assert_eq!(target.density, 0.25);
        assert_eq!((target.px_w, target.px_h), (25, 25));
    }

    #[test]
    fn a_target_of_an_empty_box_still_allocates_one_pixel() {
        let target = target_for_css((0.0, 0.0), 1.0, BUDGET, GPUI_MAX_EDGE);
        assert_eq!((target.px_w, target.px_h), (1, 1));
    }
}
