use super::{BlockComponent, DecorationScope, PaintOp};
use crate::snapshot::DecorationPiece;
use md_content::gpui_theme::ThemeColorExt;
use md_theme::DocumentTheme;

pub(crate) struct ThematicBreakBlock;

impl BlockComponent for ThematicBreakBlock {
    fn decoration_scope(&self) -> DecorationScope {
        DecorationScope::Leaf
    }

    fn paint_decoration(&self, piece: &DecorationPiece, theme: &DocumentTheme) -> Vec<PaintOp> {
        let (x, y, w, h) = piece.rect_device;
        let mid = y + (h * 0.5).max(0.0);
        vec![PaintOp::Fill {
            rect: (x, mid, w.max(1.0), theme.decoration.rule_thickness),
            color: theme.paint.rule.hsla(),
        }]
    }
}
