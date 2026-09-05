use crate::gpui_theme::{ThemeColorExt, TypeRoleExt};
use gpui::{App, Font, Hsla, Pixels, px};
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::box_tree::TypeSlot;
use md_theme::TypeRole;

pub fn snapped_row_advance(line_height: Px, scale: f64) -> Px {
    let snapped = (line_height * scale).round() / scale;
    if snapped > 0.0 { snapped } else { line_height }
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedType {
    pub(super) font: Font,
    pub(super) font_size: Pixels,
    pub(super) color: Hsla,
    pub(super) row_advance: Px,
    pub(super) ascent: Px,
    pub(super) descent: Px,
    pub(super) style_fp: u64,
    pub(super) strikethrough: Option<Hsla>,
}

pub(crate) fn ink_height(ascent: Px, descent: Px) -> Px {
    (ascent.abs() + descent.abs()).max(1.0)
}

pub(crate) fn ink_top_in_line(row_advance: Px, ascent: Px, descent: Px) -> Px {
    (row_advance - ascent.abs() - descent.abs()) / 2.0
}

pub(crate) fn text_baseline(row_advance: Px, ascent: Px, descent: Px) -> Px {
    ink_top_in_line(row_advance, ascent, descent) + ascent.abs()
}

impl ResolvedType {
    pub(super) fn ink_height(&self) -> Px {
        ink_height(self.ascent, self.descent)
    }

    pub(super) fn text_baseline(&self) -> Px {
        text_baseline(self.row_advance, self.ascent, self.descent)
    }

    pub(super) fn ink_top_in_line(&self) -> Px {
        ink_top_in_line(self.row_advance, self.ascent, self.descent)
    }
}

#[derive(Debug)]
pub(super) struct ResolvedTypes {
    pub(super) body: ResolvedType,
    pub(super) headings: [ResolvedType; 6],
    pub(super) code: ResolvedType,
    pub(super) quote: ResolvedType,
    pub(super) table: ResolvedType,
    pub(super) table_header: ResolvedType,
    pub(super) image: ResolvedType,
    pub(super) footnote: ResolvedType,
    pub(super) task_done: ResolvedType,
}

impl ResolvedTypes {
    pub(super) fn for_kind(&self, kind: BlockKind) -> &ResolvedType {
        match kind {
            BlockKind::Heading(n) => &self.headings[n.clamp(1, 6) as usize - 1],
            BlockKind::CodeBlock | BlockKind::MetadataBlock => &self.code,
            BlockKind::BlockQuote => &self.quote,
            BlockKind::TableCell => &self.table,
            BlockKind::Image => &self.image,
            BlockKind::FootnoteDefinition => &self.footnote,
            BlockKind::DocRoot
            | BlockKind::DocStart
            | BlockKind::Paragraph
            | BlockKind::List
            | BlockKind::ListItem
            | BlockKind::Table
            | BlockKind::TableRow
            | BlockKind::ThematicBreak
            | BlockKind::Mermaid
            | BlockKind::Math => &self.body,
        }
    }

    pub(super) fn for_slot(&self, kind: BlockKind, slot: TypeSlot) -> &ResolvedType {
        match slot {
            TypeSlot::FromKind => self.for_kind(kind),
            TypeSlot::Quote => &self.quote,
            TypeSlot::TableHeader => &self.table_header,
            TypeSlot::Footnote => &self.footnote,
            TypeSlot::TaskDone => &self.task_done,
        }
    }
}

pub(super) fn resolve_type(cx: &App, role: TypeRole, scale: f64) -> ResolvedType {
    let font = role.font();
    let font_size = px(role.size_px);
    let font_id = cx.text_system().resolve_font(&font);
    let ascent: Px = f32::from(cx.text_system().ascent(font_id, font_size)) as Px;
    let descent: Px = f32::from(cx.text_system().descent(font_id, font_size)) as Px;
    let metrics = (ascent - descent).abs();
    let desired = role.size_px as Px * role.line_height_em as Px;
    let line_height = if desired > 0.0 { desired } else { metrics };
    let row_advance = snapped_row_advance(
        if line_height > 0.0 {
            line_height
        } else {
            role.size_px as Px
        },
        scale,
    );
    ResolvedType {
        font,
        font_size,
        color: role.color.hsla(),
        row_advance,
        ascent,
        descent,
        style_fp: role.fingerprint(),
        strikethrough: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ink_height, ink_top_in_line, text_baseline};

    #[test]
    fn ink_box_centers_in_the_line_box() {
        assert_eq!(ink_height(16.0, 4.0), 20.0);
        assert_eq!(ink_top_in_line(28.0, 16.0, 4.0), 4.0);
        assert_eq!(text_baseline(28.0, 16.0, 4.0), 20.0);
    }

    #[test]
    fn ink_box_uses_abs_metrics_and_keeps_negative_leading() {
        assert_eq!(ink_height(16.0, -4.0), 20.0);
        assert_eq!(ink_top_in_line(20.0, 16.0, 8.0), -2.0);
        assert_eq!(text_baseline(20.0, 16.0, 8.0), 14.0);
    }
}
