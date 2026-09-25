mod items;
mod justify;
mod kp;
mod opportunities;
#[cfg(test)]
mod tests;

pub use items::{Clusters, ItemCtx, build_items};
pub use justify::glyph_shifts;
pub use kp::{Plan, PlannedLine, break_lines};
pub use opportunities::{Opp, OppCtx, has_complex_context, opportunities};

use unicode_linebreak::{BreakClass, break_property};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Box {
        width: f32,
    },
    Glue {
        width: f32,
        stretch: f32,
        shrink: f32,
    },
    Penalty {
        width: f32,
        cost: f32,
        flagged: bool,
    },
}

impl Item {
    pub fn width(self) -> f32 {
        match self {
            Item::Box { width } | Item::Glue { width, .. } | Item::Penalty { width, .. } => width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub item: Item,
    pub byte: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Ragged,
    Justify,
    Balanced,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub line_penalty: f32,
    pub flagged_demerits: f32,
    pub fitness_demerits: f32,
    pub ragged_stretch_em: f32,
    pub runt_demerits: f32,
    pub runt_max_chars: u8,
    pub emergency_penalty: f32,
    pub code_break_penalty: f32,
    pub hyphen_penalty: f32,
    pub cjk_stretch_em: f32,
    pub max_items: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            line_penalty: 10.0,
            flagged_demerits: 10000.0,
            fitness_demerits: 10000.0,
            ragged_stretch_em: 2.0,
            runt_demerits: 5000.0,
            runt_max_chars: 2,
            emergency_penalty: 1000.0,
            code_break_penalty: 100.0,
            hyphen_penalty: 50.0,
            cjk_stretch_em: 0.1,
            max_items: 20000,
        }
    }
}

pub(crate) fn break_class(c: char) -> BreakClass {
    break_property(c as u32)
}

pub(crate) fn is_cjk(c: char) -> bool {
    use BreakClass::{
        ConditionalJapaneseStarter, HangulLJamo, HangulLvSyllable, HangulLvtSyllable, HangulTJamo,
        HangulVJamo, Ideographic,
    };
    matches!(
        break_class(c),
        Ideographic
            | HangulLvSyllable
            | HangulLvtSyllable
            | HangulLJamo
            | HangulVJamo
            | HangulTJamo
            | ConditionalJapaneseStarter
    ) || matches!(
        c as u32,
        0x2014 | 0x2026 | 0x3000..=0x303F | 0xFE10..=0xFE1F | 0xFE30..=0xFE4F | 0xFF01..=0xFF65
    )
}

pub(crate) fn effective_width(width: f32) -> f64 {
    if width.is_nan() || width <= 0.0 {
        1.0
    } else {
        f64::from(width)
    }
}
