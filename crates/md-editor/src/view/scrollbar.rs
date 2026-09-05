use super::{ScrollbarGeom, WellBar, WellHit, WellScroll};
use crate::ui::scrollbar::{Gutter, Slider, scroll_at};
use md_core::Px;
use md_theme::ChromeTokens;

impl ScrollbarGeom {
    pub(super) fn layout(
        view_w: Px,
        view_h: Px,
        scroll: Px,
        total: Px,
        chrome: &ChromeTokens,
    ) -> Option<Self> {
        let s = Slider::new(
            view_h,
            total,
            scroll,
            chrome.scrollbar_pad,
            chrome.scrollbar_min_thumb,
        )?;
        let g = Gutter::new(view_w, chrome.scrollbar_hit, chrome.scrollbar_thumb_w)?;
        Some(ScrollbarGeom {
            hit_x: g.start,
            hit_w: g.len,
            track_y: s.track_start,
            track_h: s.track_len,
            thumb_x: g.thumb_start,
            thumb_y: s.thumb_start,
            thumb_w: g.thumb_len,
            thumb_h: s.thumb_len,
            max_scroll: s.max_scroll,
        })
    }

    pub(super) fn contains(self, x: Px, y: Px) -> bool {
        x >= self.hit_x
            && x < self.hit_x + self.hit_w
            && y >= self.track_y
            && y <= self.track_y + self.track_h
    }

    pub(super) fn thumb_contains(self, x: Px, y: Px) -> bool {
        x >= self.hit_x
            && x < self.hit_x + self.hit_w
            && y >= self.thumb_y
            && y < self.thumb_y + self.thumb_h
    }

    pub(super) fn scroll_for_pointer(self, y: Px, grab: Px) -> Px {
        scroll_at(
            y,
            grab,
            self.track_y,
            (self.track_h - self.thumb_h).max(0.0),
            self.max_scroll,
        )
    }
}

impl WellScroll {
    pub(super) fn along_axis(self, vertical: bool) -> Px {
        if vertical { self.y } else { self.x }
    }

    pub(super) fn with_axis(self, vertical: bool, pos: Px) -> Self {
        if vertical {
            Self { x: self.x, y: pos }
        } else {
            Self { x: pos, y: self.y }
        }
    }
}

impl WellBar {
    pub(super) fn vertical(
        view_w: Px,
        view_h: Px,
        scroll: Px,
        content_h: Px,
        chrome: &ChromeTokens,
    ) -> Option<Self> {
        let g = ScrollbarGeom::layout(view_w, view_h, scroll, content_h, chrome)?;
        Some(Self {
            vertical: true,
            hit_x: g.hit_x,
            hit_y: g.track_y,
            hit_w: g.hit_w,
            hit_h: g.track_h,
            thumb_x: g.thumb_x,
            thumb_y: g.thumb_y,
            thumb_w: g.thumb_w,
            thumb_h: g.thumb_h,
            max_scroll: g.max_scroll,
        })
    }

    pub(super) fn horizontal(
        view_w: Px,
        view_h: Px,
        scroll: Px,
        content_w: Px,
        chrome: &ChromeTokens,
    ) -> Option<Self> {
        let s = Slider::new(
            view_w,
            content_w,
            scroll,
            chrome.scrollbar_pad,
            chrome.scrollbar_min_thumb,
        )?;
        let g = Gutter::new(view_h, chrome.scrollbar_hit, chrome.scrollbar_thumb_w)?;
        Some(Self {
            vertical: false,
            hit_x: s.track_start,
            hit_y: g.start,
            hit_w: s.track_len,
            hit_h: g.len,
            thumb_x: s.thumb_start,
            thumb_y: g.thumb_start,
            thumb_w: s.thumb_len,
            thumb_h: g.thumb_len,
            max_scroll: s.max_scroll,
        })
    }

    pub(super) fn contains(self, x: Px, y: Px) -> bool {
        x >= self.hit_x
            && x < self.hit_x + self.hit_w
            && y >= self.hit_y
            && y < self.hit_y + self.hit_h
    }

    pub(super) fn thumb_contains(self, x: Px, y: Px) -> bool {
        x >= self.thumb_x
            && x < self.thumb_x + self.thumb_w
            && y >= self.thumb_y
            && y < self.thumb_y + self.thumb_h
    }

    pub(super) fn on_axis(
        vertical: bool,
        hit: &WellHit,
        s: WellScroll,
        chrome: &ChromeTokens,
    ) -> Option<Self> {
        if vertical {
            Self::vertical(hit.view_w, hit.view_h, s.y, hit.content_h, chrome)
        } else {
            Self::horizontal(hit.view_w, hit.view_h, s.x, hit.content_w, chrome)
        }
    }

