const SLATE_50: ThemeColor = ThemeColor::new(0.611, 0.14, 0.97, 1.0);
const SLATE_200: ThemeColor = ThemeColor::new(0.611, 0.13, 0.91, 1.0);
const SLATE_300: ThemeColor = ThemeColor::new(0.600, 0.12, 0.84, 1.0);
const SLATE_500: ThemeColor = ThemeColor::new(0.611, 0.09, 0.46, 1.0);
const SLATE_600: ThemeColor = ThemeColor::new(0.611, 0.15, 0.34, 1.0);
const SLATE_700: ThemeColor = ThemeColor::new(0.606, 0.19, 0.27, 1.0);
const SLATE_800: ThemeColor = ThemeColor::new(0.603, 0.28, 0.17, 1.0);
const SLATE_900: ThemeColor = ThemeColor::new(0.611, 0.39, 0.11, 1.0);
const LINK: ThemeColor = ThemeColor::new(0.553, 0.65, 0.42, 1.0);
const OK: ThemeColor = ThemeColor::new(0.34, 0.45, 0.34, 1.0);
const BAD: ThemeColor = ThemeColor::new(0.0, 0.65, 0.42, 1.0);
const WARN: ThemeColor = ThemeColor::new(0.08, 0.70, 0.40, 1.0);
const WHITE: ThemeColor = ThemeColor::new(0.0, 0.0, 1.0, 1.0);

fn cyan() -> ThemeColor {
    rgb(0x56b6c2)
}

fn close_hover() -> ThemeColor {
    rgb(0xe81123)
}

fn rgb(hex: u32) -> ThemeColor {
    ThemeColor::from_srgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeColor {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

impl ThemeColor {
    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        ThemeColor { h, s, l, a }
    }

    pub fn to_css_hex(self) -> String {
        let (r, g, b) = hsl_to_srgb(self.h, self.s, self.l);
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    pub fn from_css_hex(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let n = u32::from_str_radix(hex, 16).ok()?;
        Some(Self::from_srgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }

    pub fn to_rgba_u32(self) -> u32 {
        let (r, g, b) = hsl_to_srgb(self.h, self.s, self.l);
        let a = (self.a * 255.0).round().clamp(0.0, 255.0) as u8;
        u32::from_be_bytes([r, g, b, a])
    }

    pub fn from_srgb(r: u8, g: u8, b: u8) -> Self {
        let rf = r as f32 / 255.0;
        let gf = g as f32 / 255.0;
        let bf = b as f32 / 255.0;
        let max = rf.max(gf).max(bf);
        let min = rf.min(gf).min(bf);
        let l = (max + min) / 2.0;
        let (h, s) = if (max - min).abs() < f32::EPSILON {
            (0.0, 0.0)
        } else {
            let d = max - min;
            let s = d / (1.0 - (2.0 * l - 1.0).abs());
            let h = if (max - rf).abs() <= f32::EPSILON {
                ((gf - bf) / d).rem_euclid(6.0)
            } else if (max - gf).abs() <= f32::EPSILON {
                (bf - rf) / d + 2.0
            } else {
                (rf - gf) / d + 4.0
            } / 6.0;
            (h, s)
        };
        let mut best = ThemeColor::new(h, s, l, 1.0);
        if hsl_to_srgb(best.h, best.s, best.l) == (r, g, b) {
            return best;
        }
        let mut best_err = u32::MAX;
        for dh in -40..=40 {
            for ds in -40..=40 {
                let cand = ThemeColor::new(
                    h + dh as f32 * 5.0e-5,
                    (s + ds as f32 * 5.0e-5).clamp(0.0, 1.0),
                    l,
                    1.0,
                );
                let (rr, gg, bb) = hsl_to_srgb(cand.h, cand.s, cand.l);
                let err = u32::from(rr.abs_diff(r))
                    + u32::from(gg.abs_diff(g))
                    + u32::from(bb.abs_diff(b));
                if err < best_err {
                    best_err = err;
                    best = cand;
                    if err == 0 {
                        return best;
                    }
                }
            }
        }
        best
    }
}

fn hsl_to_srgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 1.0) + 1.0) % 1.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if hp < 1.0 {
        (c, x, 0.0)
    } else if hp < 2.0 {
        (x, c, 0.0)
    } else if hp < 3.0 {
        (0.0, c, x)
    } else if hp < 4.0 {
        (0.0, x, c)
    } else if hp < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(r1), byte(g1), byte(b1))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Palette {
    pub slate_50: ThemeColor,
    pub slate_200: ThemeColor,
    pub slate_300: ThemeColor,
    pub slate_500: ThemeColor,
    pub slate_600: ThemeColor,
    pub slate_700: ThemeColor,
    pub slate_800: ThemeColor,
    pub slate_900: ThemeColor,
    pub caret: ThemeColor,
    pub code_fg: ThemeColor,
    pub inline_code: ThemeColor,
    pub inline_code_fill: ThemeColor,
    pub search_match: ThemeColor,
    pub search_match_active: ThemeColor,
    pub canvas: ThemeColor,
    pub selection: ThemeColor,
    pub ime: ThemeColor,
    pub glyph_fault: ThemeColor,
    pub link: ThemeColor,
    pub ok: ThemeColor,
    pub bad: ThemeColor,
    pub warn: ThemeColor,
    pub scrollbar_track: ThemeColor,
    pub scrollbar_thumb: ThemeColor,

    pub panel_bg: ThemeColor,
    pub active: ThemeColor,
    pub cyan: ThemeColor,
    pub overlay: ThemeColor,
    pub on_accent: ThemeColor,

    pub close_hover: ThemeColor,
}

