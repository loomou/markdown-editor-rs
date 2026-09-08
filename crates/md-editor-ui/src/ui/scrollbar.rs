use md_core::Px;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    pub track_start: Px,
    pub track_len: Px,
    pub thumb_start: Px,
    pub thumb_len: Px,
    pub max_scroll: Px,
}

impl Slider {
    pub fn new(view: Px, content: Px, scroll: Px, pad: Px, min_thumb: Px) -> Option<Self> {
        if !view.is_finite()
            || !content.is_finite()
            || !scroll.is_finite()
            || !pad.is_finite()
            || !min_thumb.is_finite()
        {
            return None;
        }
        if view <= 0.0 || content <= view {
            return None;
        }
        let pad = pad.max(0.0);
        let min_thumb = min_thumb.max(0.0);
        let track_start = pad;
        let track_len = (view - pad * 2.0).max(0.0);
        if track_len <= 0.0 {
            return None;
        }
        let thumb_len = (view / content * track_len).clamp(min_thumb.min(track_len), track_len);
        let max_scroll = content - view;
        let t = (scroll.clamp(0.0, max_scroll) / max_scroll).clamp(0.0, 1.0);
        Some(Self {
            track_start,
            track_len,
            thumb_start: track_start + t * (track_len - thumb_len),
            thumb_len,
            max_scroll,
        })
    }
}

pub fn scroll_at(pointer: Px, grab: Px, track_start: Px, travel: Px, max_scroll: Px) -> Px {
    if travel <= 0.0 || max_scroll <= 0.0 {
        return 0.0;
    }
    let thumb_start = (pointer - grab).clamp(track_start, track_start + travel);
    ((thumb_start - track_start) / travel) * max_scroll
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gutter {
    pub start: Px,
    pub len: Px,
    pub thumb_start: Px,
    pub thumb_len: Px,
}

impl Gutter {
    pub fn new(view: Px, hit: Px, thumb: Px) -> Option<Self> {
        if !view.is_finite() || !hit.is_finite() || !thumb.is_finite() || view <= 0.0 {
            return None;
        }
        let len = hit.max(0.0).min(view);
        let start = view - len;
        let thumb_len = thumb.max(0.0).min(len);
        Some(Self {
            start,
            len,
            thumb_start: start + ((len - thumb_len) * 0.5).max(0.0),
            thumb_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Gutter, Slider, scroll_at};
    use md_core::Px;

    #[test]
    fn no_slider_when_the_content_fits() {
        assert_eq!(Slider::new(200.0, 200.0, 0.0, 4.0, 32.0), None);
        assert_eq!(Slider::new(200.0, 100.0, 0.0, 4.0, 32.0), None);
        assert_eq!(Slider::new(0.0, 800.0, 0.0, 4.0, 32.0), None);
    }

    #[test]
    fn no_slider_when_a_number_is_not_finite() {
        assert_eq!(Slider::new(f64::NAN, 800.0, 0.0, 4.0, 32.0), None);
        assert_eq!(Slider::new(200.0, f64::NAN, 0.0, 4.0, 32.0), None);
        assert_eq!(Slider::new(200.0, 800.0, f64::NAN, 4.0, 32.0), None);
        assert_eq!(Slider::new(200.0, 800.0, 0.0, f64::NAN, 32.0), None);
        assert_eq!(Slider::new(200.0, 800.0, 0.0, 4.0, f64::NAN), None);
        assert_eq!(Gutter::new(f64::NAN, 14.0, 6.0), None);
        assert_eq!(Gutter::new(200.0, f64::NAN, 6.0), None);
        assert_eq!(Gutter::new(200.0, 14.0, f64::NAN), None);
    }

    #[test]
    fn the_thumb_reaches_both_ends_of_the_track() {
        let top = Slider::new(200.0, 800.0, 0.0, 4.0, 32.0).expect("top");
        assert!((top.thumb_start - top.track_start).abs() < 1e-9);
        let bottom = Slider::new(200.0, 800.0, 600.0, 4.0, 32.0).expect("bottom");
        assert!(
            (bottom.thumb_start + bottom.thumb_len - (bottom.track_start + bottom.track_len)).abs()
                < 1e-9
        );
        let past = Slider::new(200.0, 800.0, 9_000.0, 4.0, 32.0).expect("past the bottom");
        assert_eq!(past.thumb_start, bottom.thumb_start);
    }

    #[test]
    fn a_short_track_shrinks_the_thumb_below_its_floor() {
        let s = Slider::new(10.0, 800.0, 0.0, 4.0, 32.0)
            .expect("a narrow viewport should still have a slider");
        assert!((s.track_len - 2.0).abs() < 1e-9);
        assert!((s.thumb_len - 2.0).abs() < 1e-9);
    }

    #[test]
    fn the_pointer_maps_back_to_the_scroll_it_came_from() {
        let s = Slider::new(200.0, 800.0, 150.0, 4.0, 32.0).expect("overflow");
        let back = |pointer: Px, grab: Px| {
            scroll_at(
                pointer,
                grab,
                s.track_start,
                s.track_len - s.thumb_len,
                s.max_scroll,
            )
        };
        let mid = back(s.thumb_start + s.thumb_len * 0.5, s.thumb_len * 0.5);
        assert!((mid - 150.0).abs() < 0.5, "mapped back {mid}, expected 150");
        assert!(back(s.track_start, 0.0).abs() < 1e-9);
        let end = back(s.track_start + s.track_len, s.thumb_len);
        assert!((end - s.max_scroll).abs() < 1e-9);
    }

    #[test]
    fn a_pointer_with_nowhere_to_go_maps_to_the_top() {
        assert_eq!(scroll_at(120.0, 0.0, 4.0, 0.0, 600.0), 0.0);
        assert_eq!(scroll_at(120.0, 0.0, 4.0, 160.0, 0.0), 0.0);
    }

    #[test]
    fn the_thumb_centres_in_its_gutter_and_hugs_the_far_edge() {
        let g = Gutter::new(800.0, 14.0, 6.0).expect("gutter");
        assert!((g.start - (800.0 - 14.0)).abs() < 1e-9);
        assert!((g.len - 14.0).abs() < 1e-9);
        assert!((g.thumb_len - 6.0).abs() < 1e-9);
        assert!((g.thumb_start - (800.0 - 14.0 + 4.0)).abs() < 1e-9);
        let tight = Gutter::new(4.0, 14.0, 6.0).expect("narrow");
        assert!((tight.start - 0.0).abs() < 1e-9);
        assert!((tight.len - 4.0).abs() < 1e-9);
        assert!((tight.thumb_len - 4.0).abs() < 1e-9);
    }
}
