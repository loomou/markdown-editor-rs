use super::{BlockComponent, PaintOp};
use crate::snapshot::{CellPiece, DeviceRect};
use md_content::gpui_theme::ThemeColorExt;
use md_core::Px;
use md_core::block::BlockId;
use md_theme::DocumentTheme;

pub(crate) struct TableBlock;
pub(crate) struct TableRowBlock;
pub(crate) struct TableCellBlock;

impl BlockComponent for TableBlock {}

impl BlockComponent for TableRowBlock {}

impl BlockComponent for TableCellBlock {}

#[derive(Clone, Copy, Debug)]
pub struct CellRect {
    pub table: BlockId,
    pub rect: DeviceRect,
    pub is_header: bool,
}

impl CellRect {
    pub fn of(cell: &CellPiece) -> CellRect {
        CellRect {
            table: cell.table,
            rect: cell.rect_device,
            is_header: cell.header,
        }
    }
}

pub fn paint_cell_grid(cells: &[CellRect], theme: &DocumentTheme) -> Vec<PaintOp> {
    let grid = theme.paint.table_grid.hsla();
    let outer = theme.paint.table_border.hsla();
    let head_fill = theme.paint.table_head_fill.hsla();
    let t = theme.decoration.table_line_thickness;
    let same_edge = |a: Px, b: Px| (a - b).abs() < 0.75;
    let mut ops = Vec::new();
    for cell in cells {
        if cell.is_header {
            ops.push(PaintOp::Fill {
                rect: cell.rect,
                color: head_fill,
            });
        }
    }
    for cell in cells {
        let (x, y, w, h) = cell.rect;
        let neighbour = |pred: &dyn Fn(Px, Px, Px, Px) -> bool| {
            cells.iter().any(|o| {
                if o.table != cell.table {
                    return false;
                }
                let (ox, oy, ow, oh) = o.rect;
                pred(ox, oy, ow, oh)
            })
        };
        let has_right = neighbour(&|ox, oy, _, _| same_edge(oy, y) && same_edge(ox, x + w));
        let has_below = neighbour(&|ox, oy, _, _| same_edge(ox, x) && same_edge(oy, y + h));
        let has_left = neighbour(&|ox, oy, ow, _| same_edge(oy, y) && same_edge(ox + ow, x));
        let has_above = neighbour(&|ox, oy, _, oh| same_edge(ox, x) && same_edge(oy + oh, y));
        let (top_t, top_color) = if has_above { (t, grid) } else { (t, outer) };
        ops.push(PaintOp::Fill {
            rect: (x, y, w, top_t),
            color: top_color,
        });
        ops.push(PaintOp::Fill {
            rect: (x, y, t, h),
            color: if has_left { grid } else { outer },
        });
        if !has_right {
            ops.push(PaintOp::Fill {
                rect: (x + w - t, y, t, h),
                color: outer,
            });
        }
        if !has_below {
            ops.push(PaintOp::Fill {
                rect: (x, y + h - t, w, t),
                color: outer,
            });
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::{CellRect, paint_cell_grid};
    use crate::blocks::PaintOp;
    use crate::snapshot::DeviceRect;
    use md_content::gpui_theme::ThemeColorExt;
    use md_theme::DocumentTheme;

    #[test]
    fn header_cells_get_head_fill_and_thin_body_rule() {
        let theme = DocumentTheme::one_dark();
        let cell = |rect: DeviceRect, is_header: bool| CellRect {
            table: 1,
            rect,
            is_header,
        };
        let ops = paint_cell_grid(
            &[
                cell((0.0, 0.0, 40.0, 20.0), true),
                cell((40.0, 0.0, 40.0, 20.0), true),
                cell((0.0, 20.0, 40.0, 20.0), false),
                cell((40.0, 20.0, 40.0, 20.0), false),
            ],
            &theme,
        );
        let head_fill = theme.paint.table_head_fill.hsla();
        let t = theme.decoration.table_line_thickness;

        for rect in [(0.0, 0.0, 40.0, 20.0), (40.0, 0.0, 40.0, 20.0)] {
            assert!(
                ops.iter().any(|op| {
                    matches!(
                        op,
                        PaintOp::Fill { rect: r, color }
                            if *color == head_fill
                                && (r.0 - rect.0).abs() < 0.01
                                && (r.1 - rect.1).abs() < 0.01
                                && (r.2 - rect.2).abs() < 0.01
                                && (r.3 - rect.3).abs() < 0.01
                    )
                }),
                "header fill for {rect:?}: {ops:?}"
            );
        }

        assert!(
            !ops.iter().any(|op| {
                matches!(
                    op,
                    PaintOp::Fill { rect, .. }
                        if rect.2 > t + 0.01
                            && (rect.1 - 20.0).abs() < 0.01
                            && rect.3 > t + 0.01
                )
            }),
            "no thick header/body rule: {ops:?}"
        );
    }

    #[test]
    fn neighbour_detection_does_not_cross_table_boundaries() {
        let theme = DocumentTheme::one_dark();
        let ops = paint_cell_grid(
            &[
                CellRect {
                    table: 1,
                    rect: (0.0, 0.0, 40.0, 20.0),
                    is_header: false,
                },
                CellRect {
                    table: 2,
                    rect: (40.0, 0.0, 40.0, 20.0),
                    is_header: false,
                },
            ],
            &theme,
        );
        let outer = theme.paint.table_border.hsla();
        let t = theme.decoration.table_line_thickness;
        assert!(ops.iter().any(|op| {
            matches!(
                op,
                PaintOp::Fill { rect, color }
                    if *color == outer
                        && (rect.0 - (40.0 - t)).abs() < 0.01
                        && rect.1 == 0.0
                        && rect.2 == t
                        && rect.3 == 20.0
            )
        }));
    }
}