impl Palette {
    pub(crate) fn formal() -> Self {
        Self {
            slate_50: SLATE_50,
            slate_200: SLATE_200,
            slate_300: SLATE_300,
            slate_500: SLATE_500,
            slate_600: SLATE_600,
            slate_700: SLATE_700,
            slate_800: SLATE_800,
            slate_900: SLATE_900,
            caret: SLATE_900,
            code_fg: SLATE_200,
            inline_code: SLATE_800,
            inline_code_fill: SLATE_200,
            search_match: ThemeColor::new(0.13, 0.75, 0.72, 0.42),
            search_match_active: ThemeColor::new(0.13, 0.75, 0.72, 0.42),
            canvas: ThemeColor::new(0.0, 0.0, 1.0, 1.0),
            selection: ThemeColor::new(0.553, 0.70, 0.86, 0.42),
            ime: ThemeColor::new(0.553, 0.65, 0.78, 0.55),
            glyph_fault: ThemeColor::new(0.0, 0.8, 0.5, 0.35),
            link: LINK,
            ok: OK,
            bad: BAD,
            warn: WARN,
            scrollbar_track: ThemeColor::new(0.611, 0.13, 0.91, 0.45),
            scrollbar_thumb: ThemeColor::new(0.611, 0.09, 0.46, 0.62),

            panel_bg: SLATE_300,
            active: SLATE_200,
            cyan: cyan(),
            overlay: ThemeColor::new(0.611, 0.14, 0.11, 0.35),
            on_accent: WHITE,
            close_hover: close_hover(),
        }
    }

    pub(crate) fn one_dark() -> Self {
        Self {
            slate_50: rgb(0x3b414d),
            slate_200: rgb(0x464b57),
            slate_300: rgb(0x363c46),
            slate_500: rgb(0x878a98),
            slate_600: rgb(0xa9afbc),
            slate_700: rgb(0xdce0e5),
            slate_800: rgb(0x21252c),
            slate_900: rgb(0xdce0e5),
            caret: rgb(0xdce0e5),
            code_fg: rgb(0xacb2be),
            inline_code: rgb(0xe06c75),
            inline_code_fill: rgb(0x343a45),
            search_match: ThemeColor {
                a: 0.22,
                ..rgb(0xe5c07b)
            },
            search_match_active: ThemeColor {
                a: 0.45,
                ..rgb(0xe5c07b)
            },
            canvas: rgb(0x282c33),
            selection: rgb(0x293b5b),
            ime: ThemeColor {
                a: 0.55,
                ..rgb(0x74ade8)
            },
            glyph_fault: ThemeColor {
                a: 0.35,
                ..rgb(0xe06c75)
            },
            link: rgb(0x74ade8),
            ok: rgb(0xa1c181),
            bad: rgb(0xe06c75),
            warn: rgb(0xe5c07b),
            scrollbar_track: ThemeColor {
                a: 0.45,
                ..rgb(0x363c46)
            },
            scrollbar_thumb: ThemeColor {
                a: 0.62,
                ..rgb(0x878a98)
            },
            panel_bg: rgb(0x2f343e),
            active: rgb(0x454a56),
            cyan: cyan(),

            overlay: ThemeColor {
                a: 0x8c as f32 / 255.0,
                ..rgb(0x0a0c10)
            },
            on_accent: rgb(0x14181f),
            close_hover: close_hover(),
        }
    }

