use super::paint::{Palette, ThemeColor};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppTokens {
    pub editor_bg: ThemeColor,
    pub panel_bg: ThemeColor,
    pub bar_bg: ThemeColor,
    pub hover: ThemeColor,
    pub active: ThemeColor,
    pub border: ThemeColor,
    pub border_variant: ThemeColor,
    pub selected_bg: ThemeColor,
    pub text: ThemeColor,
    pub text_muted: ThemeColor,
    pub text_disabled: ThemeColor,
    pub accent: ThemeColor,
    pub syn_cyan: ThemeColor,
    pub syn_red: ThemeColor,
    pub overlay: ThemeColor,
    pub on_accent: ThemeColor,
    pub close_hover: ThemeColor,
    pub ok: ThemeColor,
    pub warn: ThemeColor,
}

impl AppTokens {
    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            editor_bg: p.canvas,
            panel_bg: p.panel_bg,
            bar_bg: p.slate_50,
            hover: p.slate_300,
            active: p.active,
            border: p.slate_200,
            border_variant: p.slate_300,
            selected_bg: p.selection,
            text: p.slate_700,
            text_muted: p.slate_600,
            text_disabled: p.slate_500,
            accent: p.link,
            syn_cyan: p.cyan,
            syn_red: p.bad,
            overlay: p.overlay,
            on_accent: p.on_accent,
            close_hover: p.close_hover,
            ok: p.ok,
            warn: p.warn,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DocumentTheme;

    #[test]
    fn the_shell_colors_survived_the_move_unchanged() {
        let a = DocumentTheme::one_dark().app;
        assert_eq!(a.editor_bg.to_css_hex(), "#282c33");
        assert_eq!(a.panel_bg.to_css_hex(), "#2f343e");
        assert_eq!(a.bar_bg.to_css_hex(), "#3b414d");
        assert_eq!(a.hover.to_css_hex(), "#363c46");
        assert_eq!(a.active.to_css_hex(), "#454a56");
        assert_eq!(a.border.to_css_hex(), "#464b57");
        assert_eq!(a.border_variant.to_css_hex(), "#363c46");
        assert_eq!(a.selected_bg.to_css_hex(), "#293b5b");
        assert_eq!(a.text.to_css_hex(), "#dce0e5");
        assert_eq!(a.text_muted.to_css_hex(), "#a9afbc");
        assert_eq!(a.text_disabled.to_css_hex(), "#878a98");
        assert_eq!(a.accent.to_css_hex(), "#74ade8");
        assert_eq!(a.syn_cyan.to_css_hex(), "#56b6c2");
        assert_eq!(a.syn_red.to_css_hex(), "#e06c75");
        assert_eq!(a.overlay.to_css_hex(), "#0a0c10");
        assert_eq!(a.on_accent.to_css_hex(), "#14181f");
        assert_eq!(a.close_hover.to_css_hex(), "#e81123");
        assert_eq!(a.ok.to_css_hex(), "#a1c181");
        assert_eq!(a.warn.to_css_hex(), "#e5c07b");

        let a = DocumentTheme::one_light().app;
        assert_eq!(a.editor_bg.to_css_hex(), "#fafafa");
        assert_eq!(a.panel_bg.to_css_hex(), "#ebebec");
        assert_eq!(a.bar_bg.to_css_hex(), "#dcdcdd");
        assert_eq!(a.hover.to_css_hex(), "#dfdfe0");
        assert_eq!(a.active.to_css_hex(), "#cacaca");
        assert_eq!(a.border.to_css_hex(), "#c9c9ca");
        assert_eq!(a.border_variant.to_css_hex(), "#dfdfe0");
        assert_eq!(a.selected_bg.to_css_hex(), "#cbcdf6");
        assert_eq!(a.text.to_css_hex(), "#242529");
        assert_eq!(a.text_muted.to_css_hex(), "#58585a");
        assert_eq!(a.text_disabled.to_css_hex(), "#7e8086");
        assert_eq!(a.accent.to_css_hex(), "#5c78e2");
        assert_eq!(a.syn_cyan.to_css_hex(), "#3882b7");
        assert_eq!(a.syn_red.to_css_hex(), "#d36151");
        assert_eq!(a.overlay.to_css_hex(), "#282a30");
        assert_eq!(a.on_accent.to_css_hex(), "#ffffff");
        assert_eq!(a.close_hover.to_css_hex(), "#e81123");
        assert_eq!(a.ok.to_css_hex(), "#649f57");
        assert_eq!(a.warn.to_css_hex(), "#c18401");
    }

    #[test]
    fn the_overlay_keeps_its_alpha() {
        let dark = DocumentTheme::one_dark().app.overlay;
        assert!(
            (dark.a - 0x8c as f32 / 255.0).abs() < f32::EPSILON,
            "{dark:?}"
        );
        let light = DocumentTheme::one_light().app.overlay;
        assert!(
            (light.a - 0x59 as f32 / 255.0).abs() < f32::EPSILON,
            "{light:?}"
        );
    }

    #[test]
    fn the_status_bar_reads_the_same_values_it_used_to() {
        for (theme, bar, border, text, dim, bad) in [
            (
                DocumentTheme::one_dark(),
                "#3b414d",
                "#464b57",
                "#dce0e5",
                "#878a98",
                "#e06c75",
            ),
            (
                DocumentTheme::one_light(),
                "#dcdcdd",
                "#c9c9ca",
                "#242529",
                "#7e8086",
                "#d36151",
            ),
        ] {
            let a = theme.app;
            assert_eq!(a.bar_bg.to_css_hex(), bar, "the original chrome.status_bg");
            assert_eq!(
                a.border.to_css_hex(),
                border,
                "the original chrome.status_border"
            );
            assert_eq!(a.text.to_css_hex(), text, "the original chrome.status_text");
            assert_eq!(a.text_disabled.to_css_hex(), dim, "the original chrome.dim");
            assert_eq!(a.syn_red.to_css_hex(), bad, "the original chrome.bad");
        }
    }

    #[test]
    fn the_scrollbar_keeps_its_own_translucency() {
        let t = DocumentTheme::one_dark();
        let (a, c) = (t.app, t.chrome);
        assert_eq!(a.border_variant.a, 1.0);
        assert_eq!(a.text_disabled.a, 1.0);
        assert!(c.scrollbar_track.a < 0.5, "the track should be translucent");
        assert!(c.scrollbar_thumb.a < 1.0, "the thumb should be translucent");
        let same_hue =
            |x: crate::ThemeColor, y: crate::ThemeColor| (x.h, x.s, x.l) == (y.h, y.s, y.l);
        assert!(same_hue(c.scrollbar_track, a.border_variant));
        assert!(same_hue(c.scrollbar_thumb, a.text_disabled));
    }
}
