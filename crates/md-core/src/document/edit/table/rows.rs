use super::arena::{alloc_row, caret_of, cell_at, dims, kids, retarget_header};
use super::nav::TablePath;
use crate::document::Document;
use crate::document::change::DocChange;
use crate::document::edit::Caret;

pub(crate) fn insert_row(doc: &mut Document, path: TablePath, below: bool) -> Caret {
    let (_, cols) = dims(doc, path.table);
    if cols == 0 {
        return caret_of(path.cell);
    }
    let before = doc.revision;
    let row = alloc_row(doc, cols, path.table);
    let after = if below {
        Some(path.row)
    } else {
        doc.arena.get(path.row).and_then(|n| n.prev_sibling)
    };
    doc.arena.insert_after(path.table, after, row);
    let mut changes = vec![DocChange::TreeSpliced {
        parent: path.table,
        before: after,
        removed: Vec::new(),
        inserted: vec![row],
    }];
    doc.bump_structure(row);
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    let col = path.col.min(cols.saturating_sub(1));
    Caret {
        block: cell_at(doc, row, col).unwrap_or(path.cell).index,
        offset: 0,
    }
}

pub(crate) fn delete_row(doc: &mut Document, path: TablePath) -> Caret {
    let (n_rows, cols) = dims(doc, path.table);
    if n_rows <= 1 {
        return caret_of(path.cell);
    }
    let rows = kids(doc, path.table);
    let fallback = if path.row_index + 1 < n_rows {
        rows[path.row_index + 1]
    } else {
        rows[path.row_index - 1]
    };
    let col = path.col.min(cols.saturating_sub(1));
    let next_cell = cell_at(doc, fallback, col).unwrap_or(path.cell);
    let prev = doc.arena.get(path.row).and_then(|n| n.prev_sibling);
    let before = doc.revision;
    doc.arena.detach(path.row);
    let mut changes = vec![DocChange::TreeSpliced {
        parent: path.table,
        before: prev,
        removed: vec![path.row],
        inserted: Vec::new(),
    }];
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    Caret {
        block: next_cell.index,
        offset: 0,
    }
}

pub(crate) fn move_row(doc: &mut Document, path: TablePath, down: bool) -> Caret {
    let before = doc.revision;
    let mut changes = Vec::new();
    if !move_row_inner(doc, path, down, &mut changes) {
        return caret_of(path.cell);
    }
    doc.bump_structure(path.table);
    retarget_header(doc, path.table, &mut changes);
    let _ = doc.commit(before, changes);
    caret_of(path.cell)
}

fn move_row_inner(
    doc: &mut Document,
    path: TablePath,
    down: bool,
    changes: &mut Vec<DocChange>,
) -> bool {
    let rows = kids(doc, path.table);
    let n = rows.len();
    if n < 2 {
        return false;
    }
    if down && path.row_index + 1 >= n {
        return false;
    }
    if !down && path.row_index == 0 {
        return false;
    }
    let (left, right) = if down {
        (path.row, rows[path.row_index + 1])
    } else {
        (rows[path.row_index - 1], path.row)
    };
    let pair_prev = doc.arena.get(left).and_then(|n| n.prev_sibling);
    if down {
        doc.arena.insert_after(path.table, Some(right), left);
    } else {
        doc.arena.insert_after(path.table, pair_prev, right);
    }
    changes.push(DocChange::TreeSpliced {
        parent: path.table,
        before: pair_prev,
        removed: vec![left, right],
        inserted: vec![right, left],
    });
    true
}

pub(crate) fn move_row_to(doc: &mut Document, mut path: TablePath, index: usize) -> Caret {
    let n = kids(doc, path.table).len();
    if n < 2 {
        return caret_of(path.cell);
    }
    let target = index.min(n - 1);
    if target == path.row_index {
        return caret_of(path.cell);
    }
    let down = target > path.row_index;
    let steps = if down {
        target - path.row_index
    } else {
        path.row_index - target
    };
    let before = doc.revision;
    let mut changes = Vec::new();
    for _ in 0..steps {
        if !move_row_inner(doc, path, down, &mut changes) {
            break;
        }
        path.row_index = if down {
            path.row_index.saturating_add(1)
        } else {
            path.row_index.saturating_sub(1)
        };
    }
    if !changes.is_empty() {
        doc.bump_structure(path.table);
        retarget_header(doc, path.table, &mut changes);
        let _ = doc.commit(before, changes);
    }
    caret_of(path.cell)
}
