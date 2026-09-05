use super::{BlockComponent, PaintOp};
use crate::snapshot::DecorationPiece;
use md_content::gpui_theme::ThemeColorExt;
use md_core::block::AlertKind;
use md_theme::DocumentTheme;

pub(crate) struct BlockQuoteBlock;

impl BlockComponent for BlockQuoteBlock {
    fn paint_decoration(&self, piece: &DecorationPiece, theme: &DocumentTheme) -> Vec<PaintOp> {
        let (x, y, bar, h) = piece.rect_device;
        let color = match piece.alert {
            Some(AlertKind::Note) => theme.decoration.alert_note.hsla(),
            Some(AlertKind::Tip) => theme.decoration.alert_tip.hsla(),
            Some(AlertKind::Important) => theme.decoration.alert_important.hsla(),
            Some(AlertKind::Warning) => theme.decoration.alert_warning.hsla(),
            Some(AlertKind::Caution) => theme.decoration.alert_caution.hsla(),
            None => theme.paint.quote_bar.hsla(),
        };
        vec![PaintOp::Fill {
            rect: (x, y, bar, h),
            color,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::BlockQuoteBlock;
    use crate::blocks::{BlockComponent, PaintOp};
    use crate::snapshot::DecorationPiece;
    use md_core::block::BlockKind;
    use md_layout::box_tree::BoxRole;
    use md_theme::DocumentTheme;

    #[test]
    fn quote_bar_uses_the_width_published_by_geometry() {
        let piece = DecorationPiece {
            rect_device: (1.0, 2.0, 3.5, 40.0),
            clip_device: None,
            kind: BlockKind::BlockQuote,
            role: BoxRole::Bar,
            hit_block: 0,
            gutter_dot: None,
            gutter_label: None,
            gutter_label_at: None,
            gutter_label_size: 0.0,
            list_nest: 0,
            task: None,
            alert: None,
        };
        let ops = BlockQuoteBlock.paint_decoration(&piece, &DocumentTheme::one_dark());
        assert!(matches!(
            ops.as_slice(),
            [PaintOp::Fill { rect, .. }] if *rect == piece.rect_device
        ));
    }
}
