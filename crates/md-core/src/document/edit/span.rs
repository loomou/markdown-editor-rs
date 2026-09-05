use super::{Caret, Sel};
use crate::block::{BlockId, TextEditStrategy};
use crate::document::arena::NodeId;
use crate::document::{Document, bind, floor_char_boundary};
use std::ops::Range;

pub(super) fn same_block_span(sel: Sel) -> Option<(BlockId, usize, usize)> {
    if sel.anchor.block != sel.head.block {
        return None;
    }
    let lo = sel.anchor.offset.min(sel.head.offset);
    let hi = sel.anchor.offset.max(sel.head.offset);
    Some((sel.anchor.block, lo, hi))
}

pub(super) fn delete_sel(doc: &mut Document, sel: Sel) -> Option<Caret> {
    if let Some((block, lo, hi)) = same_block_span(sel)
        && lo < hi
    {
        let lists = super::list::lists_touching_leaf(doc, block);
        let _ = doc.replace_text(block, lo..hi, "");
        return Some(super::list::collapse_empty_after_span(
            doc,
            &lists,
            Caret { block, offset: lo },
            false,
        ));
    }
    if sel.anchor.block == sel.head.block {
        return None;
    }

    let (span, forward) = leaves_between(doc, sel.anchor.block, sel.head.block)?;
    doc.clear_inline_focus();
    let (from, to) = if forward {
        (sel.anchor.offset, sel.head.offset)
    } else {
        (sel.head.offset, sel.anchor.offset)
    };
    let first = *span.first()?;
    let last = *span.last()?;
    let t0 = super::table::cell_table(doc, first);
    let t1 = super::table::cell_table(doc, last);
    if let (Some(table), Some(other)) = (t0, t1)
        && table == other
    {
        return delete_in_table(doc, table, first, from, last, to);
    }
    delete_across(doc, &span, from, to)
}

fn delete_in_table(
    doc: &mut Document,
    table: NodeId,
    first: BlockId,
    from: usize,
    last: BlockId,
    to: usize,
) -> Option<Caret> {
    let (start, end) = super::table::table_end_cells(doc, table)?;
    let last_len = leaf_len(doc, last);
    if first == start && from == 0 && last == end && to >= last_len {
        return super::table::replace_table_with_paragraph(doc, table);
    }
    let first_len = leaf_len(doc, first);
    let from = from.min(first_len);

    let mut cur = doc.live_id(first)?;
    if from < first_len {
        let _ = doc.replace_text(first, from..first_len, "");
    }
    while let Some(next) = doc.next_text_leaf(cur) {
        if next.index == last {
            break;
        }
        clear_leaf(doc, next.index);
        cur = next;
    }
    let last_len = leaf_len(doc, last);
    let to = to.min(last_len);
    if to > 0 {
        let _ = doc.replace_text(last, 0..to, "");
    }
    Some(Caret {
        block: first,
        offset: from,
    })
}

fn delete_across(doc: &mut Document, span: &[BlockId], from: usize, to: usize) -> Option<Caret> {
    let first = *span.first()?;
    let last = *span.last()?;
    let lists = super::list::lists_touching_span(doc, span);
    let first_table = super::table::cell_table(doc, first);
    let last_table = super::table::cell_table(doc, last);

    if let Some(table) = first_table
        && last_table.is_none()
        && let Some((table_start, table_end)) = super::table::table_end_cells(doc, table)
        && (first != table_start || from > 0)
    {
        let end_len = leaf_len(doc, table_end);

        let end_pos = span.iter().position(|&block| block == table_end)?;
        let _ = delete_in_table(doc, table, first, from, table_end, end_len);
        for &block in &span[end_pos + 1..] {
            if block == last {
                let len = leaf_len(doc, block);
                let to = to.min(len);
                if to > 0 {
                    let _ = doc.replace_text(block, 0..to, "");
                }
            } else {
                clear_leaf(doc, block);
            }
        }
        return Some(Caret {
            block: first,
            offset: from,
        });
    }
    if let Some(table) = last_table
        && first_table.is_none()
        && let Some((table_start, table_end)) = super::table::table_end_cells(doc, table)
        && (last != table_end || to < leaf_len(doc, last))
    {
        let first_len = leaf_len(doc, first);
        let from = from.min(first_len);

        let start_pos = span.iter().position(|&block| block == table_start)?;
        if from < first_len {
            let _ = doc.replace_text(first, from..first_len, "");
        }
        for &block in &span[1..start_pos] {
            clear_leaf(doc, block);
        }
        let _ = delete_in_table(doc, table, table_start, 0, last, to);
        return Some(Caret {
            block: first,
            offset: from,
        });
    }
    let suffix = if super::table::cell_table(doc, last).is_some() {
        String::new()
    } else {
        source_suffix(doc, last, to)
    };
    let tables = tables_in_span(doc, span);
    let (survivor, caret_off) = if let Some(table) = first_table {
        let caret = super::table::replace_table_with_paragraph(doc, table)?;
        if !suffix.is_empty() {
            let _ = doc.replace_text(caret.block, 0..0, &suffix);
        }
        (caret.block, 0)
    } else {
        let first_len = leaf_len(doc, first);
        let from = from.min(first_len);
        let _ = doc.replace_text(first, from..first_len, &suffix);
        (first, from)
    };
    for table in tables.iter().rev() {
        if first_table == Some(*table) {
            continue;
        }
        super::table::detach_table(doc, *table);
    }
    for &block in span.iter().rev() {
        if block == survivor || !in_tree(doc, block) {
            continue;
        }
        clear_leaf(doc, block);
        let _ = doc.merge_into_prev(block);
    }
    let caret = if first_table.is_some()
        && leaf_len(doc, survivor) == 0
        && let Some((_, block, offset)) = doc.merge_into_prev(survivor)
    {
        Caret { block, offset }
    } else {
        Caret {
            block: survivor,
            offset: caret_off,
        }
    };
    Some(super::list::collapse_empty_after_span(
        doc, &lists, caret, true,
    ))
}

