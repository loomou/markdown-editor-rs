use super::DocumentTheme;
use super::color::ColorOverrides;
use super::font::{SYSTEM_SERIF, SYSTEM_UI};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeVariant {
    #[default]
    OneDark,
    OneLight,
}

impl ThemeVariant {
    pub const ALL: [ThemeVariant; 2] = [Self::OneDark, Self::OneLight];

    pub const COUNT: usize = Self::ALL.len();

    pub fn key(self) -> &'static str {
        match self {
            Self::OneDark => "one-dark",
            Self::OneLight => "one-light",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "one-dark" => Some(Self::OneDark),
            "one-light" => Some(Self::OneLight),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn document_theme(self) -> DocumentTheme {
        match self {
            Self::OneDark => DocumentTheme::one_dark(),
            Self::OneLight => DocumentTheme::one_light(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BodyFamily {
    #[default]
    Sans,
    Serif,
}

impl BodyFamily {
    pub fn key(self) -> &'static str {
        match self {
            Self::Sans => "sans",
            Self::Serif => "serif",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "sans" => Some(Self::Sans),
            "serif" => Some(Self::Serif),
            _ => None,
        }
    }

    pub fn family(self) -> &'static str {
        match self {
            Self::Sans => SYSTEM_UI,
            Self::Serif => SYSTEM_SERIF,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Relaxed,
}

impl Density {
    pub const ALL: [Density; 3] = [Density::Compact, Density::Normal, Density::Relaxed];

    pub fn key(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
            Self::Relaxed => "relaxed",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "compact" => Some(Self::Compact),
            "normal" => Some(Self::Normal),
            "relaxed" => Some(Self::Relaxed),
            _ => None,
        }
    }

    pub fn edge_scale(self) -> f64 {
        match self {
            Self::Compact => 0.8,
            Self::Normal => 1.0,
            Self::Relaxed => 1.25,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub variant: ThemeVariant,
    pub body_family: BodyFamily,

    pub body_size_px: f32,
    pub density: Density,

    pub colors: [ColorOverrides; ThemeVariant::COUNT],
}

impl Appearance {
    pub const BASE_BODY_SIZE_PX: f32 = 16.0;
    pub const MIN_BODY_SIZE_PX: f32 = 12.0;
    pub const MAX_BODY_SIZE_PX: f32 = 24.0;

    pub fn colors(&self) -> &ColorOverrides {
        &self.colors[self.variant.index()]
    }

    pub fn colors_mut(&mut self) -> &mut ColorOverrides {
        &mut self.colors[self.variant.index()]
    }

    pub fn with_body_size_px(mut self, size: f32) -> Self {
        self.body_size_px = size
            .round()
            .clamp(Self::MIN_BODY_SIZE_PX, Self::MAX_BODY_SIZE_PX);
        self
    }

    pub fn document_theme(&self) -> DocumentTheme {
        let mut theme = self.variant.document_theme();
        theme.edge_scale = self.density.edge_scale();

        let k = self.body_size_px / Self::BASE_BODY_SIZE_PX;
        let family = self.body_family.family();
        for role in theme.type_scale.roles_mut() {
            if k != 1.0 {
                role.size_px *= k;
                role.letter_spacing_px *= k;
            }
            if role.family == SYSTEM_UI {
                role.family = family;
            }
        }
        self.colors().apply_to(&mut theme);
        theme
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            variant: ThemeVariant::default(),
            body_family: BodyFamily::default(),
            body_size_px: Self::BASE_BODY_SIZE_PX,
            density: Density::default(),
            colors: [ColorOverrides::default(); ThemeVariant::COUNT],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Appearance, BodyFamily, Density, ThemeVariant};
    use crate::{ColorSlot, DocumentTheme, SYSTEM_MONO, SYSTEM_SERIF, SYSTEM_UI, ThemeColor};

    #[test]
    fn the_default_appearance_is_exactly_one_dark() {
        assert_eq!(
            Appearance::default().document_theme(),
            DocumentTheme::one_dark()
        );
        assert_eq!(
            Appearance {
                variant: ThemeVariant::OneLight,
                ..Appearance::default()
            }
            .document_theme(),
            DocumentTheme::one_light()
        );
    }

    #[test]
    fn body_size_scales_every_role_and_keeps_the_ratios() {
        let base = DocumentTheme::one_dark().type_scale;
        let theme = Appearance::default()
            .with_body_size_px(24.0)
            .document_theme();
        assert_eq!(theme.type_scale.body.size_px, 24.0);

        assert!(theme.type_scale.heading[3].size_px >= theme.type_scale.body.size_px);
        assert_eq!(theme.type_scale.code.size_px, base.code.size_px * 1.5);
        assert_eq!(
            theme.type_scale.heading[0].letter_spacing_px,
            base.heading[0].letter_spacing_px * 1.5
        );

        assert_eq!(
            theme.type_scale.body.line_height_em,
            base.body.line_height_em
        );
    }

    #[test]
    fn body_size_is_rounded_and_clamped() {
        assert_eq!(
            Appearance::default().with_body_size_px(2.0).body_size_px,
            12.0
        );
        assert_eq!(
            Appearance::default().with_body_size_px(99.0).body_size_px,
            24.0
        );
        assert_eq!(
            Appearance::default().with_body_size_px(15.4).body_size_px,
            15.0
        );
    }

    #[test]
    fn the_serif_switch_leaves_monospace_alone() {
        let theme = Appearance {
            body_family: BodyFamily::Serif,
            ..Appearance::default()
        }
        .document_theme();
        assert_eq!(theme.type_scale.body.family, SYSTEM_SERIF);
        assert_eq!(theme.type_scale.heading[0].family, SYSTEM_SERIF);
        assert_eq!(theme.type_scale.quote.family, SYSTEM_SERIF);
        assert_eq!(theme.type_scale.code.family, SYSTEM_MONO);
        assert_eq!(
            Appearance::default()
                .document_theme()
                .type_scale
                .body
                .family,
            SYSTEM_UI
        );
    }

    #[test]
    fn density_moves_edge_scale_and_the_spacing_with_it() {
        let compact = Appearance {
            density: Density::Compact,
            ..Appearance::default()
        }
        .document_theme();
        let base = DocumentTheme::one_dark();
        assert_eq!(compact.edge_scale, 0.8);
        assert_eq!(
            compact
                .box_style(md_core::block::BlockKind::Paragraph)
                .margin
                .top,
            base.boxes.paragraph.margin.top * 0.8
        );
        assert!(
            Appearance {
                density: Density::Relaxed,
                ..Appearance::default()
            }
            .document_theme()
            .edge_scale
                > 1.0
        );
    }

    #[test]
    fn every_key_round_trips() {
        for v in [ThemeVariant::OneDark, ThemeVariant::OneLight] {
            assert_eq!(ThemeVariant::from_key(v.key()), Some(v));
        }
        for f in [BodyFamily::Sans, BodyFamily::Serif] {
            assert_eq!(BodyFamily::from_key(f.key()), Some(f));
        }
        for d in Density::ALL {
            assert_eq!(Density::from_key(d.key()), Some(d));
        }
        assert_eq!(ThemeVariant::from_key("gruvbox"), None);
        assert_eq!(Density::from_key(""), None);
        for (i, v) in ThemeVariant::ALL.iter().enumerate() {
            assert_eq!(v.index(), i);
        }
    }

    #[test]
    fn an_override_changes_the_color_and_nothing_else() {
        let plain = Appearance::default();
        let mut tinted = plain;
        let pink = ThemeColor::new(0.9, 0.8, 0.6, 1.0);
        tinted.colors_mut().set(ColorSlot::Body, pink);
        tinted.colors_mut().set(ColorSlot::SynKeyword, pink);
        tinted.colors_mut().set(ColorSlot::Canvas, pink);

        let a = plain.document_theme();
        let b = tinted.document_theme();
        assert_ne!(a, b, "the override did not take effect");
        assert!(
            a.layout_metrics_eq(&b),
            "recoloring was judged a geometry change: one color edit would then relayout the whole document"
        );
        assert_eq!(ColorSlot::Body.read(&b), pink);
        assert_eq!(ColorSlot::SynKeyword.read(&b), pink);
        assert_eq!(ColorSlot::Canvas.read(&b), pink);
        assert_eq!(ColorSlot::Caret.read(&b), ColorSlot::Caret.read(&a));
    }

    #[test]
    fn overrides_belong_to_the_variant_they_were_made_in() {
        let mut app = Appearance::default();
        assert_eq!(app.variant, ThemeVariant::OneDark);
        let pink = ThemeColor::new(0.9, 0.8, 0.6, 1.0);
        app.colors_mut().set(ColorSlot::Canvas, pink);
        assert_eq!(ColorSlot::Canvas.read(&app.document_theme()), pink);

        app.variant = ThemeVariant::OneLight;
        assert!(
            app.colors().is_empty(),
            "edits made in the dark variant must not leak into the light one"
        );
        assert_eq!(
            app.document_theme(),
            DocumentTheme::one_light(),
            "switching over should give the stock light theme"
        );

        app.variant = ThemeVariant::OneDark;
        assert_eq!(
            ColorSlot::Canvas.read(&app.document_theme()),
            pink,
            "the edit should survive switching back"
        );
    }

    #[test]
    fn a_recolor_and_a_resize_do_not_cancel_each_other() {
        let mut app = Appearance::default().with_body_size_px(20.0);
        let pink = ThemeColor::new(0.9, 0.8, 0.6, 1.0);
        app.colors_mut().set(ColorSlot::Body, pink);
        let theme = app.document_theme();
        assert_eq!(theme.type_scale.body.size_px, 20.0);
        assert_eq!(theme.type_scale.body.color, pink);
    }
}
