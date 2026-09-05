use super::arena::{caret_of, kids};
use super::nav::TablePath;
use crate::block::{BlockId, BlockKind};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::Caret;

pub(crate) fn cell_table(doc: &Document, block: BlockId) -> Option<NodeId> {
    TablePath::at(doc, Caret { block, offset: 0 }).map(|p| p.table)
}

pub(crate) fn table_end_cells(doc: &Document, table: NodeId) -> Option<(BlockId, BlockId)> {
    let rows = kids(doc, table);
    let first = *kids(doc, *rows.first()?).first()?;
    let last = *kids(doc, *rows.last()?).last()?;
    Some((first.index, last.index))
}

pub(crate) fn replace_table_with_paragraph(doc: &mut Document, table: NodeId) -> Option<Caret> {
    let parent = doc.arena.get(table).and_then(|n| n.parent)?;
    let prev = doc.arena.get(table).and_then(|n| n.prev_sibling);
    let before = doc.revision;
    let para = doc.alloc_leaf(BlockKind::Paragraph);
    doc.arena.insert_after(parent, prev, para);
    doc.arena.detach(table);
    doc.bump_structure(parent);
    let _ = doc.commit(
        before,
        vec![DocChange::TreeSpliced {
            parent,
            before: prev,
            removed: vec![table],
            inserted: vec![para],
        }],
    );
    Some(Caret {
        block: para.index,
        offset: 0,
    })
}

pub(crate) fn detach_table(doc: &mut Document, table: NodeId) {
    let Some(parent) = doc.arena.get(table).and_then(|n| n.parent) else {
        return;
    };
    let prev = doc.arena.get(table).and_then(|n| n.prev_sibling);
    let before = doc.revision;
    doc.arena.detach(table);
    doc.bump_structure(parent);
    let _ = doc.commit(
        before,
        vec![DocChange::TreeSpliced {
            parent,
            before: prev,
            removed: vec![table],
            inserted: Vec::new(),
        }],
    );
}

pub(crate) fn delete_table(doc: &mut Document, path: TablePath) -> Caret {
    replace_table_with_paragraph(doc, path.table).unwrap_or_else(|| caret_of(path.cell))
}
