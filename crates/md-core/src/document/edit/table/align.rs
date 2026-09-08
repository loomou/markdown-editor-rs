use super::arena::{
    caret_of, cell_at, dims, kids, retarget_header, set_alignment_at, set_packed, table_packed,
};
use super::cols::{delete_column, insert_column};
use super::nav::TablePath;
use super::rows::{delete_row, insert_row};
use super::{TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS};
use crate::block::{NodeExtra, TableCellAlign, set_alignment};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::edit::Caret;

pub(crate) fn set_column_align(
    doc: &mut Document,
    path: TablePath,
    align: TableCellAlign,
) -> Caret {
    let bits = align.bits();
    if doc.table_alignment_at(path.table, path.col) == bits {
        let same = kids(doc, path.table).into_iter().all(|row| {
            cell_at(doc, row, path.col).is_some_and(|cell| match doc.extra(cell) {
                NodeExtra::Cell { align: a, .. } => a == align,
                _ => false,
            })
        });
        if same {
            return caret_of(path.cell);
        }
    }
    let before = doc.revision;
    let mut changes = Vec::new();
    set_alignment_at(doc, path.table, path.col, bits, &mut changes);
    set_packed(
        doc,
        path.table,
        set_alignment(table_packed(doc, path.table), path.col, bits),
        &mut changes,
    );
    retarget_header(doc, path.table, &mut changes);
    if changes.is_empty() {
        return caret_of(path.cell);
    }
    let _ = doc.commit(before, changes);
    caret_of(path.cell)
}

fn tail_path(doc: &Document, table: NodeId) -> Option<TablePath> {
    let rows = kids(doc, table);
    let row_index = rows.len().checked_sub(1)?;
    let row = rows[row_index];
    let cells = kids(doc, row);
    let col = cells.len().checked_sub(1)?;
    let cell = cells[col];
    Some(TablePath {
        cell,
        row,
        table,
        col,
        row_index,
    })
}

pub(crate) fn resize_table(doc: &mut Document, path: TablePath, rows: usize, cols: usize) -> Caret {
    let rows = rows.clamp(1, TABLE_INSERT_MAX_ROWS);
    let cols = cols.clamp(1, TABLE_INSERT_MAX_COLS);
    let (cur_r, cur_c) = dims(doc, path.table);
    if cur_r == rows && cur_c == cols {
        return caret_of(path.cell);
    }
    if cur_c > TABLE_INSERT_MAX_COLS {
        return caret_of(path.cell);
    }
    while dims(doc, path.table).1 < cols {
        let Some(tail) = tail_path(doc, path.table) else {
            break;
        };
        let before_cols = dims(doc, path.table).1;
        let _ = insert_column(doc, tail, true);
        if dims(doc, path.table).1 == before_cols {
            break;
        }
    }
    while dims(doc, path.table).1 > cols {
        let Some(tail) = tail_path(doc, path.table) else {
            break;
        };
        let before_cols = dims(doc, path.table).1;
        let _ = delete_column(doc, tail);
        if dims(doc, path.table).1 == before_cols {
            break;
        }
    }
    while dims(doc, path.table).0 < rows {
        let Some(tail) = tail_path(doc, path.table) else {
            break;
        };
        let before_rows = dims(doc, path.table).0;
        let _ = insert_row(doc, tail, true);
        if dims(doc, path.table).0 == before_rows {
            break;
        }
    }
    while dims(doc, path.table).0 > rows {
        let Some(tail) = tail_path(doc, path.table) else {
            break;
        };
        let before_rows = dims(doc, path.table).0;
        let _ = delete_row(doc, tail);
        if dims(doc, path.table).0 == before_rows {
            break;
        }
    }
    if TablePath::at(
        doc,
        Caret {
            block: path.cell.index,
            offset: 0,
        },
    )
    .is_some_and(|p| p.table == path.table)
    {
        return caret_of(path.cell);
    }
    let (n_rows, n_cols) = dims(doc, path.table);
    let ri = path.row_index.min(n_rows.saturating_sub(1));
    let ci = path.col.min(n_cols.saturating_sub(1));
    let rows = kids(doc, path.table);
    let cell = rows
        .get(ri)
        .copied()
        .and_then(|row| cell_at(doc, row, ci))
        .unwrap_or(path.cell);
    caret_of(cell)
}
