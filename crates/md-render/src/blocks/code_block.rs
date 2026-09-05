use super::{BlockComponent, DecorationScope, PaintOp};
use crate::snapshot::DecorationPiece;
use md_content::gpui_theme::ThemeColorExt;
use md_core::Px;
use md_theme::DocumentTheme;

pub(crate) struct CodeBlock;

pub(crate) fn code_well_ops(rect: (Px, Px, Px, Px), theme: &DocumentTheme) -> Vec<PaintOp> {
    let fill = theme.paint.code_fill.hsla();
    let radius = theme.decoration.code_radius;
    let border_width = theme.decoration.code_border as f32;
    if border_width > 0.0 {
        vec![PaintOp::RoundBorder {
            rect,
            fill,
            radius,
            border_width,
            border_color: theme.paint.code_border.hsla(),
        }]
    } else if radius > 0.0 {
        vec![PaintOp::Round {
            rect,
            color: fill,
            radius,
        }]
    } else {
        vec![PaintOp::Fill { rect, color: fill }]
    }
}

impl BlockComponent for CodeBlock {
    fn decoration_scope(&self) -> DecorationScope {
        DecorationScope::Leaf
    }

    fn paint_decoration(&self, piece: &DecorationPiece, theme: &DocumentTheme) -> Vec<PaintOp> {
        code_well_ops(piece.rect_device, theme)
    }
}

pub(crate) struct WellBlock;

impl BlockComponent for WellBlock {
    fn decoration_scope(&self) -> DecorationScope {
        DecorationScope::Leaf
    }

    fn paint_decoration(&self, piece: &DecorationPiece, theme: &DocumentTheme) -> Vec<PaintOp> {
        code_well_ops(piece.rect_device, theme)
    }
}

#[cfg(test)]
mod tests {
    use super::CodeBlock;
    use crate::blocks::for_kind;
    use crate::blocks::{BlockComponent, DecorationScope, PaintOp};
    use crate::snapshot::DecorationPiece;
    use md_content::gpui_theme::ThemeColorExt;
    use md_core::block::BlockKind;
    use md_layout::box_tree::BoxRole;
    use md_theme::DocumentTheme;

    fn piece(kind: BlockKind) -> DecorationPiece {
        DecorationPiece {
            rect_device: (1.0, 2.0, 30.0, 40.0),
            clip_device: None,
            kind,
            role: BoxRole::Frame,
            hit_block: 0,
            gutter_dot: None,
            gutter_label: None,
            gutter_label_at: None,
            gutter_label_size: 0.0,
            list_nest: 0,
            task: None,
            alert: None,
        }
    }

    fn assert_v2_well(kind: BlockKind) {
        let theme = DocumentTheme::one_dark();
        let component = for_kind(kind);
        assert_eq!(
            component.decoration_scope(),
            DecorationScope::Leaf,
            "{kind:?}"
        );
        let ops = component.paint_decoration(&piece(kind), &theme);
        match ops.as_slice() {
            [
                PaintOp::RoundBorder {
                    rect,
                    fill,
                    radius,
                    border_width,
                    border_color,
                },
            ] => {
                assert_eq!(*rect, (1.0, 2.0, 30.0, 40.0), "{kind:?}");
                assert_eq!(*fill, theme.paint.code_fill.hsla(), "{kind:?}");
                assert_eq!(*radius, 6.0, "{kind:?}");
                assert_eq!(*border_width, 1.0, "{kind:?}");
                assert_eq!(*border_color, theme.paint.code_border.hsla(), "{kind:?}");
            }
            other => panic!("{kind:?} well ops: {other:?}"),
        }
    }

    #[test]
    fn code_math_mermaid_share_v2_well_chrome() {
        assert_v2_well(BlockKind::CodeBlock);
        assert_v2_well(BlockKind::Math);
        assert_v2_well(BlockKind::Mermaid);
    }

    #[test]
    fn well_chrome_follows_theme_tokens() {
        let mut theme = DocumentTheme::one_dark();
        theme.decoration.code_border = 0.0;
        theme.decoration.code_radius = 0.0;
        let ops = CodeBlock.paint_decoration(&piece(BlockKind::CodeBlock), &theme);
        match ops.as_slice() {
            [PaintOp::Fill { color, .. }] => {
                assert_eq!(*color, theme.paint.code_fill.hsla());
            }
            other => panic!("zero radius/border should fill: {other:?}"),
        }

        theme.decoration.code_radius = 4.0;
        let ops = CodeBlock.paint_decoration(&piece(BlockKind::CodeBlock), &theme);
        match ops.as_slice() {
            [PaintOp::Round { radius, color, .. }] => {
                assert_eq!(*radius, 4.0);
                assert_eq!(*color, theme.paint.code_fill.hsla());
            }
            other => panic!("radius without border should round-fill: {other:?}"),
        }
    }
}