    pub(crate) fn one_light() -> Self {
        Self {
            slate_50: rgb(0xdcdcdd),
            slate_200: rgb(0xc9c9ca),
            slate_300: rgb(0xdfdfe0),
            slate_500: rgb(0x7e8086),
            slate_600: rgb(0x58585a),
            slate_700: rgb(0x242529),
            slate_800: rgb(0xefefef),
            slate_900: rgb(0x242529),
            caret: rgb(0x242529),
            code_fg: rgb(0x383a40),
            inline_code: rgb(0xd36151),
            inline_code_fill: rgb(0xececec),
            search_match: ThemeColor {
                a: 0.16,
                ..rgb(0xc18401)
            },
            search_match_active: ThemeColor {
                a: 0.34,
                ..rgb(0xc18401)
            },
            canvas: rgb(0xfafafa),
            selection: rgb(0xcbcdf6),
            ime: ThemeColor {
                a: 0.55,
                ..rgb(0x5c78e2)
            },
            glyph_fault: ThemeColor {
                a: 0.35,
                ..rgb(0xd36151)
            },
            link: rgb(0x5c78e2),
            ok: rgb(0x649f57),
            bad: rgb(0xd36151),
            warn: rgb(0xc18401),
            scrollbar_track: ThemeColor {
                a: 0.45,
                ..rgb(0xdfdfe0)
            },
            scrollbar_thumb: ThemeColor {
                a: 0.62,
                ..rgb(0x7e8086)
            },
            panel_bg: rgb(0xebebec),
            active: rgb(0xcacaca),
            cyan: rgb(0x3882b7),

            overlay: ThemeColor {
                a: 0x59 as f32 / 255.0,
                ..rgb(0x282a30)
            },
            on_accent: WHITE,
            close_hover: close_hover(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintTokens {
    pub canvas: ThemeColor,
    pub caret: ThemeColor,
    pub caret_width: f64,

    pub caret_blink_ms: f64,
    pub selection: ThemeColor,
    pub ime: ThemeColor,
    pub quote_bar: ThemeColor,
    pub code_fill: ThemeColor,
    pub code_border: ThemeColor,
    pub table_grid: ThemeColor,
    pub table_border: ThemeColor,
    pub table_head_fill: ThemeColor,
    pub list_marker: ThemeColor,
    pub task_border: ThemeColor,
    pub task_checked: ThemeColor,
    pub task_check: ThemeColor,
    pub glyph_fault: ThemeColor,
    pub rule: ThemeColor,
    pub search_match: ThemeColor,
    pub search_match_active: ThemeColor,
}

impl PaintTokens {
    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            canvas: p.canvas,
            caret: p.caret,
            caret_width: 2.0,
            caret_blink_ms: 530.0,
            selection: p.selection,
            ime: p.ime,
            quote_bar: p.slate_200,
            code_fill: p.slate_800,
            code_border: p.slate_300,
            table_grid: p.slate_200,
            table_border: p.slate_300,
            table_head_fill: p.slate_800,
            list_marker: p.slate_300,
            task_border: p.slate_200,
            task_checked: p.link,
            task_check: ThemeColor::new(0.0, 0.0, 1.0, 1.0),
            glyph_fault: p.glyph_fault,
            rule: p.slate_300,
            search_match: p.search_match,
            search_match_active: p.search_match_active,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeTokens {
    pub scrollbar_track: ThemeColor,

    pub scrollbar_thumb: ThemeColor,
    pub status_font: &'static str,
    pub status_size_px: f32,

    pub status_line_height_em: f32,
    pub scrollbar_hit: f64,
    pub scrollbar_thumb_w: f64,
    pub scrollbar_pad: f64,
    pub scrollbar_min_thumb: f64,
    pub select_autoscroll_edge: f64,
    pub select_autoscroll_outside: f64,
    pub select_autoscroll_edge_px: f64,
    pub select_autoscroll_max_px: f64,
    pub search_comfort_lines: f64,
    pub search_comfort_vh: f64,
    pub search_park_next: f64,
    pub search_park_prev: f64,
    pub window_width: f32,
    pub window_height: f32,
}

impl ChromeTokens {
    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            scrollbar_track: p.scrollbar_track,
            scrollbar_thumb: p.scrollbar_thumb,
            status_font: crate::SYSTEM_MONO,
            status_size_px: 11.0,
            status_line_height_em: 1.4,
            scrollbar_hit: 14.0,
            scrollbar_thumb_w: 6.0,
            scrollbar_pad: 4.0,
            scrollbar_min_thumb: 32.0,
            select_autoscroll_edge: 56.0,
            select_autoscroll_outside: 96.0,
            select_autoscroll_edge_px: 12.0,
            select_autoscroll_max_px: 32.0,
            search_comfort_lines: 3.0,
            search_comfort_vh: 0.22,
            search_park_next: 0.28,
            search_park_prev: 0.72,
            window_width: 900.0,
            window_height: 680.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PaintTokens, Palette, ThemeColor};

    #[test]
    fn hex_survives_the_trip_through_hsl() {
        for hex in [
            "#000000", "#ffffff", "#282c33", "#abb2bf", "#e06c75", "#5c78e2", "#c18401", "#7f7f80",
            "#010203", "#fedcba",
        ] {
            let c = ThemeColor::from_css_hex(hex).expect("recognized");
            assert_eq!(c.to_css_hex(), hex);
            assert_eq!(c.a, 1.0, "hex should not carry alpha");
        }
    }

    #[test]
    fn the_shipped_colors_round_trip_too() {
        let p = PaintTokens::from_palette(&Palette::one_dark());
        for c in [p.canvas, p.caret, p.code_fill, p.rule, p.list_marker] {
            let back = ThemeColor::from_css_hex(&c.to_css_hex()).expect("recognized");
            assert_eq!(back.to_css_hex(), c.to_css_hex());
        }
    }

    #[test]
    fn a_color_read_back_from_hex_is_a_fixed_point() {
        for c in [
            ThemeColor::new(0.592_592_6, 0.5, 0.5, 1.0),
            ThemeColor::new(0.0, 0.0, 1.0, 1.0),
            ThemeColor::new(0.333_333_3, 1.0, 0.5, 1.0),
            ThemeColor::new(0.777, 0.123, 0.456, 1.0),
            ThemeColor::new(0.1, 0.03, 0.97, 1.0),
        ] {
            let once = ThemeColor::from_css_hex(&c.to_css_hex()).expect("recognized");
            assert_eq!(
                once.to_css_hex(),
                c.to_css_hex(),
                "the on-screen color must be the same"
            );
            let twice = ThemeColor::from_css_hex(&once.to_css_hex()).expect("recognized");
            assert_eq!(
                twice, once,
                "the second round trip still moved: {once:?} -> {twice:?}"
            );
        }
    }

    #[test]
    fn lightness_is_piecewise_linear_in_srgb() {
        let chans = |c: ThemeColor| {
            let n = c.to_rgba_u32().to_be_bytes();
            [n[0] as f32, n[1] as f32, n[2] as f32]
        };
        for h in [0.0, 0.08, 0.333, 0.553, 0.9] {
            for s in [0.2, 0.6, 1.0] {
                let mid = chans(ThemeColor::new(h, s, 0.5, 1.0));
                for i in 1..10 {
                    let f = i as f32 / 10.0;
                    let up = chans(ThemeColor::new(h, s, 1.0 - f * 0.5, 1.0));
                    let down = chans(ThemeColor::new(h, s, 0.5 - f * 0.5, 1.0));
                    for k in 0..3 {
                        let want_up = 255.0 + (mid[k] - 255.0) * f;
                        let want_down = mid[k] * (1.0 - f);
                        assert!(
                            (up[k] - want_up).abs() <= 1.0,
                            "h={h} s={s} l={} channel {k}: measured {} but the line value is {want_up}",
                            1.0 - f * 0.5,
                            up[k]
                        );
                        assert!(
                            (down[k] - want_down).abs() <= 1.0,
                            "h={h} s={s} l={} channel {k}: measured {} but the line value is {want_down}",
                            0.5 - f * 0.5,
                            down[k]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn anything_that_is_not_six_hex_digits_is_refused() {
        for bad in [
            "", "#", "#fff", "#12345", "#1234567", "abcdef", "#gggggg", "#12 456", "#-12345",
        ] {
            assert_eq!(
                ThemeColor::from_css_hex(bad),
                None,
                "{bad:?} should not be recognized"
            );
        }

        assert_eq!(
            ThemeColor::from_css_hex("  #282c33 ").map(ThemeColor::to_css_hex),
            Some("#282c33".to_string())
        );
    }
}
