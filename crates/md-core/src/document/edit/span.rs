use super::{Caret, Sel, normalize};
use crate::block::BlockId;
use crate::block::BlockKind;
use crate::document::Document;
use crate::document::arena::NodeId;
use std::ops::Range;

const CLOSE_OUT_ROUNDS: usize = 8;

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
        let (_, offset) = doc.replace_text_with_caret(block, lo..hi, "");
        let caret =
            super::list::collapse_empty_after_span(doc, &lists, Caret { block, offset }, false);
        return Some(close_out_emptied_document(doc, caret));
    }
    if sel.anchor.block == sel.head.block {
        return None;
    }
    let anchor_leaf = snap_forward(doc, sel.anchor.block)?;
    let head_leaf = snap_backward(doc, sel.head.block)?;
    let edges = Edges {
        top: sel.anchor.block != anchor_leaf,
        bottom: sel.head.block != head_leaf,
    };
    let (span, forward) = if anchor_leaf == head_leaf {
        (vec![anchor_leaf], true)
    } else {
        leaves_between(doc, anchor_leaf, head_leaf)?
    };
    let (start, end, start_leaf, end_leaf) = if forward {
        (sel.anchor, sel.head, anchor_leaf, head_leaf)
    } else {
        (sel.head, sel.anchor, head_leaf, anchor_leaf)
    };
    let from = if start.block == start_leaf {
        doc.visual_caret_to_collapsed(start).offset
    } else {
        0
    };
    let to = if end.block == end_leaf {
        doc.visual_caret_to_collapsed(end).offset
    } else {
        leaf_len(doc, end_leaf)
    };
    doc.clear_inline_focus();
    let first = *span.first()?;
    let last = *span.last()?;
    let t0 = super::table::cell_table(doc, first);
    let t1 = super::table::cell_table(doc, last);
    let caret = if let (Some(table), Some(other)) = (t0, t1)
        && table == other
    {
        delete_in_table(doc, table, first, from, last, to)?
    } else {
        delete_across(doc, &span, from, to, edges)?
    };
    Some(close_out_emptied_document(doc, caret))
}

#[derive(Clone, Copy, Default)]
pub(super) struct Edges {
    top: bool,
    bottom: bool,
}

fn snap_forward(doc: &Document, block: BlockId) -> Option<BlockId> {
    let id = doc.live_id(block)?;
    if doc.arena.get(id)?.kind.is_text_leaf() {
        return Some(block);
    }
    doc.next_text_leaf(id).map(|id| id.index)
}

fn snap_backward(doc: &Document, block: BlockId) -> Option<BlockId> {
    let id = doc.live_id(block)?;
    if doc.arena.get(id)?.kind.is_text_leaf() {
        return Some(block);
    }
    doc.prev_text_leaf(id).map(|id| id.index)
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
    let stop = doc.live_id(last);
    let mut caret = from;
    if from < first_len {
        let (_, offset) = doc.replace_text_with_caret(first, from..first_len, "");
        caret = offset;
    }
    while Some(cur) != stop
        && let Some(next) = doc.next_text_leaf(cur)
    {
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
        offset: caret,
    })
}

