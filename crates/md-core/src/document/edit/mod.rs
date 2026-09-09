use super::{Document, PasteIntent};
use crate::block::BlockId;

mod delete;
mod list;
pub(crate) mod normalize;
mod path;
mod span;
mod structure;
mod table;
mod typing;

#[cfg(test)]
mod tests;

use delete::{delete_backward, delete_forward};
use structure::{break_block, indent, outdent, paste, soft_break, toggle_task, wrap_list};
use typing::insert;

pub use table::{
    TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS, TABLE_INSERT_MIN_ROWS,
    TABLE_PICKER_MAX_COLS, TABLE_PICKER_MAX_ROWS, TableLoc, TableOp, TableStep, in_table,
    table_loc, table_step,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caret {
    pub block: BlockId,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub anchor: Caret,
    pub head: Caret,
}

impl Sel {
    pub fn collapsed(caret: Caret) -> Self {
        Sel {
            anchor: caret,
            head: caret,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Insert { text: String },
    DeleteBackward,
    DeleteForward,
    Break,
    SoftBreak,
    Indent,
    Outdent,
    ToggleTask,
    WrapList { ordered: bool, task: Option<bool> },
    Paste { text: String, intent: PasteIntent },
    Table(TableOp),
}

pub fn apply(doc: &mut Document, sel: Sel, cmd: Command) -> Caret {
    let caret = match cmd {
        Command::Insert { text } => insert(doc, sel, &text),
        Command::DeleteBackward => delete_backward(doc, sel),
        Command::DeleteForward => delete_forward(doc, sel),
        Command::Break => break_block(doc, sel),
        Command::SoftBreak => soft_break(doc, sel),
        Command::Indent => indent(doc, sel),
        Command::Outdent => outdent(doc, sel),
        Command::ToggleTask => toggle_task(doc, sel),
        Command::WrapList { ordered, task } => wrap_list(doc, sel, ordered, task),
        Command::Paste { text, intent } => paste(doc, sel, &text, intent),
        Command::Table(op) => table::apply_table(doc, sel, op),
    };
    doc.clamp_live_caret(caret)
}
