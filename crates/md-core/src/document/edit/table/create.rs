use super::arena::{alloc_table, cell_at, kids};
use super::lifecycle::delete_table;
use super::nav::TablePath;
use super::{TABLE_INSERT_MAX_COLS, TABLE_INSERT_MIN_ROWS};
use crate::block::BlockKind;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::Caret;

pub(crate) fn insert_table(doc: &mut Document, caret: Caret, rows: usize, cols: usize) -> Caret {
    splice_table(doc, caret, rows, cols, &[], false, false)
}

pub(crate) fn try_delete_empty_table(doc: &mut Document, caret: Caret) -> Option<Caret> {
    if caret.offset != 0 {
        return None;
    }
    let path = TablePath::at(doc, caret)?;
    if path.row_index != 0 || path.col != 0 {
        return None;
    }
    if !table_cells_empty(doc, path.table) {
        return None;
    }
    Some(delete_table(doc, path))
}

fn table_cells_empty(doc: &Document, table: NodeId) -> bool {
    kids(doc, table).into_iter().all(|row| {
        kids(doc, row)
            .into_iter()
            .all(|cell| doc.display(cell).is_empty())
    })
}

pub(crate) fn try_commit_pipe_table(doc: &mut Document, caret: Caret) -> Option<Caret> {
    if TablePath::at(doc, caret).is_some() {
        return None;
    }
    let id = doc.live_id(caret.block)?;
    if doc.arena.get(id).map(|n| n.kind) != Some(BlockKind::Paragraph) {
        return None;
    }
    if doc.enclosed_by(id, BlockKind::ListItem) || doc.enclosed_by(id, BlockKind::BlockQuote) {
        return None;
    }
    let source = doc.leaf_source(id);
    if source.contains('\n') {
        return None;
    }
    let headers = parse_pipe_header(source)?;
    let cols = headers.len().min(TABLE_INSERT_MAX_COLS);
    if cols == 0 {
        return None;
    }
    Some(splice_table(
        doc,
        caret,
        TABLE_INSERT_MIN_ROWS,
        cols,
        &headers[..cols],
        true,
        true,
    ))
}

fn splice_table(
    doc: &mut Document,
    caret: Caret,
    rows: usize,
    cols: usize,
    headers: &[String],
    replace: bool,
    caret_in_body: bool,
) -> Caret {
    if TablePath::at(doc, caret).is_some() {
        return caret;
    }
    let Some(leaf) = doc.live_id(caret.block) else {
        return caret;
    };
    let Some(kind) = doc.arena.get(leaf).map(|n| n.kind) else {
        return caret;
    };
    if !kind.is_text_leaf() {
        return caret;
    }
    let Some(parent) = doc.arena.get(leaf).and_then(|n| n.parent) else {
        return caret;
    };
    let replace = replace || (kind == BlockKind::Paragraph && doc.display(leaf).is_empty());
    let prev = if replace {
        doc.arena.get(leaf).and_then(|n| n.prev_sibling)
    } else {
        Some(leaf)
    };
    let before = doc.revision;
    let (table, header, body) = alloc_table(doc, rows, cols);
    if replace {
        doc.arena.detach(leaf);
    }
    doc.arena.insert_after(parent, prev, table);
    let removed = if replace { vec![leaf] } else { Vec::new() };
    doc.bump_structure(parent);
    let _ = doc.commit(
        before,
        vec![DocChange::TreeSpliced {
            parent,
            before: prev,
            removed,
            inserted: vec![table],
        }],
    );
    fill_header_cells(doc, table, headers);
    let target = if caret_in_body { body } else { header };
    Caret {
        block: target.index,
        offset: 0,
    }
}

fn fill_header_cells(doc: &mut Document, table: NodeId, headers: &[String]) {
    let Some(row) = kids(doc, table).first().copied() else {
        return;
    };
    for (i, text) in headers.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let Some(cell) = cell_at(doc, row, i) else {
            continue;
        };
        let _ = doc.replace_text(cell.index, 0..0, text);
    }
}

pub(crate) fn parse_pipe_header(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('|') || line.contains('\n') {
        return None;
    }
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    chars.next();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some('\\') => {
                    chars.next();
                    if chars.peek() == Some(&'|') {
                        chars.next();
                        cur.push('|');
                    } else {
                        cur.push('\\');
                        cur.push('\\');
                    }
                    continue;
                }
                Some('|') => {
                    chars.next();
                    cur.push('|');
                    continue;
                }
                _ => {}
            }
        }
        if c == '|' {
            cells.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(c);
    }
    if !line.ends_with('|') || !cur.is_empty() {
        cells.push(cur);
    }
    if cells.is_empty() {
        return None;
    }
    let mut cells: Vec<String> = cells.into_iter().map(|c| c.trim().to_string()).collect();
    while cells.first().is_some_and(|c| c.is_empty()) {
        cells.remove(0);
    }
    while cells.last().is_some_and(|c| c.is_empty()) {
        cells.pop();
    }
    if cells.is_empty() {
        return None;
    }
    if cells.iter().all(|c| is_sep_cell(c)) {
        return None;
    }
    Some(cells)
}

fn is_sep_cell(s: &str) -> bool {
    s.chars().filter(|c| *c == '-').count() >= 3 && s.chars().all(|c| c == '-' || c == ':')
}