    pub(super) fn along(self, x: Px, y: Px) -> Px {
        if self.vertical { y } else { x }
    }

    pub(super) fn thumb_start(self) -> Px {
        if self.vertical {
            self.thumb_y
        } else {
            self.thumb_x
        }
    }

    pub(super) fn thumb_len(self) -> Px {
        if self.vertical {
            self.thumb_h
        } else {
            self.thumb_w
        }
    }

    pub(super) fn scroll_for_pointer(self, pos: Px, grab: Px) -> Px {
        let (track_start, track_len, thumb_len) = if self.vertical {
            (self.hit_y, self.hit_h, self.thumb_h)
        } else {
            (self.hit_x, self.hit_w, self.thumb_w)
        };
        scroll_at(
            pos,
            grab,
            track_start,
            (track_len - thumb_len).max(0.0),
            self.max_scroll,
        )
    }
}

pub(super) fn select_autoscroll_delta(y: Px, view_h: Px, chrome: &ChromeTokens) -> Px {
    let edge = chrome.select_autoscroll_edge;
    if view_h <= 0.0 || edge <= 0.0 {
        return 0.0;
    }
    let outside = chrome.select_autoscroll_outside.max(0.0);
    let edge_px = chrome.select_autoscroll_edge_px.max(0.0);
    let max_px = chrome.select_autoscroll_max_px.max(edge_px);
    let band = |t: Px| {
        let t = t.clamp(0.0, 1.0);
        edge_px * t * t
    };
    let past = |d: Px| {
        let t = if outside > 0.0 {
            (d / outside).clamp(0.0, 1.0)
        } else {
            1.0
        };
        edge_px + (max_px - edge_px) * t
    };
    if y < 0.0 {
        -past(-y)
    } else if y < edge {
        -band((edge - y) / edge)
    } else if y > view_h {
        past(y - view_h)
    } else if y > view_h - edge {
        band((y - (view_h - edge)) / edge)
    } else {
        0.0
    }
}

pub(super) fn select_autoscroll_can_move(delta: Px, scroll: Px, view_h: Px, total: Px) -> bool {
    if delta == 0.0 {
        return false;
    }
    let max = (total - view_h).max(0.0);
    if max <= 0.0 {
        return false;
    }
    (delta < 0.0 && scroll > 0.0) || (delta > 0.0 && scroll < max)
}

const PARK_TOP_LESS_THAN_PARA: Px = 4.0;

pub(super) fn park_block_top_margin(paragraph_gap: Px) -> Px {
    if !paragraph_gap.is_finite() {
        return 0.0;
    }
    (paragraph_gap - PARK_TOP_LESS_THAN_PARA).max(0.0)
}

pub(super) fn park_block_top_scroll(
    y: Px,
    scroll: Px,
    view_h: Px,
    total: Px,
    margin: Px,
) -> Option<Px> {
    if !y.is_finite()
        || !scroll.is_finite()
        || !view_h.is_finite()
        || !total.is_finite()
        || !margin.is_finite()
        || view_h <= 0.0
    {
        return None;
    }
    let margin = margin.max(0.0).min(view_h * 0.35);
    let next = (y - margin).clamp(0.0, (total - view_h).max(0.0));
    if (next - scroll).abs() < 0.5 {
        None
    } else {
        Some(next)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn search_reveal_scroll(
    y0: Px,
    y1: Px,
    scroll: Px,
    view_h: Px,
    total: Px,
    line_h: Px,
    dir: i32,
    chrome: &ChromeTokens,
) -> Option<Px> {
    if !y0.is_finite()
        || !y1.is_finite()
        || !scroll.is_finite()
        || !view_h.is_finite()
        || !total.is_finite()
        || !line_h.is_finite()
        || !chrome.search_comfort_lines.is_finite()
        || !chrome.search_comfort_vh.is_finite()
        || !chrome.search_park_next.is_finite()
        || !chrome.search_park_prev.is_finite()
        || view_h <= 0.0
    {
        return None;
    }
    let margin = (chrome.search_comfort_lines * line_h.max(0.0))
        .max(chrome.search_comfort_vh * view_h)
        .min(view_h * 0.45);
    let inner_top = scroll + margin;
    let inner_bot = scroll + view_h - margin;
    if y0 >= inner_top && y1 <= inner_bot {
        return None;
    }
    let raw = if dir < 0 {
        y1 - chrome.search_park_prev * view_h
    } else {
        y0 - chrome.search_park_next * view_h
    };
    let next = raw.clamp(0.0, (total - view_h).max(0.0));
    if (next - scroll).abs() < 0.5 {
        None
    } else {
        Some(next)
    }
}
