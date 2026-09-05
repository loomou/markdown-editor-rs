use super::DocumentTheme;
use super::paint::ThemeColor;
use md_i18n::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColorGroup {
    Text,
    Surface,
    Cursor,
    Alert,
    Syntax,
    AppSurface,
    AppText,
}

impl ColorGroup {
    pub const ALL: [ColorGroup; 7] = [
        Self::Text,
        Self::Surface,
        Self::Cursor,
        Self::Alert,
        Self::Syntax,
        Self::AppSurface,
        Self::AppText,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn label(self) -> Key {
        match self {
            Self::Text => Key::ClrGroupText,
            Self::Surface => Key::ClrGroupSurface,
            Self::Cursor => Key::ClrGroupCursor,
            Self::Alert => Key::ClrGroupAlert,
            Self::Syntax => Key::ClrGroupSyntax,
            Self::AppSurface => Key::ClrGroupAppSurface,
            Self::AppText => Key::ClrAppText,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn slots(self) -> impl Iterator<Item = ColorSlot> {
        ColorSlot::ALL
            .iter()
            .copied()
            .filter(move |s| s.group() == self)
    }
}

macro_rules! color_slots {
    ($( $variant:ident $key:literal $label:ident $group:ident ($($path:tt)+) )*) => {

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum ColorSlot { $($variant,)* }

        impl ColorSlot {
            pub const ALL: &'static [ColorSlot] = &[$(Self::$variant,)*];

            pub const COUNT: usize = Self::ALL.len();


            pub fn key(self) -> &'static str {
                match self { $(Self::$variant => $key,)* }
            }


            pub fn label(self) -> Key {
                match self { $(Self::$variant => Key::$label,)* }
            }

            pub fn group(self) -> ColorGroup {
                match self { $(Self::$variant => ColorGroup::$group,)* }
            }

            pub fn from_key(key: &str) -> Option<Self> {
                match key { $($key => Some(Self::$variant),)* _ => None }
            }


            pub fn read(self, theme: &DocumentTheme) -> ThemeColor {
                match self { $(Self::$variant => theme.$($path)+,)* }
            }

            fn place(self, theme: &mut DocumentTheme) -> &mut ThemeColor {
                match self { $(Self::$variant => &mut theme.$($path)+,)* }
            }
        }
    };
}

color_slots! {

    Body            "editor.body"              ClrBody               Text       (type_scale.body.color)
    Heading1        "editor.h1"                ClrHeading1           Text       (type_scale.heading[0].color)
    Heading2        "editor.h2"                ClrHeading2           Text       (type_scale.heading[1].color)
    Heading3        "editor.h3"                ClrHeading3           Text       (type_scale.heading[2].color)
    Heading4        "editor.h4"                ClrHeading4           Text       (type_scale.heading[3].color)
    Heading5        "editor.h5"                ClrHeading5           Text       (type_scale.heading[4].color)
    Heading6        "editor.h6"                ClrHeading6           Text       (type_scale.heading[5].color)
    Quote           "editor.quote"             ClrQuote              Text       (type_scale.quote.color)
    Link            "editor.link"              ClrLink               Text       (inline.link)
    InlineCode      "editor.inline_code"       ClrInlineCode         Text       (inline.inline_code)
    CodeText        "editor.code"              ClrCodeText           Text       (type_scale.code.color)
    WellLang        "editor.well_lang"         ClrWellLang           Text       (decoration.well_lang)
    TableCell       "editor.table"             ClrTableCell          Text       (type_scale.table.color)
    TableHead       "editor.table_head"        ClrTableHead          Text       (type_scale.table_header.color)
    ImageAlt        "editor.image_alt"         ClrImageAlt           Text       (type_scale.image.color)
    Footnote        "editor.footnote"          ClrFootnote           Text       (type_scale.footnote.color)
    TaskDone        "editor.task_done"         ClrTaskDone           Text       (type_scale.task_done.color)
    Strikethrough   "editor.strike"            ClrStrikethrough      Text       (inline.strikethrough)
    SyntaxMarker    "editor.marker"            ClrSyntaxMarker       Text       (inline.syntax_marker)


    Canvas          "editor.canvas"            ClrCanvas             Surface    (paint.canvas)
    CodeFill        "editor.code_fill"         ClrCodeFill           Surface    (paint.code_fill)
    CodeBorder      "editor.code_border"       ClrCodeBorder         Surface    (paint.code_border)
    InlineCodeFill  "editor.inline_code_fill"  ClrInlineCodeFill     Surface    (inline.inline_code_fill)
    QuoteBar        "editor.quote_bar"         ClrQuoteBar           Surface    (paint.quote_bar)
    ListMarker      "editor.list_marker"       ClrListMarker         Surface    (paint.list_marker)
    TableGrid       "editor.table_grid"        ClrTableGrid          Surface    (paint.table_grid)
    TableBorder     "editor.table_border"      ClrTableBorder        Surface    (paint.table_border)
    TableHeadFill   "editor.table_head_fill"   ClrTableHeadFill      Surface    (paint.table_head_fill)
    Rule            "editor.rule"              ClrRule               Surface    (paint.rule)
    ImageFill       "editor.image_fill"        ClrImageFill          Surface    (inline.image_fill)
    ImageBorder     "editor.image_border"      ClrImageBorder        Surface    (inline.image_border)


    Caret           "editor.caret"             ClrCaret              Cursor     (paint.caret)
    Selection       "editor.selection"         ClrSelection          Cursor     (paint.selection)
    Ime             "editor.ime"               ClrIme                Cursor     (paint.ime)
    SearchMatch     "editor.search"            ClrSearchMatch        Cursor     (paint.search_match)
    SearchActive    "editor.search_active"     ClrSearchActive       Cursor     (paint.search_match_active)
    TaskBorder      "editor.task_border"       ClrTaskBorder         Cursor     (paint.task_border)
    TaskChecked     "editor.task_checked"      ClrTaskChecked        Cursor     (paint.task_checked)
    TaskCheck       "editor.task_check"        ClrTaskCheck          Cursor     (paint.task_check)
    GlyphFault      "editor.glyph_fault"       ClrGlyphFault         Cursor     (paint.glyph_fault)


    AlertNote       "editor.alert_note"        ClrAlertNote          Alert      (decoration.alert_note)
    AlertTip        "editor.alert_tip"         ClrAlertTip           Alert      (decoration.alert_tip)
    AlertImportant  "editor.alert_important"   ClrAlertImportant     Alert      (decoration.alert_important)
    AlertWarning    "editor.alert_warning"     ClrAlertWarning       Alert      (decoration.alert_warning)
    AlertCaution    "editor.alert_caution"     ClrAlertCaution       Alert      (decoration.alert_caution)



    SynDefault      "editor.syntax.default"    ClrSynDefault         Syntax     (syntax.default)
    SynComment      "editor.syntax.comment"    ClrSynComment         Syntax     (syntax.comment)
    SynKeyword      "editor.syntax.keyword"    ClrSynKeyword         Syntax     (syntax.keyword)
    SynString       "editor.syntax.string"     ClrSynString          Syntax     (syntax.string)
    SynCharacter    "editor.syntax.character"  ClrSynCharacter       Syntax     (syntax.character)
    SynSpecial      "editor.syntax.escape"     ClrSynSpecial         Syntax     (syntax.special)
    SynSymbol       "editor.syntax.symbol"     ClrSynSymbol          Syntax     (syntax.symbol)
    SynNumber       "editor.syntax.number"     ClrSynNumber          Syntax     (syntax.number)
    SynFunction     "editor.syntax.function"   ClrSynFunction        Syntax     (syntax.function)
    SynMacro        "editor.syntax.macro"      ClrSynMacro           Syntax     (syntax.macro_name)
    SynType         "editor.syntax.type"       ClrSynType            Syntax     (syntax.type_name)
    SynProperty     "editor.syntax.property"   ClrSynProperty        Syntax     (syntax.property)
    SynOperator     "editor.syntax.operator"   ClrSynOperator        Syntax     (syntax.operator)
    SynParameter    "editor.syntax.parameter"  ClrSynParameter       Syntax     (syntax.parameter)
    SynBuiltin      "editor.syntax.builtin"    ClrSynBuiltin         Syntax     (syntax.builtin)
    SynPunctuation  "editor.syntax.punctuation" ClrSynPunctuation    Syntax     (syntax.punctuation)
    SynLabel        "editor.syntax.label"      ClrSynLabel           Syntax     (syntax.label)




    AppEditorBg     "app.editor_bg"            ClrAppEditorBg        AppSurface (app.editor_bg)
    AppPanelBg      "app.panel_bg"             ClrAppPanelBg         AppSurface (app.panel_bg)
    AppBarBg        "app.bar_bg"               ClrAppBarBg           AppSurface (app.bar_bg)
    AppHover        "app.hover"                ClrAppHover           AppSurface (app.hover)
    AppActive       "app.active"               ClrAppActive          AppSurface (app.active)
    AppBorder       "app.border"               ClrAppBorder          AppSurface (app.border)
    AppBorderVar    "app.border_variant"       ClrAppBorderVar       AppSurface (app.border_variant)
    AppSelectedBg   "app.selected_bg"          ClrAppSelectedBg      AppSurface (app.selected_bg)
    AppOverlay      "app.overlay"              ClrAppOverlay         AppSurface (app.overlay)
    AppCloseHover   "app.close_hover"          ClrAppCloseHover      AppSurface (app.close_hover)
    AppScrollTrack  "app.scrollbar_track"      ClrAppScrollTrack     AppSurface (chrome.scrollbar_track)
    AppScrollThumb  "app.scrollbar_thumb"      ClrAppScrollThumb     AppSurface (chrome.scrollbar_thumb)


    AppText         "app.text"                 ClrAppText            AppText    (app.text)
    AppTextMuted    "app.text_muted"           ClrAppTextMuted       AppText    (app.text_muted)
    AppTextDisabled "app.text_disabled"        ClrAppTextDisabled    AppText    (app.text_disabled)
    AppAccent       "app.accent"               ClrAppAccent          AppText    (app.accent)
    AppOnAccent     "app.on_accent"            ClrAppOnAccent        AppText    (app.on_accent)
    AppCyan         "app.cyan"                 ClrAppCyan            AppText    (app.syn_cyan)
    AppRed          "app.red"                  ClrAppRed             AppText    (app.syn_red)
    AppOk           "app.ok"                   ClrAppOk              AppText    (app.ok)
    AppWarn         "app.warn"                 ClrAppWarn            AppText    (app.warn)
}

impl ColorSlot {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn write(self, theme: &mut DocumentTheme, color: ThemeColor) {
        let dst = self.place(theme);
        *dst = ThemeColor { a: dst.a, ..color };
        let follower = match self {
            Self::Link => &mut theme.inline.link_underline,
            Self::Strikethrough => &mut theme.inline.task_strike,
            _ => return,
        };
        *follower = ThemeColor {
            a: follower.a,
            ..color
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorOverrides {
    slots: [Option<ThemeColor>; ColorSlot::COUNT],
}

impl Default for ColorOverrides {
    fn default() -> Self {
        Self {
            slots: [None; ColorSlot::COUNT],
        }
    }
}

impl ColorOverrides {
    pub fn get(&self, slot: ColorSlot) -> Option<ThemeColor> {
        self.slots[slot.index()]
    }

    pub fn set(&mut self, slot: ColorSlot, color: ThemeColor) {
        self.slots[slot.index()] = Some(ThemeColor { a: 1.0, ..color });
    }

    pub fn clear(&mut self, slot: ColorSlot) {
        self.slots[slot.index()] = None;
    }

    pub fn clear_all(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ColorSlot, ThemeColor)> + '_ {
        ColorSlot::ALL
            .iter()
            .copied()
            .filter_map(|s| self.get(s).map(|c| (s, c)))
    }

    pub fn apply_to(&self, theme: &mut DocumentTheme) {
        for (slot, color) in self.iter() {
            slot.write(theme, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorGroup, ColorOverrides, ColorSlot};
    use crate::{DocumentTheme, ThemeColor};

    const PROBE: ThemeColor = ThemeColor::new(0.123_456, 0.654_321, 0.5, 1.0);

    #[test]
    fn every_key_reads_as_a_path_under_app_or_editor() {
        for slot in ColorSlot::ALL {
            let key = slot.key();
            let segments: Vec<&str> = key.split('.').collect();
            assert!(
                matches!(segments[0], "app" | "editor"),
                "{key} starts with neither app nor editor"
            );
            assert!(
                (2..=3).contains(&segments.len()),
                "{key} split into {} segments",
                segments.len()
            );
            for seg in &segments {
                assert!(!seg.is_empty(), "{key} has an empty segment");
                assert!(
                    seg.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                    "{key}'s segment `{seg}` contains a disallowed character"
                );
            }

            let is_app = matches!(slot.group(), ColorGroup::AppSurface | ColorGroup::AppText);
            assert_eq!(
                is_app,
                segments[0] == "app",
                "{key}'s group and prefix do not match"
            );
        }
    }

    #[test]
    fn the_syntax_keys_use_the_names_the_community_uses() {
        for slot in ColorGroup::Syntax.slots() {
            let key = slot.key();
            let name = key.strip_prefix("editor.syntax.").unwrap_or_else(|| {
                panic!("{key} is not under editor.syntax");
            });
            assert!(!name.starts_with("syn"), "{key} has the syn prefix again");
        }

        for (want, slot) in [
            ("editor.syntax.keyword", ColorSlot::SynKeyword),
            ("editor.syntax.string", ColorSlot::SynString),
            ("editor.syntax.comment", ColorSlot::SynComment),
            ("editor.syntax.type", ColorSlot::SynType),
        ] {
            assert_eq!(slot.key(), want);
        }
    }

    #[test]
    fn the_shell_colors_can_be_overridden_one_by_one() {
        let base = DocumentTheme::one_dark();
        let mut theme = base;
        ColorSlot::AppBarBg.write(&mut theme, PROBE);
        let got = theme.app.bar_bg;
        assert_eq!((got.h, got.s, got.l), (PROBE.h, PROBE.s, PROBE.l));

        assert_eq!(theme.app.panel_bg, base.app.panel_bg);

        assert!(base.layout_metrics_eq(&theme));
    }

    #[test]
    fn the_translucent_shell_slots_stay_translucent() {
        let base = DocumentTheme::one_dark();
        for slot in [
            ColorSlot::AppOverlay,
            ColorSlot::AppScrollTrack,
            ColorSlot::AppScrollThumb,
        ] {
            let was = slot.read(&base).a;
            assert!(was < 1.0, "{} should be translucent", slot.key());
            let mut theme = base;
            slot.write(&mut theme, PROBE);
            assert_eq!(
                slot.read(&theme).a,
                was,
                "{}'s alpha was changed",
                slot.key()
            );
        }
    }

    #[test]
    fn every_key_is_unique_and_round_trips() {
        let mut keys: Vec<&str> = ColorSlot::ALL.iter().map(|s| s.key()).collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "the on-disk keys collide");
        for (i, slot) in ColorSlot::ALL.iter().enumerate() {
            assert_eq!(ColorSlot::from_key(slot.key()), Some(*slot));
            assert_eq!(
                slot.index(),
                i,
                "{}'s index does not match its position in the table",
                slot.key()
            );
        }
        assert_eq!(ColorSlot::from_key("no-such-color"), None);
        assert_eq!(ColorSlot::COUNT, total);
    }

    #[test]
    fn the_labels_line_up_with_the_slots() {
        let mut seen: Vec<&str> = Vec::new();
        for slot in ColorSlot::ALL {
            let variant = format!("{slot:?}");
            let want = format!("Clr{variant}");
            let got = slot.label().debug_name();
            assert_eq!(got, want, "{}'s label points to {got}", slot.key());
            seen.push(got);
        }
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "two slots share the same label");

        for group in ColorGroup::ALL {
            assert!(!md_i18n::t(group.label()).is_empty());
        }
    }

    #[test]
    fn each_slot_moves_exactly_one_color() {
        let base = DocumentTheme::one_dark();
        for slot in ColorSlot::ALL {
            let mut theme = base;
            slot.write(&mut theme, PROBE);
            let got = slot.read(&theme);
            assert_eq!((got.h, got.s, got.l), (PROBE.h, PROBE.s, PROBE.l));
            for other in ColorSlot::ALL {
                if other == slot {
                    continue;
                }
                assert_eq!(
                    other.read(&theme),
                    other.read(&base),
                    "changing {} also changed {}",
                    slot.key(),
                    other.key()
                );
            }
        }
    }

    #[test]
    fn the_themes_own_alpha_survives_a_recolor() {
        let base = DocumentTheme::one_dark();
        let mut translucent = 0;
        for slot in ColorSlot::ALL {
            let mut theme = base;
            slot.write(&mut theme, PROBE);
            let was = slot.read(&base).a;
            assert_eq!(
                slot.read(&theme).a,
                was,
                "{}'s alpha was changed",
                slot.key()
            );
            if was < 1.0 {
                translucent += 1;
            }
        }

        assert!(
            translucent >= 4,
            "found only {translucent} translucent colors"
        );

        let mut theme = base;
        ColorSlot::Canvas.write(&mut theme, PROBE);
        assert_eq!(ColorSlot::Canvas.read(&theme), PROBE);
    }

    #[test]
    fn the_faint_companions_follow_along() {
        let base = DocumentTheme::one_dark();
        let mut theme = base;
        ColorSlot::Link.write(&mut theme, PROBE);
        let u = theme.inline.link_underline;
        assert_eq!((u.h, u.s, u.l), (PROBE.h, PROBE.s, PROBE.l));
        assert_eq!(u.a, base.inline.link_underline.a);

        ColorSlot::Strikethrough.write(&mut theme, PROBE);
        let s = theme.inline.task_strike;
        assert_eq!((s.h, s.s, s.l), (PROBE.h, PROBE.s, PROBE.l));
        assert_eq!(s.a, base.inline.task_strike.a);
    }

    #[test]
    fn the_groups_partition_the_table() {
        let mut seen = 0;
        for group in ColorGroup::ALL {
            let n = group.slots().count();
            assert!(n > 0, "{} is an empty group", group.label().debug_name());
            for slot in group.slots() {
                assert_eq!(slot.group(), group);
            }
            seen += n;
        }
        assert_eq!(seen, ColorSlot::COUNT);
        for (i, g) in ColorGroup::ALL.iter().enumerate() {
            assert_eq!(g.index(), i);
        }
    }

    #[test]
    fn an_override_lands_and_lifts() {
        let mut over = ColorOverrides::default();
        assert!(over.is_empty());
        over.set(ColorSlot::Canvas, PROBE);
        over.set(ColorSlot::SynKeyword, PROBE);
        assert_eq!(over.len(), 2);
        assert_eq!(over.get(ColorSlot::Canvas), Some(PROBE));
        assert_eq!(over.get(ColorSlot::Caret), None);
        assert_eq!(
            over.iter().map(|(s, _)| s).collect::<Vec<_>>(),
            vec![ColorSlot::Canvas, ColorSlot::SynKeyword],
            "iter should follow the table order"
        );

        let base = DocumentTheme::one_dark();
        let mut theme = base;
        over.apply_to(&mut theme);
        assert_eq!(ColorSlot::Canvas.read(&theme), PROBE);
        assert_eq!(ColorSlot::SynKeyword.read(&theme), PROBE);
        assert_eq!(ColorSlot::Caret.read(&theme), ColorSlot::Caret.read(&base));

        over.clear(ColorSlot::Canvas);
        assert_eq!(over.get(ColorSlot::Canvas), None);
        over.clear_all();
        assert!(over.is_empty());
        let mut theme = base;
        over.apply_to(&mut theme);
        assert_eq!(theme, base, "an empty override should not change anything");
    }

    #[test]
    fn stored_overrides_are_opaque() {
        let mut over = ColorOverrides::default();
        over.set(ColorSlot::Selection, ThemeColor { a: 0.3, ..PROBE });
        assert_eq!(over.get(ColorSlot::Selection), Some(PROBE));
    }

    #[test]
    fn the_shipped_colors_are_all_reachable_from_a_hex_string() {
        for variant in crate::ThemeVariant::ALL {
            let base = variant.document_theme();
            for slot in ColorSlot::ALL {
                let shipped = slot.read(&base);
                let mut over = ColorOverrides::default();
                over.set(
                    *slot,
                    ThemeColor::from_css_hex(&shipped.to_css_hex()).expect("recognized"),
                );
                let mut theme = base;
                over.apply_to(&mut theme);
                assert_eq!(
                    slot.read(&theme),
                    shipped,
                    "{}'s shipped color could not be reproduced ({})",
                    slot.key(),
                    shipped.to_css_hex()
                );
            }
        }
    }
}