fn delete_across(
    doc: &mut Document,
    span: &[BlockId],
    from: usize,
    to: usize,
    edges: Edges,
) -> Option<Caret> {
    let first = *span.first()?;
    let last = *span.last()?;
    let lists = super::list::lists_touching_span(doc, span);
    let first_table = super::table::cell_table(doc, first);
    let last_table = super::table::cell_table(doc, last);
    let doomed = doomed_structures(doc, span, first, last, edges);
    if let Some(table) = first_table
        && last_table.is_none()
        && let Some((table_start, table_end)) = super::table::table_end_cells(doc, table)
        && (first != table_start || from > 0)
    {
        let end_len = leaf_len(doc, table_end);
        let end_pos = span.iter().position(|&block| block == table_end)?;
        let middle_tables = tables_in_span(doc, span);
        let caret = delete_in_table(doc, table, first, from, table_end, end_len)?;
        for t in middle_tables.iter().rev() {
            if *t != table {
                super::table::detach_table(doc, *t);
            }
        }
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
        detach_structures(doc, &doomed);
        return Some(caret);
    }
    if let Some(table) = last_table
        && first_table.is_none()
        && let Some((table_start, table_end)) = super::table::table_end_cells(doc, table)
        && (last != table_end || to < leaf_len(doc, last))
    {
        let first_len = leaf_len(doc, first);
        let from = from.min(first_len);
        let start_pos = span.iter().position(|&block| block == table_start)?;
        let middle_tables = tables_in_span(doc, span);
        let mut caret = from;
        if from < first_len {
            let (_, offset) = doc.replace_text_with_caret(first, from..first_len, "");
            caret = offset;
        }
        for t in middle_tables.iter().rev() {
            if *t != table {
                super::table::detach_table(doc, *t);
            }
        }
        for &block in &span[1..start_pos] {
            clear_leaf(doc, block);
        }
        let _ = delete_in_table(doc, table, table_start, 0, last, to);
        detach_structures(doc, &doomed);
        return Some(Caret {
            block: first,
            offset: caret,
        });
    }
    let head_partial = first_table.is_some_and(|table| {
        super::table::table_end_cells(doc, table)
            .is_some_and(|(start, _)| first != start || from > 0)
    });
    let tail_partial = last_table.is_some_and(|table| {
        super::table::table_end_cells(doc, table)
            .is_some_and(|(_, end)| last != end || to < leaf_len(doc, last))
    });
    if let (Some(t0), Some(t1)) = (first_table, last_table)
        && t0 != t1
        && (head_partial || tail_partial)
    {
        let caret = delete_across_two_tables(doc, span, first, from, last, to);
        detach_structures(doc, &doomed);
        return caret;
    }
    let suffix = if super::table::cell_table(doc, last).is_some() {
        String::new()
    } else {
        doc.source_suffix(last, to)
    };
    let tables = tables_in_span(doc, span);
    let mut keep_tail = false;
    let (survivor, caret_off) = if let Some(table) = first_table {
        let caret = super::table::replace_table_with_paragraph(doc, table)?;
        if !suffix.is_empty() {
            let _ = doc.replace_text(caret.block, 0..0, &suffix);
        }
        (caret.block, 0)
    } else {
        let first_len = leaf_len(doc, first);
        let from = from.min(first_len);
        let joined = doc.replace_text(first, from..first_len, &suffix);
        if joined.is_empty() && !suffix.is_empty() {
            keep_tail = true;
            let (_, offset) = doc.replace_text_with_caret(first, from..first_len, "");
            let _ = doc.replace_text(last, 0..to, "");
            (first, offset)
        } else {
            (first, from)
        }
    };
    for table in tables.iter().rev() {
        if first_table == Some(*table) {
            continue;
        }
        super::table::detach_table(doc, *table);
    }
    detach_structures(doc, &doomed);
    let quote_before = doc.revision;
    let mut quote_changes = Vec::new();
    let quote_caret = normalize::drop_covered_empty_quotes(
        doc,
        span,
        from == 0,
        Caret {
            block: survivor,
            offset: caret_off,
        },
        &mut quote_changes,
    );
    if !quote_changes.is_empty() {
        let _ = doc.commit(quote_before, quote_changes);
    }
    for &block in span.iter().rev() {
        if block == survivor || (keep_tail && block == last) || !in_tree(doc, block) {
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
        quote_caret
    };
    let caret = super::list::collapse_empty_after_span(doc, &lists, caret, true);
    Some(demote_emptied_survivor(doc, caret))
}

fn demote_emptied_survivor(doc: &mut Document, caret: Caret) -> Caret {
    demote_emptied_block(doc, caret).unwrap_or(caret)
}

fn demote_emptied_block(doc: &mut Document, caret: Caret) -> Option<Caret> {
    let id = doc.live_id(caret.block)?;
    if !doc.display(id).is_empty() {
        return None;
    }
    doc.try_demote_heading(id)
        .or_else(|| doc.try_demote_empty_block(id))
        .or_else(|| lift_out_of_emptied_wrapper(doc, id))
}

fn close_out_emptied_document(doc: &mut Document, caret: Caret) -> Caret {
    if !document_is_empty_except(doc, caret.block) {
        return caret;
    }
    let mut caret = caret;
    let mut rounds = 0;
    while rounds < CLOSE_OUT_ROUNDS {
        let before = doc.revision();
        if let Some(next) = demote_emptied_block(doc, caret) {
            caret = next;
        }
        let lists = super::list::lists_touching_leaf(doc, caret.block);
        caret = super::list::collapse_empty_after_span(doc, &lists, caret, false);
        if doc.revision() == before {
            break;
        }
        rounds += 1;
    }
    caret
}

fn document_is_empty_except(doc: &Document, keep: BlockId) -> bool {
    let keep_id = doc.live_id(keep);
    for id in doc.preorder() {
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        if Some(id) == keep_id {
            continue;
        }
        if node.kind.is_text_leaf() {
            if !doc.display(id).is_empty() {
                return false;
            }
            continue;
        }
        if matches!(node.kind, BlockKind::ThematicBreak | BlockKind::Table) {
            return false;
        }
    }
    true
}

fn lift_out_of_emptied_wrapper(doc: &mut Document, id: NodeId) -> Option<Caret> {
    let wrapper = doc.arena.get(id).and_then(|n| n.parent)?;
    let only_child = doc.arena.get(wrapper).and_then(|n| n.first_child) == Some(id)
        && doc.arena.get(id).and_then(|n| n.next_sibling).is_none();
    if !only_child {
        return None;
    }
    doc.try_lift_wrapper(id)
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

fn doomed_structures(
    doc: &Document,
    _span: &[BlockId],
    first: BlockId,
    last: BlockId,
    edges: Edges,
) -> Vec<NodeId> {
    let Some(first_id) = doc.live_id(first) else {
        return Vec::new();
    };
    let mut first_index = None;
    let mut last_index = None;
    let mut seen = 0usize;
    for id in doc.preorder() {
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        if node.kind.is_text_leaf() {
            if id == first_id {
                first_index = Some(seen);
            }
            if id.index == last {
                last_index = Some(seen);
            }
            seen += 1;
        }
    }
    let (Some(lo), Some(hi)) = (first_index, last_index) else {
        return Vec::new();
    };
    let lo = if edges.top { lo } else { lo + 1 };
    let hi = if edges.bottom { hi + 1 } else { hi };
    let mut seen = 0usize;
    let mut doomed: Vec<NodeId> = Vec::new();
    for id in doc.preorder() {
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        if node.kind.is_text_leaf() {
            seen += 1;
        } else if seen >= lo
            && seen <= hi
            && matches!(node.kind, crate::block::BlockKind::ThematicBreak)
            && node.parent.is_some()
        {
            doomed.push(id);
        }
    }
    doomed
}

fn detach_structures(doc: &mut Document, doomed: &[NodeId]) {
    for id in doomed.iter().rev() {
        super::table::detach_table(doc, *id);
    }
}

fn delete_across_two_tables(
    doc: &mut Document,
    span: &[BlockId],
    first: BlockId,
    from: usize,
    last: BlockId,
    to: usize,
) -> Option<Caret> {
    let first_table = super::table::cell_table(doc, first)?;
    let last_table = super::table::cell_table(doc, last)?;
    let head_partial = super::table::table_end_cells(doc, first_table)
        .is_some_and(|(start, _)| first != start || from > 0);
    let tail_partial = super::table::table_end_cells(doc, last_table)
        .is_some_and(|(_, end)| last != end || to < leaf_len(doc, last));
    let (t1_start, _) = super::table::table_end_cells(doc, last_table)?;
    let t1_start_pos = span.iter().position(|&b| b == t1_start)?;
    let (survivor, caret_off) = if head_partial {
        let (_, t0_end) = super::table::table_end_cells(doc, first_table)?;
        let end_len = leaf_len(doc, t0_end);
        let caret = delete_in_table(doc, first_table, first, from, t0_end, end_len)?;
        (first, caret.offset)
    } else {
        super::table::detach_table(doc, first_table);
        (t1_start, 0)
    };
    for table in tables_in_span(doc, span) {
        if table == first_table || table == last_table {
            continue;
        }
        super::table::detach_table(doc, table);
    }
    for &block in &span[..t1_start_pos] {
        if block == survivor || !in_tree(doc, block) {
            continue;
        }
        clear_leaf(doc, block);
    }
    if tail_partial {
        let _ = delete_in_table(doc, last_table, t1_start, 0, last, to);
    } else {
        super::table::detach_table(doc, last_table);
    }
    Some(Caret {
        block: survivor,
        offset: caret_off,
    })
}

fn leaf_len(doc: &Document, block: BlockId) -> usize {
    doc.live_id(block)
        .map(|id| doc.caret_text(id).len())
        .unwrap_or(0)
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
