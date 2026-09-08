use gpui::Hsla;
use md_content::gpui_theme::ThemeColorExt;
use md_theme::AppTokens;

pub const TITLE_BAR_H: f32 = 34.0;
pub const STATUS_BAR_H: f32 = 28.0;
pub const OUTLINE_W: f32 = 220.0;
pub const OUTLINE_HEAD_H: f32 = 32.0;
pub const OUTLINE_ROW_H: f32 = 24.0;
pub const OUTLINE_SB: f32 = 8.0;
pub const OUTLINE_SB_THUMB: f32 = 4.0;
pub const WINCTL_W: f32 = 44.0;
pub const RADIUS: f32 = 6.0;
pub const DLG_W: f32 = 320.0;
pub const DLG_MIN_H: f32 = 168.0;
pub const UI_FONT: &str = md_theme::SYSTEM_UI;
pub const MONO_FONT: &str = md_theme::SYSTEM_MONO;
#[cfg(target_os = "macos")]
pub const TITLE_LEAD_PAD: f32 = 78.0;
#[cfg(not(target_os = "macos"))]
pub const TITLE_LEAD_PAD: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellTheme {
    pub editor_bg: Hsla,
    pub panel_bg: Hsla,
    pub bar_bg: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub border: Hsla,
    pub border_variant: Hsla,
    pub selected_bg: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_disabled: Hsla,
    pub accent: Hsla,
    pub syn_cyan: Hsla,
    pub syn_red: Hsla,
    pub overlay: Hsla,
    pub on_accent: Hsla,
    pub close_hover: Hsla,
}

impl ShellTheme {
    pub fn from_app(a: &AppTokens) -> Self {
        Self {
            editor_bg: a.editor_bg.hsla(),
            panel_bg: a.panel_bg.hsla(),
            bar_bg: a.bar_bg.hsla(),
            hover: a.hover.hsla(),
            active: a.active.hsla(),
            border: a.border.hsla(),
            border_variant: a.border_variant.hsla(),
            selected_bg: a.selected_bg.hsla(),
            text: a.text.hsla(),
            text_muted: a.text_muted.hsla(),
            text_disabled: a.text_disabled.hsla(),
            accent: a.accent.hsla(),
            syn_cyan: a.syn_cyan.hsla(),
            syn_red: a.syn_red.hsla(),
            overlay: a.overlay.hsla(),
            on_accent: a.on_accent.hsla(),
            close_hover: a.close_hover.hsla(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShellTheme;
    use gpui::Hsla;
    use md_theme::DocumentTheme;

    #[test]
    fn the_two_variants_still_paint_the_colors_they_always_did() {
        #[rustfmt::skip]
        let expect: [(&str, [u32; 17]); 2] = [
            ("dark", [
                0x282c33, 0x2f343e, 0x3b414d, 0x363c46, 0x454a56, 0x464b57, 0x363c46,
                0x293b5b, 0xdce0e5, 0xa9afbc, 0x878a98, 0x74ade8, 0x56b6c2, 0xe06c75,
                0x0a0c10, 0x14181f, 0xe81123,
            ]),
            ("light", [
                0xfafafa, 0xebebec, 0xdcdcdd, 0xdfdfe0, 0xcacaca, 0xc9c9ca, 0xdfdfe0,
                0xcbcdf6, 0x242529, 0x58585a, 0x7e8086, 0x5c78e2, 0x3882b7, 0xd36151,
                0x282a30, 0xffffff, 0xe81123,
            ]),
        ];

        for (variant, (label, want)) in [DocumentTheme::one_dark(), DocumentTheme::one_light()]
            .into_iter()
            .zip(expect)
        {
            let t = ShellTheme::from_app(&variant.app);
            #[rustfmt::skip]
            let got = [
                t.editor_bg, t.panel_bg, t.bar_bg, t.hover, t.active, t.border,
                t.border_variant, t.selected_bg, t.text, t.text_muted, t.text_disabled,
                t.accent, t.syn_cyan, t.syn_red, t.overlay, t.on_accent, t.close_hover,
            ];
            for (i, (g, w)) in got.into_iter().zip(want).enumerate() {
                assert_eq!(hex(g), format!("#{w:06x}"), "{label} slot {i}");
            }
        }
    }

    #[test]
    fn the_overlay_arrives_still_translucent() {
        for (label, theme, want) in [
            ("dark", DocumentTheme::one_dark(), 0x8c as f32 / 255.0),
            ("light", DocumentTheme::one_light(), 0x59 as f32 / 255.0),
        ] {
            let a = ShellTheme::from_app(&theme.app).overlay.a;
            assert!(
                (a - want).abs() < 1e-6,
                "{label} mask opacity: {a} != {want}"
            );
        }
    }

    fn hex(c: Hsla) -> String {
        let rgb = gpui::Rgba::from(c);
        let q = |v: f32| (v * 255.0).round() as u32;
        format!("#{:02x}{:02x}{:02x}", q(rgb.r), q(rgb.g), q(rgb.b))
    }
}
