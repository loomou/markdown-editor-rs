use super::MATH_PAD;
use crate::pixels::snap_css;

#[derive(Clone, Copy, Debug)]
pub struct MathEm {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl MathEm {
    pub fn estimate(latex: &str) -> Self {
        let n = latex.chars().count().max(1) as f32;
        MathEm {
            width: (n * 0.45).max(0.5),
            height: 0.7,
            depth: 0.25,
        }
    }

    pub(crate) fn css_width(self, font_size: f32) -> f32 {
        (self.width * font_size + 2.0 * MATH_PAD).max(1.0)
    }

    pub(crate) fn css_ascent(self, font_size: f32) -> f32 {
        (self.height * font_size + MATH_PAD).max(0.0)
    }

    pub(crate) fn css_depth(self, font_size: f32) -> f32 {
        (self.depth * font_size + MATH_PAD).max(0.0)
    }

    pub(crate) fn css_height(self, font_size: f32) -> f32 {
        (self.css_ascent(font_size) + self.css_depth(font_size)).max(1.0)
    }

    pub fn box_width(self, font_size: f32, dpr: f32) -> f32 {
        snap_css(self.css_width(font_size), dpr)
    }

    pub fn box_height(self, font_size: f32, dpr: f32) -> f32 {
        snap_css(self.css_height(font_size), dpr)
    }
}
