use super::CursorMotion;
use super::EditorView;
use super::table_cols::CellHit;
use gpui::{Context, Window};
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::{Cursor, Doc};
use md_core::document::TableOp;

const GRIP: Px = 20.0;
const ROW_GUTTER: Px = 27.0;
const COL_GUTTER: Px = 18.0;
const COL_GRIP_LIFT: Px = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GripHover {
    pub table: BlockId,
    pub row: Option<usize>,
    pub col: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReorderAxis {
    Row,
    Col,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReorderDrag {
    pub table: BlockId,
    pub axis: ReorderAxis,
    pub from: usize,
    pub cell: BlockId,
    pub grab: (Px, Px),
    pub pointer: (Px, Px),
    pub dest: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GripHit {
    pub table: BlockId,
    pub axis: ReorderAxis,
    pub from: usize,
    pub cell: BlockId,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum DropGuide {
    H { y: Px, x0: Px, x1: Px },
    V { x: Px, y0: Px, y1: Px },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CloneGuide {
    pub x: Px,
    pub y: Px,
    pub w: Px,
    pub h: Px,
    pub row: bool,
    pub dx: Px,
    pub dy: Px,
    pub cap: Px,
    pub table: BlockId,
    pub from: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReorderVisual {
    pub row_grip: Option<(Px, Px)>,
    pub col_grip: Option<(Px, Px)>,
    pub ghost: Option<(Px, Px, Px, Px)>,
    pub drop: Option<DropGuide>,
    pub clone: Option<CloneGuide>,
}

struct TableBands {
    rows_n: usize,
    cols_n: usize,
    x0: Px,
    y0: Px,
    x1: Px,
    y1: Px,
    row_y0: Vec<Option<Px>>,
    row_y1: Vec<Option<Px>>,
    col_x0: Vec<Option<Px>>,
    col_x1: Vec<Option<Px>>,
    row_cell: Vec<Option<BlockId>>,
    col_cell: Vec<Option<BlockId>>,
}

fn collect_bands(doc: &Doc, cells: &[CellHit], table: BlockId) -> Option<TableBands> {
    let mut rows_n = 0;
    let mut cols_n = 0;
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    let mut any = false;
    for &(block, (x, y, w, h)) in cells {
        let Some(loc) = doc.table_loc(block) else {
            continue;
        };
        if loc.table != table {
            continue;
        }
        rows_n = loc.rows.max(rows_n);
        cols_n = loc.cols.max(cols_n);
        any = true;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    if !any || rows_n == 0 || cols_n == 0 {
        return None;
    }
    let mut row_y0 = vec![None; rows_n];
    let mut row_y1 = vec![None; rows_n];
    let mut col_x0 = vec![None; cols_n];
    let mut col_x1 = vec![None; cols_n];
    let mut row_cell = vec![None; rows_n];
    let mut col_cell = vec![None; cols_n];
    for &(block, (x, y, w, h)) in cells {
        let Some(loc) = doc.table_loc(block) else {
            continue;
        };
        if loc.table != table {
            continue;
        }
        if loc.row < rows_n {
            row_y0[loc.row] = Some(row_y0[loc.row].map_or(y, |p: Px| p.min(y)));
            row_y1[loc.row] = Some(row_y1[loc.row].map_or(y + h, |p: Px| p.max(y + h)));
            if row_cell[loc.row].is_none() {
                row_cell[loc.row] = Some(block);
            }
        }
        if loc.col < cols_n {
            col_x0[loc.col] = Some(col_x0[loc.col].map_or(x, |p: Px| p.min(x)));
            col_x1[loc.col] = Some(col_x1[loc.col].map_or(x + w, |p: Px| p.max(x + w)));
            if col_cell[loc.col].is_none() {
                col_cell[loc.col] = Some(block);
            }
        }
    }
    Some(TableBands {
        rows_n,
        cols_n,
        x0,
        y0,
        x1,
        y1,
        row_y0,
        row_y1,
        col_x0,
        col_x1,
        row_cell,
        col_cell,
    })
}

fn in_rect(pos: (Px, Px), x: Px, y: Px, w: Px, h: Px) -> bool {
    pos.0 >= x && pos.0 < x + w && pos.1 >= y && pos.1 < y + h
}

fn row_grip_xy(bands: &TableBands, row: usize) -> Option<(Px, Px)> {
    let y0 = bands.row_y0[row]?;
    let y1 = bands.row_y1[row]?;
    Some((bands.x0 - ROW_GUTTER, (y0 + y1 - GRIP) * 0.5))
}

fn col_grip_xy(bands: &TableBands, col: usize) -> Option<(Px, Px)> {
    let x0 = bands.col_x0[col]?;
    let x1 = bands.col_x1[col]?;
    Some(((x0 + x1 - GRIP) * 0.5, bands.y0 - COL_GRIP_LIFT))
}

fn row_band(bands: &TableBands, row: usize) -> Option<(Px, Px)> {
    Some((bands.row_y0[row]?, bands.row_y1[row]?))
}

fn col_band(bands: &TableBands, col: usize) -> Option<(Px, Px)> {
    Some((bands.col_x0[col]?, bands.col_x1[col]?))
}

fn target_index(pos: Px, bands: &[(usize, Px, Px)], count: usize) -> usize {
    if count == 0 || bands.is_empty() {
        return 0;
    }
    if pos < bands[0].1 {
        return 0;
    }
    for &(i, _, b) in bands {
        if pos < b {
            return i;
        }
    }
    count - 1
}

fn packed_rows(bands: &TableBands) -> Vec<(usize, Px, Px)> {
    (0..bands.rows_n)
        .filter_map(|i| row_band(bands, i).map(|(a, b)| (i, a, b)))
        .collect()
}

fn packed_cols(bands: &TableBands) -> Vec<(usize, Px, Px)> {
    (0..bands.cols_n)
        .filter_map(|i| col_band(bands, i).map(|(a, b)| (i, a, b)))
        .collect()
}

pub(super) fn hover_grips(
    doc: &Doc,
    cells: &[CellHit],
    caret: BlockId,
    pos: (Px, Px),
) -> Option<GripHover> {
    let loc = doc.table_loc(caret)?;
    let bands = collect_bands(doc, cells, loc.table)?;
    let in_pad = in_rect(
        pos,
        bands.x0 - ROW_GUTTER,
        bands.y0 - COL_GRIP_LIFT,
        (bands.x1 - bands.x0) + ROW_GUTTER,
        (bands.y1 - bands.y0) + COL_GRIP_LIFT,
    );
    if !in_pad {
        return None;
    }
    let first_col_x1 = col_band(&bands, 0).map_or(bands.x0, |(_, x1)| x1);
    let first_row_y1 = row_band(&bands, 0).map_or(bands.y0, |(_, y1)| y1);
    let mut row = None;
    if bands.rows_n >= 2 {
        for i in 0..bands.rows_n {
            let Some((y0, y1)) = row_band(&bands, i) else {
                continue;
            };
            let grip = row_grip_xy(&bands, i);
            let in_row =
                pos.1 >= y0 && pos.1 < y1 && pos.0 >= bands.x0 - ROW_GUTTER && pos.0 < first_col_x1;
            let in_grip = grip.is_some_and(|(gx, gy)| in_rect(pos, gx, gy, GRIP, GRIP));
            if in_row || in_grip {
                row = Some(i);
                break;
            }
        }
    }
    let mut col = None;
    if bands.cols_n >= 2 {
        for i in 0..bands.cols_n {
            let Some((x0, x1)) = col_band(&bands, i) else {
                continue;
            };
            let grip = col_grip_xy(&bands, i);
            let in_col = pos.0 >= x0
                && pos.0 < x1
                && pos.1 >= bands.y0 - COL_GRIP_LIFT
                && pos.1 < first_row_y1;
            let in_grip = grip.is_some_and(|(gx, gy)| in_rect(pos, gx, gy, GRIP, GRIP));
            if in_col || in_grip {
                col = Some(i);
                break;
            }
        }
    }
    if row.is_none() && col.is_none() {
        return None;
    }
    Some(GripHover {
        table: loc.table,
        row,
        col,
    })
}

pub(super) fn hit_grip(
    doc: &Doc,
    cells: &[CellHit],
    caret: BlockId,
    pos: (Px, Px),
) -> Option<GripHit> {
    let loc = doc.table_loc(caret)?;
    let bands = collect_bands(doc, cells, loc.table)?;
    let mut best: Option<(Px, GripHit)> = None;
    if bands.rows_n >= 2 {
        for i in 0..bands.rows_n {
            let Some((gx, gy)) = row_grip_xy(&bands, i) else {
                continue;
            };
            if !in_rect(pos, gx, gy, GRIP, GRIP) {
                continue;
            }
            let Some(cell) = bands.row_cell[i] else {
                continue;
            };
            let d = (pos.0 - (gx + GRIP * 0.5)).abs() + (pos.1 - (gy + GRIP * 0.5)).abs();
            let hit = GripHit {
                table: loc.table,
                axis: ReorderAxis::Row,
                from: i,
                cell: if loc.row == i { caret } else { cell },
            };
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, hit));
            }
        }
    }
    if bands.cols_n >= 2 {
        for i in 0..bands.cols_n {
            let Some((gx, gy)) = col_grip_xy(&bands, i) else {
                continue;
            };
            if !in_rect(pos, gx, gy, GRIP, GRIP) {
                continue;
            }
            let Some(cell) = bands.col_cell[i] else {
                continue;
            };
            let d = (pos.0 - (gx + GRIP * 0.5)).abs() + (pos.1 - (gy + GRIP * 0.5)).abs();
            let hit = GripHit {
                table: loc.table,
                axis: ReorderAxis::Col,
                from: i,
                cell: if loc.col == i { caret } else { cell },
            };
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, hit));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

pub(super) fn reorder_visual(
    doc: &Doc,
    cells: &[CellHit],
    hover: Option<GripHover>,
    drag: Option<ReorderDrag>,
    picker_open: bool,
    col_resize: bool,
) -> Option<ReorderVisual> {
    if picker_open || col_resize {
        return None;
    }
    if let Some(drag) = drag {
        let bands = collect_bands(doc, cells, drag.table)?;
        return Some(drag_visual(&bands, drag));
    }
    let hover = hover?;
    let bands = collect_bands(doc, cells, hover.table)?;
    let row_grip = hover
        .row
        .filter(|_| bands.rows_n >= 2)
        .and_then(|r| row_grip_xy(&bands, r));
    let col_grip = hover
        .col
        .filter(|_| bands.cols_n >= 2)
        .and_then(|c| col_grip_xy(&bands, c));
    if row_grip.is_none() && col_grip.is_none() {
        return None;
    }
    Some(ReorderVisual {
        row_grip,
        col_grip,
        ghost: None,
        drop: None,
        clone: None,
    })
}

fn drag_visual(bands: &TableBands, drag: ReorderDrag) -> ReorderVisual {
    match drag.axis {
        ReorderAxis::Row => {
            let (y0, y1) = row_band(bands, drag.from).unwrap_or((bands.y0, bands.y0));
            let h = (y1 - y0).max(1.0);
            let w = (bands.x1 - bands.x0).max(1.0);
            let clone_x = bands.x0 - ROW_GUTTER;
            let clone_y = drag.pointer.1 - drag.grab.1;
            let rows = packed_rows(bands);
            let drop = drop_at_dest(drag.from, drag.dest, &rows, bands.y0, bands.y1).map(|y| {
                DropGuide::H {
                    y,
                    x0: bands.x0,
                    x1: bands.x1,
                }
            });
            ReorderVisual {
                row_grip: None,
                col_grip: None,
                ghost: Some((bands.x0, y0, w, h)),
                drop,
                clone: Some(CloneGuide {
                    x: clone_x,
                    y: clone_y,
                    w: w + ROW_GUTTER,
                    h,
                    row: true,
                    dx: 0.0,
                    dy: clone_y - y0,
                    cap: ROW_GUTTER,
                    table: drag.table,
                    from: drag.from,
                }),
            }
        }
        ReorderAxis::Col => {
            let (x0, x1) = col_band(bands, drag.from).unwrap_or((bands.x0, bands.x0));
            let w = (x1 - x0).max(1.0);
            let h = (bands.y1 - bands.y0).max(1.0);
            let clone_x = drag.pointer.0 - drag.grab.0;
            let clone_y = bands.y0 - COL_GUTTER;
            let cols = packed_cols(bands);
            let drop = drop_at_dest(drag.from, drag.dest, &cols, bands.x0, bands.x1).map(|x| {
                DropGuide::V {
                    x,
                    y0: bands.y0,
                    y1: bands.y1,
                }
            });
            ReorderVisual {
                row_grip: None,
                col_grip: None,
                ghost: Some((x0, bands.y0, w, h)),
                drop,
                clone: Some(CloneGuide {
                    x: clone_x,
                    y: clone_y,
                    w,
                    h: h + COL_GUTTER,
                    row: false,
                    dx: clone_x - x0,
                    dy: 0.0,
                    cap: COL_GUTTER,
                    table: drag.table,
                    from: drag.from,
                }),
            }
        }
    }
}

fn drop_at_dest(
    from: usize,
    dest: usize,
    bands: &[(usize, Px, Px)],
    start: Px,
    end: Px,
) -> Option<Px> {
    if dest == from {
        return None;
    }
    let band = bands.iter().find(|(i, _, _)| *i == dest);
    if dest < from {
        Some(band.map(|(_, a, _)| *a).unwrap_or(start))
    } else {
        Some(band.map(|(_, _, b)| *b).unwrap_or(end))
    }
}

fn grab_for(bands: &TableBands, hit: GripHit, pointer: (Px, Px)) -> (Px, Px) {
    match hit.axis {
        ReorderAxis::Row => {
            let y0 = bands
                .row_y0
                .get(hit.from)
                .copied()
                .flatten()
                .unwrap_or(pointer.1);
            (pointer.0 - (bands.x0 - ROW_GUTTER), pointer.1 - y0)
        }
        ReorderAxis::Col => {
            let x0 = bands
                .col_x0
                .get(hit.from)
                .copied()
                .flatten()
                .unwrap_or(pointer.0);
            (pointer.0 - x0, pointer.1 - (bands.y0 - COL_GUTTER))
        }
    }
}

fn dest_for(bands: &TableBands, axis: ReorderAxis, pointer: (Px, Px)) -> usize {
    match axis {
        ReorderAxis::Row => target_index(pointer.1, &packed_rows(bands), bands.rows_n),
        ReorderAxis::Col => target_index(pointer.0, &packed_cols(bands), bands.cols_n),
    }
}

fn menu_grip_axes(op: Option<TableOp>) -> (bool, bool) {
    match op {
        Some(
            TableOp::InsertRowAbove
            | TableOp::InsertRowBelow
            | TableOp::MoveRowUp
            | TableOp::MoveRowDown
            | TableOp::DeleteRow,
        ) => (true, false),
        Some(
            TableOp::InsertColumnLeft
            | TableOp::InsertColumnRight
            | TableOp::MoveColumnLeft
            | TableOp::MoveColumnRight
            | TableOp::DeleteColumn,
        ) => (false, true),
        _ => (true, true),
    }
}

fn caret_menu_grips(doc: &Doc, caret: BlockId, op: Option<TableOp>) -> Option<GripHover> {
    let loc = doc.table_loc(caret)?;
    let (want_row, want_col) = menu_grip_axes(op);
    let row = (want_row && loc.rows >= 2 && loc.col == 0).then_some(loc.row);
    let col = (want_col && loc.cols >= 2 && loc.row == 0).then_some(loc.col);
    if row.is_none() && col.is_none() {
        return None;
    }
    Some(GripHover {
        table: loc.table,
        row,
        col,
    })
}

impl EditorView {
    fn menu_driving_grips(&self) -> bool {
        self.table_ui.toolbar_hover || self.table_ui.menu_op.is_some()
    }

    pub(crate) fn hide_table_grips(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_ui.grip_hover.take().is_some() {
            cx.notify();
        }
    }

    fn table_panel_open(&self) -> bool {
        self.table_ui.picker_open || self.table_ui.more_open
    }

    pub(crate) fn apply_menu_grip_hover(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_panel_open()
            || self.table_ui.col_resize.is_some()
            || self.table_ui.reorder.is_some()
        {
            return;
        }
        if !self.menu_driving_grips() {
            return;
        }
        let next = caret_menu_grips(
            &self.state.doc,
            self.state.cursor.block,
            self.table_ui.menu_op,
        );
        if self.table_ui.grip_hover != next {
            self.table_ui.grip_hover = next;
            cx.notify();
        }
    }

    pub(crate) fn set_table_toolbar_hover(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.table_ui.toolbar_hover == on {
            return;
        }
        self.table_ui.toolbar_hover = on;
        if on {
            self.apply_menu_grip_hover(cx);
        } else if self.table_ui.menu_op.is_none() && self.table_ui.grip_hover.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn set_table_menu_op_hover(
        &mut self,
        op: Option<TableOp>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.table_ui.menu_op == op {
            return;
        }
        self.table_ui.menu_op = op;
        if self.menu_driving_grips() {
            self.apply_menu_grip_hover(cx);
        } else if self.table_ui.grip_hover.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn clear_table_menu_hover(&mut self, cx: &mut Context<'_, Self>) {
        let had = self.table_ui.toolbar_hover || self.table_ui.menu_op.is_some();
        self.table_ui.toolbar_hover = false;
        self.table_ui.menu_op = None;
        if had && self.table_ui.grip_hover.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn update_table_grip_hover(
        &mut self,
        cells: &[CellHit],
        pos: (Px, Px),
        cx: &mut Context<'_, Self>,
    ) {
        if self.table_panel_open()
            || self.table_ui.col_resize.is_some()
            || self.table_ui.reorder.is_some()
        {
            self.hide_table_grips(cx);
            return;
        }
        if self.menu_driving_grips() {
            return;
        }
        let next = hover_grips(&self.state.doc, cells, self.state.cursor.block, pos);
        if self.table_ui.grip_hover != next {
            self.table_ui.grip_hover = next;
            cx.notify();
        }
    }

    pub(super) fn begin_table_reorder(
        &mut self,
        hit: GripHit,
        pointer: (Px, Px),
        cells: &[CellHit],
        cx: &mut Context<'_, Self>,
    ) {
        self.close_table_picker(true, cx);
        self.table_ui.more_open = false;
        let bands = collect_bands(&self.state.doc, cells, hit.table);
        let grab = bands
            .as_ref()
            .map(|bands| grab_for(bands, hit, pointer))
            .unwrap_or((0.0, 0.0));
        let dest = bands
            .as_ref()
            .map(|bands| dest_for(bands, hit.axis, pointer))
            .unwrap_or(hit.from);
        self.table_ui.reorder = Some(ReorderDrag {
            table: hit.table,
            axis: hit.axis,
            from: hit.from,
            cell: hit.cell,
            grab,
            pointer,
            dest,
        });
        self.table_ui.grip_hover = None;
        self.dragging = false;
        self.pending_click = None;
        self.drag_pointer = None;
        self.follow_caret = false;
        cx.notify();
    }

    pub(super) fn table_reorder_to(
        &mut self,
        cells: &[CellHit],
        pos: (Px, Px),
        cx: &mut Context<'_, Self>,
    ) {
        let Some(mut drag) = self.table_ui.reorder else {
            return;
        };
        drag.pointer = pos;
        if let Some(bands) = collect_bands(&self.state.doc, cells, drag.table) {
            drag.dest = dest_for(&bands, drag.axis, pos);
        }
        self.table_ui.reorder = Some(drag);
        cx.notify();
    }

    pub(super) fn end_table_reorder(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(drag) = self.table_ui.reorder.take() else {
            return;
        };
        let dest = drag.dest;
        if dest == drag.from {
            cx.notify();
            return;
        }
        self.place_cursor(
            Cursor {
                block: drag.cell,
                offset: 0,
            },
            CursorMotion::Move,
        );
        let op = match drag.axis {
            ReorderAxis::Row => TableOp::MoveRowTo { index: dest },
            ReorderAxis::Col => TableOp::MoveColumnTo { index: dest },
        };
        self.apply_table_toolbar(op, window, cx);
    }

    pub(super) fn cancel_table_reorder(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_ui.reorder.take().is_some() {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::target_index;

    #[test]
    fn target_index_uses_containing_band() {
        let bands = [(0, 0.0, 100.0), (1, 100.0, 200.0), (2, 200.0, 300.0)];
        assert_eq!(target_index(-10.0, &bands, 3), 0);
        assert_eq!(target_index(0.0, &bands, 3), 0);
        assert_eq!(target_index(99.0, &bands, 3), 0);
        assert_eq!(target_index(100.0, &bands, 3), 1);
        assert_eq!(target_index(150.0, &bands, 3), 1);
        assert_eq!(target_index(199.0, &bands, 3), 1);
        assert_eq!(target_index(200.0, &bands, 3), 2);
        assert_eq!(target_index(250.0, &bands, 3), 2);
        assert_eq!(target_index(400.0, &bands, 3), 2);
    }
}
