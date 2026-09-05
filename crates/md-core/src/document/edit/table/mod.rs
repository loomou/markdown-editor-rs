use super::{Caret, Sel};
use crate::block::{TABLE_ALIGN_COLS, TableCellAlign};
use crate::document::Document;

mod align;
mod arena;
mod cols;
mod create;
mod lifecycle;
mod nav;
mod rows;

use align::{resize_table, set_column_align};
use arena::caret_of;
#[cfg(debug_assertions)]
use arena::debug_assert_rectangular_table;
use cols::{delete_column, insert_column, move_column, move_column_to};
use create::insert_table;
use lifecycle::delete_table;
pub(crate) use nav::TablePath;
use rows::{delete_row, insert_row, move_row, move_row_to};

pub use nav::{TableLoc, in_table, table_loc, table_step};

#[cfg(test)]
pub(crate) use create::parse_pipe_header;
pub(super) use create::{try_commit_pipe_table, try_delete_empty_table};
pub(super) use lifecycle::{
    cell_table, detach_table, replace_table_with_paragraph, table_end_cells,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOp {
    Insert { rows: usize, cols: usize },
    InsertRowAbove,
    InsertRowBelow,
    MoveRowUp,
    MoveRowDown,
    InsertColumnLeft,
    InsertColumnRight,
    MoveColumnLeft,
    MoveColumnRight,
    MoveRowTo { index: usize },
    MoveColumnTo { index: usize },
    DeleteRow,
    DeleteColumn,
    DeleteTable,
    SetColumnAlign(TableCellAlign),
    Resize { rows: usize, cols: usize },
}

pub const TABLE_PICKER_MAX_ROWS: usize = 5;
pub const TABLE_PICKER_MAX_COLS: usize = 8;
pub const TABLE_INSERT_MIN_ROWS: usize = 2;
pub const TABLE_INSERT_MIN_COLS: usize = 1;
pub const TABLE_INSERT_MAX_ROWS: usize = 100;
pub const TABLE_INSERT_MAX_COLS: usize = 32;

const _: () = assert!(TABLE_INSERT_MAX_COLS <= TABLE_ALIGN_COLS);

const _: () = assert!(TABLE_INSERT_MIN_ROWS >= 1 && TABLE_INSERT_MIN_COLS >= 1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableStep {
    NextCell,
    PrevCell,
    Above,
    Below,
    RowHome,
    ExitAfter,
    ExitBefore,
}

pub(super) fn apply_table(doc: &mut Document, sel: Sel, op: TableOp) -> Caret {
    if let TableOp::Insert { rows, cols } = op {
        let caret = insert_table(doc, sel.head, rows, cols);
        #[cfg(debug_assertions)]
        if let Some(path) = TablePath::at(doc, caret) {
            debug_assert_rectangular_table(doc, path.table);
        }
        return caret;
    }
    let Some(path) = TablePath::at(doc, sel.head) else {
        return sel.head;
    };

    #[cfg(debug_assertions)]
    let table = path.table;
    let caret = match op {
        TableOp::Insert { .. } => caret_of(path.cell),
        TableOp::InsertRowAbove => insert_row(doc, path, false),
        TableOp::InsertRowBelow => insert_row(doc, path, true),
        TableOp::MoveRowUp => move_row(doc, path, false),
        TableOp::MoveRowDown => move_row(doc, path, true),
        TableOp::InsertColumnLeft => insert_column(doc, path, false),
        TableOp::InsertColumnRight => insert_column(doc, path, true),
        TableOp::MoveColumnLeft => move_column(doc, path, false),
        TableOp::MoveColumnRight => move_column(doc, path, true),
        TableOp::MoveRowTo { index } => move_row_to(doc, path, index),
        TableOp::MoveColumnTo { index } => move_column_to(doc, path, index),
        TableOp::DeleteRow => delete_row(doc, path),
        TableOp::DeleteColumn => delete_column(doc, path),
        TableOp::DeleteTable => delete_table(doc, path),
        TableOp::SetColumnAlign(align) => set_column_align(doc, path, align),
        TableOp::Resize { rows, cols } => resize_table(doc, path, rows, cols),
    };
    #[cfg(debug_assertions)]
    debug_assert_rectangular_table(doc, table);
    caret
}

#[cfg(test)]
pub(crate) use align::resize_table as resize_table_unchecked;
#[cfg(test)]
pub(crate) use arena::retarget_header;
#[cfg(test)]
pub(crate) use cols::delete_column as delete_column_unchecked;
