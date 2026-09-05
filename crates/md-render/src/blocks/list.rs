use super::{BlockComponent, PaintOp};
use crate::snapshot::DecorationPiece;
use gpui::Hsla;
use md_content::gpui_theme::ThemeColorExt;
use md_theme::DocumentTheme;

pub(crate) struct ListBlock;
pub(crate) struct ListItemBlock;

impl BlockComponent for ListBlock {}

impl BlockComponent for ListItemBlock {
    fn paint_decoration(&self, piece: &DecorationPiece, theme: &DocumentTheme) -> Vec<PaintOp> {
        let color = theme.paint.list_marker.hsla();
        let canvas = theme.paint.canvas.hsla();
        let Some((dx, dy)) = piece.gutter_dot else {
            return Vec::new();
        };
        let d = &theme.decoration;
        if let Some(checked) = piece.task {
            let size = d.task_size;
            let radius = d.task_radius;
            let border_width = d.task_border as f32;
            let accent = theme.paint.task_checked.hsla();
            let (fill, border) = if checked {
                (accent, accent)
            } else {
                (
                    Hsla {
                        h: 0.0,
                        s: 0.0,
                        l: 0.0,
                        a: 0.0,
                    },
                    theme.paint.task_border.hsla(),
                )
            };
            let mut ops = vec![PaintOp::RoundBorder {
                rect: (dx, dy, size, size),
                fill,
                radius,
                border_width,
                border_color: border,
            }];
            if checked {
                ops.push(PaintOp::Check {
                    origin: (dx, dy),
                    size,
                    color: theme.paint.task_check.hsla(),
                });
            }
            return ops;
        }
        if piece.gutter_label.is_some() {
            return Vec::new();
        }
        let size = d.list_marker_size;
        match piece.list_nest % 3 {
            1 => {
                let inner = (size - 2.0).max(1.0);
                vec![
                    PaintOp::Round {
                        rect: (dx, dy, size, size),
                        color,
                        radius: (size * 0.5) as f32,
                    },
                    PaintOp::Round {
                        rect: (dx + 1.0, dy + 1.0, inner, inner),
                        color: canvas,
                        radius: (inner * 0.5) as f32,
                    },
                ]
            }
            2 => vec![PaintOp::Fill {
                rect: (dx, dy, size, size),
                color,
            }],
            _ => vec![PaintOp::Round {
                rect: (dx, dy, size, size),
                color,
                radius: (size * 0.5) as f32,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ListItemBlock;
    use crate::blocks::{BlockComponent, PaintOp};
    use crate::snapshot::DecorationPiece;
    use md_content::gpui_theme::ThemeColorExt;
    use md_core::block::BlockKind;
    use md_layout::box_tree::BoxRole;
    use md_theme::DocumentTheme;

    fn piece() -> DecorationPiece {
        DecorationPiece {
            rect_device: (0.0, 0.0, 20.0, 20.0),
            clip_device: None,
            kind: BlockKind::ListItem,
            role: BoxRole::Slot,
            hit_block: 0,
            gutter_dot: Some((0.0, 0.0)),
            gutter_label: None,
            gutter_label_at: None,
            gutter_label_size: 16.0,
            list_nest: 0,
            task: None,
            alert: None,
        }
    }

    #[test]
    fn ordered_task_paints_checkbox() {
        let mut p = piece();
        p.gutter_label = Some("1.".into());
        p.task = Some(false);
        let ops = ListItemBlock.paint_decoration(&p, &DocumentTheme::formal());
        assert!(!ops.is_empty());
    }

    #[test]
    fn ordered_only_skips_bullet() {
        let mut p = piece();
        p.gutter_label = Some("1.".into());
        let ops = ListItemBlock.paint_decoration(&p, &DocumentTheme::formal());
        assert!(ops.is_empty());
    }

    #[test]
    fn v2_task_checkbox_is_round_border_and_check() {
        let theme = DocumentTheme::one_dark();
        let mut open = piece();
        open.task = Some(false);
        let ops = ListItemBlock.paint_decoration(&open, &theme);
        match ops.as_slice() {
            [
                PaintOp::RoundBorder {
                    radius,
                    border_width,
                    border_color,
                    fill,
                    ..
                },
            ] => {
                assert!((*radius - theme.decoration.task_radius).abs() < f32::EPSILON);
                assert!((*border_width - theme.decoration.task_border as f32).abs() < f32::EPSILON);
                assert_eq!(*border_color, theme.paint.task_border.hsla());
                assert_eq!(fill.a, 0.0);
            }
            other => panic!("open task: {other:?}"),
        }

        let mut done = piece();
        done.task = Some(true);
        let ops = ListItemBlock.paint_decoration(&done, &theme);
        match ops.as_slice() {
            [
                PaintOp::RoundBorder {
                    fill,
                    border_color,
                    radius,
                    ..
                },
                PaintOp::Check { color, .. },
            ] => {
                assert_eq!(*fill, theme.paint.task_checked.hsla());
                assert_eq!(*border_color, theme.paint.task_checked.hsla());
                assert!((*radius - 4.0).abs() < f32::EPSILON);
                assert_eq!(*color, theme.paint.task_check.hsla());
            }
            other => panic!("done task: {other:?}"),
        }
    }
}
