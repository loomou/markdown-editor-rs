use super::TableStep;
use super::arena::{cell_at, dims, kids};
use crate::block::{BlockId, BlockKind, TableCellAlign};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::edit::Caret;

pub fn in_table(doc: &Document, block: BlockId) -> bool {
    doc.kind(block) == Some(BlockKind::TableCell)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableLoc {
    pub table: BlockId,
    pub rows: usize,
    pub cols: usize,
    pub row: usize,
    pub col: usize,
    pub align: TableCellAlign,
}

pub fn table_loc(doc: &Document, block: BlockId) -> Option<TableLoc> {
    let path = TablePath::at(doc, Caret { block, offset: 0 })?;
    let (rows, cols) = dims(doc, path.table);
    Some(TableLoc {
        table: path.table.index,
        rows,
        cols,
        row: path.row_index,
        col: path.col,
        align: TableCellAlign::from(doc.table_alignment_at(path.table, path.col)),
    })
}

pub fn table_step(doc: &Document, caret: Caret, step: TableStep) -> Option<Caret> {
    let path = TablePath::at(doc, caret)?;
    match step {
        TableStep::NextCell => step_row_major(doc, &path, 1),
        TableStep::PrevCell => step_row_major(doc, &path, -1),
        TableStep::Above => step_vertical(doc, &path, false),
        TableStep::Below => step_vertical(doc, &path, true),
        TableStep::RowHome => {
            let cell = cell_at(doc, path.row, 0)?;
            Some(caret_at(doc, cell, false))
        }
        TableStep::ExitAfter => exit_after(doc, path.table),
        TableStep::ExitBefore => exit_before(doc, path.table),
    }
}

fn step_row_major(doc: &Document, path: &TablePath, dir: i32) -> Option<Caret> {
    let (n_rows, n_cols) = dims(doc, path.table);
    if n_rows == 0 || n_cols == 0 {
        return None;
    }
    let i = path.row_index * n_cols + path.col;
    let next = i as i32 + dir;
    if next < 0 || next as usize >= n_rows * n_cols {
        return None;
    }
    let next = next as usize;
    let row = kids(doc, path.table).get(next / n_cols).copied()?;
    let cell = cell_at(doc, row, next % n_cols)?;
    Some(caret_at(doc, cell, dir < 0))
}

fn step_vertical(doc: &Document, path: &TablePath, down: bool) -> Option<Caret> {
    let rows = kids(doc, path.table);
    let ri = if down {
        path.row_index.checked_add(1)?
    } else {
        path.row_index.checked_sub(1)?
    };
    let row = rows.get(ri).copied()?;
    let cell = cell_at(doc, row, path.col)?;
    Some(caret_at(doc, cell, false))
}

fn exit_after(doc: &Document, table: NodeId) -> Option<Caret> {
    let next = doc.arena.get(table).and_then(|n| n.next_sibling)?;
    let leaf = first_text_leaf(doc, next)?;
    Some(Caret {
        block: leaf.index,
        offset: 0,
    })
}

fn exit_before(doc: &Document, table: NodeId) -> Option<Caret> {
    let prev = doc.arena.get(table).and_then(|n| n.prev_sibling)?;
    let (block, offset) = doc.last_text_caret(prev)?;
    Some(Caret { block, offset })
}

fn first_text_leaf(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        if doc.arena.get(cur).is_some_and(|n| n.kind.is_text_leaf()) {
            return Some(cur);
        }
        let mut children = kids(doc, cur);
        for child in children.drain(..).rev() {
            stack.push(child);
        }
    }
    None
}

fn caret_at(doc: &Document, cell: NodeId, at_end: bool) -> Caret {
    Caret {
        block: cell.index,
        offset: if at_end { doc.display(cell).len() } else { 0 },
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TablePath {
    pub(crate) cell: NodeId,
    pub(crate) row: NodeId,
    pub(crate) table: NodeId,
    pub(crate) col: usize,
    pub(crate) row_index: usize,
}

impl TablePath {
    pub(crate) fn at(doc: &Document, caret: Caret) -> Option<Self> {
        let cell = doc.live_id(caret.block)?;
        let cell_node = doc.arena.get(cell)?;
        if cell_node.kind != BlockKind::TableCell {
            return None;
        }
        let row = cell_node.parent?;
        let row_node = doc.arena.get(row)?;
        if row_node.kind != BlockKind::TableRow {
            return None;
        }
        let table = row_node.parent?;
        if doc.arena.get(table).map(|n| n.kind) != Some(BlockKind::Table) {
            return None;
        }
        let col = doc.arena.children(row).position(|id| id == cell)?;
        let row_index = doc.arena.children(table).position(|id| id == row)?;
        Some(TablePath {
            cell,
            row,
            table,
            col,
            row_index,
        })
    }
}
