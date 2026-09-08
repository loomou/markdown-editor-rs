use super::paint::{Palette, ThemeColor};
use md_core::Px;
use md_core::block::BlockKind;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationTokens {
    pub list_marker_size: Px,
    pub task_size: Px,
    pub task_radius: f32,
    pub task_border: Px,
    pub task_gap: Px,
    pub list_gutter_min: Px,
    pub list_digit_width: Px,
    pub list_task_extra: Px,
    pub list_marker_gap: Px,
    pub list_nested_top: Px,
    pub image_placeholder_height: Px,
    pub image_max_width: Px,
    pub image_max_height: Px,
    pub image_popover_max_width: Px,
    pub image_popover_max_height: Px,
    pub popover_gap: Px,
    pub popover_pad: Px,
    pub code_max_height: Px,
    pub math_max_height: Px,
    pub mermaid_max_width: Px,
    pub mermaid_max_height: Px,
    pub rule_thickness: Px,
    pub table_line_thickness: Px,
    pub code_radius: f32,
    pub code_border: Px,
    pub well_lang_size: f32,
    pub well_lang_right: Px,
    pub well_lang_top: Px,
    pub well_lang: ThemeColor,
    pub placeholder_dash: Px,
    pub placeholder_gap: Px,
    pub placeholder_label_size: f32,
    pub quote_alert_label_dx: Px,
    pub alert_note: ThemeColor,
    pub alert_tip: ThemeColor,
    pub alert_important: ThemeColor,
    pub alert_warning: ThemeColor,
    pub alert_caution: ThemeColor,
}

impl DecorationTokens {
    pub fn fingerprint(self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut h);
        h.finish()
    }

    pub(crate) fn without_colors(self) -> Self {
        const FLAT: ThemeColor = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
        Self {
            well_lang: FLAT,
            alert_note: FLAT,
            alert_tip: FLAT,
            alert_important: FLAT,
            alert_warning: FLAT,
            alert_caution: FLAT,
            ..self
        }
    }

    pub(crate) fn from_palette(p: &Palette) -> Self {
        Self {
            list_marker_size: 5.0,
            task_size: 16.0,
            task_radius: 4.0,
            task_border: 1.5,
            task_gap: 10.0,
            list_gutter_min: 40.0,
            list_digit_width: 10.0,
            list_task_extra: 16.0,
            list_marker_gap: 8.0,
            list_nested_top: 8.0,
            image_placeholder_height: 150.0,
            image_max_width: 804.0,
            image_max_height: 720.0,
            image_popover_max_width: 320.0,
            image_popover_max_height: 240.0,
            popover_gap: 6.0,
            popover_pad: 4.0,
            code_max_height: 420.0,
            math_max_height: 420.0,
            mermaid_max_width: 804.0,
            mermaid_max_height: 420.0,
            rule_thickness: 1.0,
            table_line_thickness: 1.0,
            code_radius: 6.0,
            code_border: 1.0,
            well_lang_size: 10.5,
            well_lang_right: 10.0,
            well_lang_top: 6.0,
            well_lang: p.slate_500,
            placeholder_dash: 4.0,
            placeholder_gap: 3.0,
            placeholder_label_size: 13.0,
            quote_alert_label_dx: 12.0,
            alert_note: p.link,
            alert_tip: p.ok,
            alert_important: p.slate_900,
            alert_warning: p.warn,
            alert_caution: p.bad,
        }
    }

    pub fn well_max_height(self, kind: BlockKind, edit_source: bool) -> Option<Px> {
        if edit_source {
            return Some(self.code_max_height);
        }
        match kind {
            BlockKind::CodeBlock => Some(self.code_max_height),
            BlockKind::Math => Some(self.math_max_height),
            BlockKind::Mermaid => Some(self.mermaid_max_height),
            BlockKind::Image => Some(self.image_max_height),
            _ => None,
        }
    }
}