fn tables_in_span(doc: &Document, span: &[BlockId]) -> Vec<NodeId> {
    let mut out = Vec::new();
    for &leaf in span {
        if let Some(table) = super::table::cell_table(doc, leaf)
            && out.last().copied() != Some(table)
        {
            out.push(table);
        }
    }
    out
}

fn leaf_len(doc: &Document, block: BlockId) -> usize {
    doc.live_id(block)
        .map(|id| doc.caret_text(id).len())
        .unwrap_or(0)
}

fn source_suffix(doc: &Document, block: BlockId, offset: usize) -> String {
    let Some(id) = doc.live_id(block) else {
        return String::new();
    };
    let source = doc.leaf_source(id);
    let strategy = doc
        .arena
        .get(id)
        .map(|node| node.kind.text_edit_strategy())
        .unwrap_or(TextEditStrategy::Literal);
    let source_at = match strategy {
        TextEditStrategy::Phrasing => {
            let display = doc.caret_text(id);
            let display_at = floor_char_boundary(display, offset.min(display.len()));
            bind::display_to_source_first(&doc.visual_s2d(id), display_at)
        }
        TextEditStrategy::BlockSource | TextEditStrategy::Literal => offset,
    };
    let source_at = floor_char_boundary(source, source_at.min(source.len()));
    source.get(source_at..).unwrap_or("").to_string()
}

fn clear_leaf(doc: &mut Document, block: BlockId) {
    let n = leaf_len(doc, block);
    if n > 0 {
        let _ = doc.replace_text(block, 0..n, "");
    }
}

fn in_tree(doc: &Document, block: BlockId) -> bool {
    let Some(mut id) = doc.live_id(block) else {
        return false;
    };
    loop {
        if id == doc.root {
            return true;
        }
        match doc.arena.get(id).and_then(|n| n.parent) {
            Some(p) => id = p,
            None => return false,
        }
    }
}

fn leaves_between(doc: &Document, anchor: BlockId, head: BlockId) -> Option<(Vec<BlockId>, bool)> {
    let start = doc.live_id(anchor)?;
    let head_id = doc.live_id(head)?;
    if !doc.arena.get(start)?.kind.is_text_leaf() || !doc.arena.get(head_id)?.kind.is_text_leaf() {
        return None;
    }
    let mut ahead = vec![anchor];
    let mut behind = vec![anchor];
    let mut fwd = Some(start);
    let mut back = Some(start);
    while fwd.is_some() || back.is_some() {
        if let Some(cur) = fwd {
            match doc.next_text_leaf(cur) {
                Some(next) => {
                    ahead.push(next.index);
                    if next.index == head {
                        return Some((ahead, true));
                    }
                    fwd = Some(next);
                }
                None => fwd = None,
            }
        }
        if let Some(cur) = back {
            match doc.prev_text_leaf(cur) {
                Some(prev) => {
                    behind.push(prev.index);
                    if prev.index == head {
                        behind.reverse();
                        return Some((behind, false));
                    }
                    back = Some(prev);
                }
                None => back = None,
            }
        }
    }
    None
}

pub(super) fn insert_span(sel: Sel) -> (BlockId, Range<usize>) {
    match same_block_span(sel) {
        Some((block, lo, hi)) => (block, lo..hi),
        None => (sel.head.block, sel.head.offset..sel.head.offset),
    }
}

pub(super) fn clear_same_block_span(doc: &mut Document, sel: Sel) -> Caret {
    delete_sel(doc, sel).unwrap_or(sel.head)
}
