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
    let adjacency = cell_adjacency(cells);
    let step_neighbour = |runs: &[Vec<usize>],
                          pos: (u32, u32),
                          forward: bool,
                          pred: &dyn Fn(&CellRect) -> bool|
     -> Option<bool> {
        let run = &runs[pos.0 as usize];
        let at = if forward {
            pos.1 as usize + 1
        } else {
            pos.1.checked_sub(1)? as usize
        };
        run.get(at).map(|&o| pred(&cells[o]))
    };
    for (ci, cell) in cells.iter().enumerate() {
        let (x, y, w, h) = cell.rect;
        let has_right = step_neighbour(&adjacency.rows, adjacency.row_pos[ci], true, &|o| {
            same_edge(o.rect.0, x + w)
        });
        let has_below = step_neighbour(&adjacency.cols, adjacency.col_pos[ci], true, &|o| {
            same_edge(o.rect.1, y + h)
        });
        let has_left = step_neighbour(&adjacency.rows, adjacency.row_pos[ci], false, &|o| {
            same_edge(o.rect.0 + o.rect.2, x)
        });
        let has_above = step_neighbour(&adjacency.cols, adjacency.col_pos[ci], false, &|o| {
            same_edge(o.rect.1 + o.rect.3, y)
        });
        let (top_t, top_color) = if has_above == Some(true) {
            (t, grid)
        } else {
            (t, outer)
        };
        ops.push(PaintOp::Fill {
            rect: (x, y, w, top_t),
            color: top_color,
        });
        ops.push(PaintOp::Fill {
            rect: (x, y, t, h),
            color: if has_left == Some(true) { grid } else { outer },
        });
        if has_right != Some(true) {
            ops.push(PaintOp::Fill {
                rect: (x + w - t, y, t, h),
                color: outer,
            });
        }
        if has_below != Some(true) {
            ops.push(PaintOp::Fill {
                rect: (x, y + h - t, w, t),
                color: outer,
            });
        }
    }
    ops
}

struct CellAdjacency {
    rows: Vec<Vec<usize>>,
    cols: Vec<Vec<usize>>,
    row_pos: Vec<(u32, u32)>,
    col_pos: Vec<(u32, u32)>,
}

fn cell_adjacency(cells: &[CellRect]) -> CellAdjacency {
    let same_edge = |a: Px, b: Px| (a - b).abs() < 0.75;
    let runs = |axis: &dyn Fn(&CellRect) -> (BlockId, f64, f64)| {
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by(|&a, &b| {
            let (ka, ma, na) = axis(&cells[a]);
            let (kb, mb, nb) = axis(&cells[b]);
            ka.cmp(&kb)
                .then_with(|| ma.total_cmp(&mb))
                .then_with(|| na.total_cmp(&nb))
        });
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut pos = vec![(0u32, 0u32); cells.len()];
        let mut i = 0;
        while i < order.len() {
            let (table, main_val, _) = axis(&cells[order[i]]);
            let mut run = Vec::new();
            while i < order.len() {
                let (t, m, _) = axis(&cells[order[i]]);
                if t != table || !same_edge(m, main_val) {
                    break;
                }
                run.push(order[i]);
                i += 1;
            }
            run.sort_by(|&a, &b| axis(&cells[a]).2.total_cmp(&axis(&cells[b]).2));
            for (p, &ci) in run.iter().enumerate() {
                pos[ci] = (runs.len() as u32, p as u32);
            }
            runs.push(run);
        }
        (runs, pos)
    };
    let (rows, row_pos) = runs(&|c: &CellRect| (c.table, c.rect.1, c.rect.0));
    let (cols, col_pos) = runs(&|c: &CellRect| (c.table, c.rect.0, c.rect.1));
    CellAdjacency {
        rows,
        cols,
        row_pos,
        col_pos,
    }
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

    #[test]
    fn full_grid_emits_two_edges_per_cell_plus_outer_frame() {
        let theme = DocumentTheme::one_dark();
        let (rows, cols) = (6usize, 8usize);
        let cells: Vec<_> = (0..rows * cols)
            .map(|i| CellRect {
                table: 1,
                rect: (
                    ((i % cols) * 30) as f64,
                    ((i / cols) * 24) as f64,
                    30.0,
                    24.0,
                ),
                is_header: i < cols,
            })
            .collect();
        let ops = paint_cell_grid(&cells, &theme);
        let expected = cols + 2 * rows * cols + cols + rows;
        assert_eq!(ops.len(), expected, "{ops:?}");
    }

    #[test]
    fn adjacency_matches_full_scan() {
        let theme = DocumentTheme::one_dark();
        let mut state = 0x2545F491u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..120 {
            let rows = 1 + (next() % 5) as usize;
            let cols = 1 + (next() % 6) as usize;
            let col_w: Vec<f64> = (0..cols).map(|_| 18.0 + (next() % 30) as f64).collect();
            let row_h: Vec<f64> = (0..rows).map(|_| 16.0 + (next() % 20) as f64).collect();
            let mut cells = Vec::new();
            let mut y = 0.0;
            for (r, &h) in row_h.iter().enumerate() {
                let mut x = 0.0;
                for (c, &w) in col_w.iter().enumerate() {
                    if next() % 4 != 0 {
                        cells.push(CellRect {
                            table: 1,
                            rect: (x, y, w, h),
                            is_header: r == 0 && c % 3 == 0,
                        });
                    }
                    x += w;
                }
                y += h;
            }
            let got = paint_cell_grid(&cells, &theme);
            let want = paint_cell_grid_naive(&cells, &theme);
            let flat = |ops: &[PaintOp]| {
                ops.iter()
                    .map(|op| match *op {
                        PaintOp::Fill { rect, color } => (rect, color),
                        _ => panic!("grid ops are fills only"),
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                flat(&got),
                flat(&want),
                "cells={:?}",
                cells.iter().map(|c| c.rect).collect::<Vec<_>>()
            );
        }
    }

    fn paint_cell_grid_naive(cells: &[CellRect], theme: &DocumentTheme) -> Vec<PaintOp> {
        let grid = theme.paint.table_grid.hsla();
        let outer = theme.paint.table_border.hsla();
        let head_fill = theme.paint.table_head_fill.hsla();
        let t = theme.decoration.table_line_thickness;
        let same_edge = |a: f64, b: f64| (a - b).abs() < 0.75;
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
            let neighbour = |pred: &dyn Fn(f64, f64, f64, f64) -> bool| {
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
            ops.push(PaintOp::Fill {
                rect: (x, y, w, t),
                color: if has_above { grid } else { outer },
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
}
