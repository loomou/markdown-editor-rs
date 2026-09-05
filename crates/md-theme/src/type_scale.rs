use super::font::{FontStyle, FontWeight, SYSTEM_MONO, SYSTEM_UI};
use super::paint::{Palette, ThemeColor};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeRole {
    pub family: &'static str,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub size_px: f32,
    pub line_height_em: f32,
    pub letter_spacing_px: f32,
    pub color: ThemeColor,
}

impl TypeRole {
    pub fn fingerprint(self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.family.hash(&mut h);
        format!("{:?}", self.weight).hash(&mut h);
        format!("{:?}", self.style).hash(&mut h);
        self.size_px.to_bits().hash(&mut h);
        self.line_height_em.to_bits().hash(&mut h);
        self.letter_spacing_px.to_bits().hash(&mut h);
        self.color.h.to_bits().hash(&mut h);
        self.color.s.to_bits().hash(&mut h);
        self.color.l.to_bits().hash(&mut h);
        self.color.a.to_bits().hash(&mut h);
        h.finish()
    }

    fn without_color(mut self) -> Self {
        self.color = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
        self
    }
}

const UI_FONT: &str = SYSTEM_UI;
const MONO_FONT: &str = SYSTEM_MONO;

fn role(
    family: &'static str,
    weight: FontWeight,
    style: FontStyle,
    size_px: f32,
    line_px: f32,
    color: ThemeColor,
) -> TypeRole {
    TypeRole {
        family,
        weight,
        style,
        size_px,
        line_height_em: line_px / size_px,
        letter_spacing_px: 0.0,
        color,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    pub body: TypeRole,
    pub heading: [TypeRole; 6],
    pub code: TypeRole,
    pub quote: TypeRole,
    pub table: TypeRole,
    pub table_header: TypeRole,
    pub image: TypeRole,
    pub footnote: TypeRole,
    pub task_done: TypeRole,
}

impl TypeScale {
    pub fn roles_mut(&mut self) -> impl Iterator<Item = &mut TypeRole> {
        let Self {
            body,
            heading,
            code,
            quote,
            table,
            table_header,
            image,
            footnote,
            task_done,
        } = self;
        [body].into_iter().chain(heading.iter_mut()).chain([
            code,
            quote,
            table,
            table_header,
            image,
            footnote,
            task_done,
        ])
    }

    pub(crate) fn without_colors(mut self) -> Self {
        for role in self.roles_mut() {
            *role = role.without_color();
        }
        self
    }

    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            body: role(
                UI_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                16.0,
                28.0,
                p.slate_700,
            ),
            heading: [
                TypeRole {
                    letter_spacing_px: -0.3,
                    ..role(
                        UI_FONT,
                        FontWeight::EXTRA_BOLD,
                        FontStyle::Normal,
                        36.0,
                        40.0,
                        p.slate_900,
                    )
                },
                role(
                    UI_FONT,
                    FontWeight::BOLD,
                    FontStyle::Normal,
                    24.0,
                    32.0,
                    p.slate_900,
                ),
                role(
                    UI_FONT,
                    FontWeight::SEMIBOLD,
                    FontStyle::Normal,
                    20.0,
                    32.0,
                    p.slate_900,
                ),
                role(
                    UI_FONT,
                    FontWeight::SEMIBOLD,
                    FontStyle::Normal,
                    16.0,
                    24.0,
                    p.slate_900,
                ),
                role(
                    UI_FONT,
                    FontWeight::SEMIBOLD,
                    FontStyle::Normal,
                    14.0,
                    20.0,
                    p.slate_700,
                ),
                role(
                    UI_FONT,
                    FontWeight::SEMIBOLD,
                    FontStyle::Normal,
                    14.0,
                    20.0,
                    p.slate_600,
                ),
            ],
            code: role(
                MONO_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                14.0,
                24.0,
                p.code_fg,
            ),
            quote: role(
                UI_FONT,
                FontWeight::MEDIUM,
                FontStyle::Italic,
                16.0,
                28.0,
                p.slate_900,
            ),
            table: role(
                UI_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                14.0,
                24.0,
                p.slate_700,
            ),
            table_header: role(
                UI_FONT,
                FontWeight::SEMIBOLD,
                FontStyle::Normal,
                14.0,
                24.0,
                p.slate_900,
            ),
            image: role(
                UI_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                16.0,
                28.0,
                p.slate_500,
            ),
            footnote: role(
                UI_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                14.0,
                24.0,
                p.slate_600,
            ),
            task_done: role(
                UI_FONT,
                FontWeight::NORMAL,
                FontStyle::Normal,
                16.0,
                28.0,
                p.slate_600,
            ),
        }
    }
}
