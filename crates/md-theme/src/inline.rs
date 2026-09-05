use super::font::{FontStyle, FontWeight};
use super::paint::{Palette, ThemeColor};
use md_core::Px;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineTokens {
    pub link: ThemeColor,
    pub inline_code: ThemeColor,
    pub inline_code_fill: ThemeColor,

    pub inline_code_radius: f32,
    pub inline_code_pad_x: Px,
    pub inline_code_pad_y: Px,
    pub syntax_marker: ThemeColor,
    pub image_fill: ThemeColor,
    pub image_border: ThemeColor,
    pub strikethrough: ThemeColor,
    pub task_strike: ThemeColor,
    pub strong_weight: FontWeight,
    pub emphasis_style: FontStyle,
    pub link_underline_px: f32,
    pub link_underline: ThemeColor,
    pub strikethrough_px: f32,
}

impl InlineTokens {
    pub fn fingerprint(self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut h);
        h.finish()
    }

    pub(crate) fn without_colors(self) -> Self {
        const FLAT: ThemeColor = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
        let Self {
            link: _,
            inline_code: _,
            inline_code_fill: _,
            inline_code_radius,
            inline_code_pad_x,
            inline_code_pad_y,
            syntax_marker: _,
            image_fill: _,
            image_border: _,
            strikethrough: _,
            task_strike: _,
            strong_weight,
            emphasis_style,
            link_underline_px,
            link_underline: _,
            strikethrough_px,
        } = self;
        Self {
            link: FLAT,
            inline_code: FLAT,
            inline_code_fill: FLAT,
            inline_code_radius,
            inline_code_pad_x,
            inline_code_pad_y,
            syntax_marker: FLAT,
            image_fill: FLAT,
            image_border: FLAT,
            strikethrough: FLAT,
            task_strike: FLAT,
            strong_weight,
            emphasis_style,
            link_underline_px,
            link_underline: FLAT,
            strikethrough_px,
        }
    }

    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            link: p.link,
            inline_code: p.inline_code,
            inline_code_fill: p.inline_code_fill,
            inline_code_radius: 4.0,
            inline_code_pad_x: 3.0,
            inline_code_pad_y: 1.0,
            syntax_marker: p.slate_500,
            image_fill: p.slate_800,
            image_border: p.slate_300,
            strikethrough: p.slate_600,
            task_strike: ThemeColor {
                a: 0.5,
                ..p.slate_600
            },
            strong_weight: FontWeight::BOLD,
            emphasis_style: FontStyle::Italic,
            link_underline_px: 1.0,
            link_underline: ThemeColor { a: 0.4, ..p.link },
            strikethrough_px: 1.0,
        }
    }
}
