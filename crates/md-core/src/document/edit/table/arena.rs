use super::{
    TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS, TABLE_INSERT_MIN_ROWS,
};
use crate::block::{BlockKind, NodeExtra, TableCellAlign};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::Caret;

pub(crate) fn kids(doc: &Document, id: NodeId) -> Vec<NodeId> {
    doc.arena.children(id).collect()
}

pub(crate) fn table_packed(doc: &Document, table: NodeId) -> u64 {
    match doc.extra(table) {
        NodeExtra::Table { alignments, .. } => alignments,
        _ => 0,
    }
}

pub(crate) fn dims(doc: &Document, table: NodeId) -> (usize, usize) {
    let mut rows = doc.arena.children(table);
    let Some(first) = rows.next() else {
        return (0, 0);
    };
    let mut cols = doc.arena.children(first).count();
    let mut row_count = 1;
    for row in rows {
        row_count += 1;
        cols = cols.max(doc.arena.children(row).count());
    }
    (row_count, cols)
}

#[cfg(debug_assertions)]
pub(crate) fn debug_assert_rectangular_table(doc: &Document, table: NodeId) {
    let Some(table_node) = doc.arena.get(table) else {
        return;
    };
    assert_eq!(
        table_node.kind,
        BlockKind::Table,
        "table root must stay a Table"
    );
    let rows = kids(doc, table);
    assert!(!rows.is_empty(), "table must keep at least one row");
    let expected_cols = kids(doc, rows[0]).len();
    assert!(expected_cols > 0, "table rows must keep at least one cell");
    for row in rows {
        assert_eq!(
            doc.arena.get(row).map(|node| node.kind),
            Some(BlockKind::TableRow),
            "table children must be TableRow nodes"
        );
        let cells = kids(doc, row);
        assert_eq!(
            cells.len(),
            expected_cols,
            "table rows must have the same number of cells"
        );
        assert!(
            cells.iter().all(|cell| {
                doc.arena.get(*cell).map(|node| node.kind) == Some(BlockKind::TableCell)
            }),
            "table row children must be TableCell nodes"
        );
    }
}

pub(crate) fn cell_at(doc: &Document, row: NodeId, col: usize) -> Option<NodeId> {
    doc.arena.children(row).nth(col)
}

pub(crate) fn alloc_cell(doc: &mut Document, align: TableCellAlign, header: bool) -> NodeId {
    let cell = doc.alloc_leaf(BlockKind::TableCell);
    doc.set_extra(cell, NodeExtra::Cell { align, header });
    cell
}

pub(crate) fn alloc_row(doc: &mut Document, cols: usize, table: NodeId) -> NodeId {
    let row = doc.alloc_container(BlockKind::TableRow);
    for col in 0..cols {
        let align = TableCellAlign::from(doc.table_alignment_at(table, col));
        let cell = alloc_cell(doc, align, false);
        doc.arena.append_child(row, cell);
    }
    row
}

fn clamp_insert_dims(rows: usize, cols: usize) -> (usize, usize) {
    (
        rows.clamp(TABLE_INSERT_MIN_ROWS, TABLE_INSERT_MAX_ROWS),
        cols.clamp(TABLE_INSERT_MIN_COLS, TABLE_INSERT_MAX_COLS),
    )
}

pub(crate) fn alloc_table(
    doc: &mut Document,
    rows: usize,
    cols: usize,
) -> (NodeId, NodeId, NodeId) {
    let (rows, cols) = clamp_insert_dims(rows, cols);
    let table = doc.alloc_container(BlockKind::Table);
    doc.set_extra(
        table,
        NodeExtra::Table {
            alignments: 0,
            source: None,
        },
    );
    let mut header = None;
    let mut body = None;
    for ri in 0..rows {
        let row = doc.alloc_container(BlockKind::TableRow);
        if ri == 0 {
            doc.set_extra(row, NodeExtra::HeaderRow);
        }
        for _ in 0..cols {
            let cell = alloc_cell(doc, TableCellAlign::Start, ri == 0);
            if header.is_none() {
                header = Some(cell);
            } else if ri == 1 && body.is_none() {
                body = Some(cell);
            }
            doc.arena.append_child(row, cell);
        }
        doc.arena.append_child(table, row);
    }

    let header = header.expect("header");
    (table, header, body.unwrap_or(header))
}

pub(crate) fn set_packed(
    doc: &mut Document,
    table: NodeId,
    packed: u64,
    changes: &mut Vec<DocChange>,
) {
    let old = doc.extra(table);
    if matches!(old, NodeExtra::Table { alignments, .. } if alignments == packed) {
        return;
    }
    doc.set_extra(
        table,
        NodeExtra::Table {
            alignments: packed,
            source: None,
        },
    );
    changes.push(doc.attrs_change(table, BlockKind::Table, old));
}

pub(crate) fn retarget_header(doc: &mut Document, table: NodeId, changes: &mut Vec<DocChange>) {
    let rows = kids(doc, table);
    for (i, row) in rows.into_iter().enumerate() {
        let header = i == 0;
        let want_row = if header {
            NodeExtra::HeaderRow
        } else {
            NodeExtra::None
        };
        let old_row = doc.extra(row);
        if old_row != want_row {
            doc.set_extra(row, want_row);
            changes.push(doc.attrs_change(row, BlockKind::TableRow, old_row));
        }
        for (ci, cell) in kids(doc, row).into_iter().enumerate() {
            if doc.arena.get(cell).map(|node| node.kind) != Some(BlockKind::TableCell) {
                continue;
            }
            let want = NodeExtra::Cell {
                align: TableCellAlign::from(doc.table_alignment_at(table, ci)),
                header,
            };
            let old = doc.extra(cell);
            if old != want {
                doc.set_extra(cell, want);
                changes.push(doc.attrs_change(cell, BlockKind::TableCell, old));
            }
        }
    }
}

pub(crate) fn caret_of(cell: NodeId) -> Caret {
    Caret {
        block: cell.index,
        offset: 0,
    }
}
