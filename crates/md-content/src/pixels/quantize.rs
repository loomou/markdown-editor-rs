pub const WIDTH_QUANT: f64 = 32.0;

pub fn dpr_q(dpr: f64) -> u16 {
    ((dpr * 4.0).round() as i32).clamp(4, 16) as u16
}

pub fn dpr_from_q(dpr_q: u16) -> f32 {
    f32::from(dpr_q) / 4.0
}

pub fn snap_css(css: f32, dpr: f32) -> f32 {
    let dpr = dpr.max(0.25);
    (css * dpr).ceil().max(1.0) / dpr
}

pub fn snap_px(v: f32, dpr: f32) -> f32 {
    (v * dpr).round() / dpr
}

#[cfg(test)]
mod tests {
    use super::{dpr_from_q, dpr_q, snap_css};

    #[test]
    fn dpr_quantises_to_quarter_steps_within_range() {
        assert_eq!(dpr_q(1.0), 4);
        assert_eq!(dpr_q(1.5), 6);
        assert_eq!(dpr_q(2.0), 8);
        assert_eq!(dpr_q(0.1), 4);
        assert_eq!(dpr_q(9.0), 16);
        for q in [4u16, 6, 8, 16] {
            assert_eq!(
                dpr_q(f64::from(dpr_from_q(q))),
                q,
                "q={q} does not round-trip"
            );
        }
    }

    #[test]
    fn snap_css_lands_on_whole_device_pixels() {
        for dpr in [1.0f32, 1.5, 2.0] {
            for css in [0.1f32, 3.3, 21.744, 100.0] {
                let snapped = snap_css(css, dpr);
                let device = snapped * dpr;
                assert!(
                    (device - device.round()).abs() < 1e-3,
                    "css={css} dpr={dpr}: the snapped {device} is not a whole pixel"
                );
                assert!(
                    snapped * dpr >= 1.0,
                    "css={css} dpr={dpr}: snapped down to zero"
                );
            }
        }
    }

    #[test]
    fn snap_css_rounds_up_not_nearest() {
        let css = 21.744;
        let dpr = 1.5;
        assert_eq!(
            (snap_css(css, dpr) * dpr).round() as u32,
            (css * dpr).ceil() as u32
        );
    }
}
