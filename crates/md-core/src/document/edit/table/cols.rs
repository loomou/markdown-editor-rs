use super::arena::{alloc_cell, caret_of, dims, kids, retarget_header, set_packed, table_packed};
use super::nav::TablePath;
use crate::block::{
    TABLE_ALIGN_COLS, TableCellAlign, insert_alignment, remove_alignment, swap_alignment,
};
use crate::document::Document;
use crate::document::change::DocChange;
use crate::document::edit::Caret;

pub(crate) fn insert_column(doc: &mut Document, path: TablePath, right: bool) -> Caret {
    let (_, cols) = dims(doc, path.table);
    if cols >= TABLE_ALIGN_COLS {
        return caret_of(path.cell);
    }
    let at = if right { path.col + 1 } else { path.col };
    let packed = insert_alignment(table_packed(doc, path.table), at, 0);
    let before = doc.revision;
    let mut changes = Vec::new();
    let mut new_in_row = None;
    for row in kids(doc, path.table) {
        let cells = kids(doc, row);
        let idx = at.min(cells.len());
        let prev = if idx == 0 { None } else { Some(cells[idx - 1]) };
        let cell = alloc_cell(doc, TableCellAlign::Start, false);
        if row == path.row {
            new_in_row = Some(cell);
        }
        doc.arena.insert_after(row, prev, cell);
        changes.push(DocChange::TreeSpliced {
            parent: row,
            before: prev,
            removed: Vec::new(),
            inserted: vec![cell],
        });
        doc.bump_structure(row);
    }
    set_packed(doc, path.table, packed, &mut changes);
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    Caret {
        block: new_in_row.unwrap_or(path.cell).index,
        offset: 0,
    }
}

pub(crate) fn delete_column(doc: &mut Document, path: TablePath) -> Caret {
    let (_, cols) = dims(doc, path.table);
    if cols <= 1 || cols > super::TABLE_INSERT_MAX_COLS {
        return caret_of(path.cell);
    }
    let packed = remove_alignment(table_packed(doc, path.table), path.col);
    let before = doc.revision;
    let mut changes = Vec::new();
    let mut next_cell = path.cell;
    for row in kids(doc, path.table) {
        let cells = kids(doc, row);
        let Some(cell) = cells.get(path.col).copied() else {
            continue;
        };
        if row == path.row {
            next_cell = cells
                .get(path.col + 1)
                .copied()
                .or_else(|| {
                    path.col
                        .checked_sub(1)
                        .and_then(|col| cells.get(col).copied())
                })
                .unwrap_or(cell);
        }
        let prev = doc.arena.get(cell).and_then(|n| n.prev_sibling);
        doc.arena.detach(cell);
        changes.push(DocChange::TreeSpliced {
            parent: row,
            before: prev,
            removed: vec![cell],
            inserted: Vec::new(),
        });
        doc.bump_structure(row);
    }
    set_packed(doc, path.table, packed, &mut changes);
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    Caret {
        block: next_cell.index,
        offset: 0,
    }
}

pub(crate) fn move_column(doc: &mut Document, path: TablePath, right: bool) -> Caret {
    let before = doc.revision;
    let mut changes = Vec::new();
    if !move_column_inner(doc, path, right, &mut changes) {
        return caret_of(path.cell);
    }
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    caret_of(path.cell)
}

fn move_column_inner(
    doc: &mut Document,
    path: TablePath,
    right: bool,
    changes: &mut Vec<DocChange>,
) -> bool {
    let (_, cols) = dims(doc, path.table);
    if !(2..=super::TABLE_INSERT_MAX_COLS).contains(&cols) {
        return false;
    }
    if right && path.col + 1 >= cols {
        return false;
    }
    if !right && path.col == 0 {
        return false;
    }
    let a = if right { path.col } else { path.col - 1 };
    let b = a + 1;
    let packed = swap_alignment(table_packed(doc, path.table), a, b);
    for row in kids(doc, path.table) {
        let cells = kids(doc, row);
        if b >= cells.len() {
            continue;
        }
        let left = cells[a];
        let right_id = cells[b];
        let pair_prev = doc.arena.get(left).and_then(|n| n.prev_sibling);
        if right {
            doc.arena.insert_after(row, Some(right_id), left);
        } else {
            doc.arena.insert_after(row, pair_prev, right_id);
        }
        changes.push(DocChange::TreeSpliced {
            parent: row,
            before: pair_prev,
            removed: vec![left, right_id],
            inserted: vec![right_id, left],
        });
        doc.bump_structure(row);
    }
    set_packed(doc, path.table, packed, changes);
    true
}

pub(crate) fn move_column_to(doc: &mut Document, mut path: TablePath, index: usize) -> Caret {
    let (_, cols) = dims(doc, path.table);
    if !(2..=super::TABLE_INSERT_MAX_COLS).contains(&cols) {
        return caret_of(path.cell);
    }
    let target = index.min(cols - 1);
    if target == path.col {
        return caret_of(path.cell);
    }
    let right = target > path.col;
    let steps = if right {
        target - path.col
    } else {
        path.col - target
    };
    let before = doc.revision;
    let mut changes = Vec::new();
    for _ in 0..steps {
        if !move_column_inner(doc, path, right, &mut changes) {
            break;
        }
        path = TablePath::at(doc, caret_of(path.cell)).unwrap_or(path);
    }
    if !changes.is_empty() {
        doc.bump_structure(path.table);
        retarget_header(doc, path.table, &mut changes);
        let _ = doc.commit(before, changes);
    }
    caret_of(path.cell)
}
