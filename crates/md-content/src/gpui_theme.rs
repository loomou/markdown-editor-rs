use gpui::{Font, FontFeatures, Hsla};
use md_theme::{FontStyle, FontWeight, ThemeColor, TypeRole};

pub fn font_weight(weight: FontWeight) -> gpui::FontWeight {
    gpui::FontWeight(weight.0)
}

pub fn font_style(style: FontStyle) -> gpui::FontStyle {
    match style {
        FontStyle::Normal => gpui::FontStyle::Normal,
        FontStyle::Italic => gpui::FontStyle::Italic,
        FontStyle::Oblique => gpui::FontStyle::Oblique,
    }
}

pub trait ThemeColorExt {
    fn hsla(self) -> Hsla;
}

impl ThemeColorExt for ThemeColor {
    fn hsla(self) -> Hsla {
        Hsla {
            h: self.h,
            s: self.s,
            l: self.l,
            a: self.a,
        }
    }
}

pub trait TypeRoleExt {
    fn font(self) -> Font;
}

impl TypeRoleExt for TypeRole {
    fn font(self) -> Font {
        Font {
            family: self.family.into(),
            features: FontFeatures::default(),
            weight: font_weight(self.weight),
            style: font_style(self.style),
            fallbacks: None,
        }
    }
}
