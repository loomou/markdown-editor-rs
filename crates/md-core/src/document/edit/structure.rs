use super::span::{clear_same_block_span, same_block_span};
use super::typing::insert;
use super::{Caret, Sel, list, path};
use crate::block::BlockKind;
use crate::document::syntax::line_range;
use crate::document::{Document, PasteIntent};

fn is_fence_leaf(kind: Option<BlockKind>) -> bool {
    matches!(
        kind,
        Some(BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math)
    )
}

pub(super) fn break_block(doc: &mut Document, sel: Sel) -> Caret {
    let at = clear_same_block_span(doc, sel);
    if doc.kind(at.block) == Some(BlockKind::ThematicBreak) {
        return at;
    }
    if matches!(
        doc.kind(at.block),
        Some(BlockKind::TableCell | BlockKind::Image)
    ) {
        return at;
    }
    if is_fence_leaf(doc.kind(at.block))
        && let Some(id) = doc.live_id(at.block)
    {
        return doc.break_literal_leaf(id, at.offset);
    }
    if let Some(path) = path::Path::at(doc, at)
        && path.simple_empty_item(doc)
    {
        return list::lift_item(doc, &path);
    }
    if let Some(id) = doc.live_id(at.block)
        && let Some(caret) = doc.try_commit_open_fence(id)
    {
        return caret;
    }
    if let Some(id) = doc.live_id(at.block)
        && let Some(caret) = doc.try_commit_math_fence(id)
    {
        return caret;
    }
    if let Some(id) = doc.live_id(at.block)
        && let Some(caret) = doc.try_commit_thematic_break(id)
    {
        return caret;
    }
    if let Some(caret) = super::table::try_commit_pipe_table(doc, at) {
        return caret;
    }
    if let Some(path) = path::Path::at(doc, at)
        && path.item_direct_leaf(doc)
    {
        return list::split_item(doc, &path);
    }
    if let Some(id) = doc.live_id(at.block)
        && let Some(caret) = doc.try_lift_empty_wrapper(id)
    {
        return caret;
    }
    if !doc.kind(at.block).is_some_and(|k| k.is_text_leaf()) {
        return at;
    }
    let (_, id) = doc.split_leaf(at.block, at.offset);
    Caret {
        block: id,
        offset: 0,
    }
}

pub(super) fn soft_break(doc: &mut Document, sel: Sel) -> Caret {
    let at = clear_same_block_span(doc, sel);
    if doc.kind(at.block) == Some(BlockKind::ThematicBreak) {
        return at;
    }
    if doc.kind(at.block) == Some(BlockKind::TableCell)
        && let Some(id) = doc.live_id(at.block)
    {
        return doc.break_literal_leaf(id, at.offset);
    }
    if doc.kind(at.block) == Some(BlockKind::Image) {
        return at;
    }
    if is_fence_leaf(doc.kind(at.block))
        && let Some(id) = doc.live_id(at.block)
    {
        return doc.break_literal_leaf(id, at.offset);
    }
    if let Some(id) = doc.live_id(at.block)
        && let Some(caret) = doc.try_break_commonmark(id, at.offset)
    {
        return caret;
    }
    if !doc.kind(at.block).is_some_and(|k| k.is_text_leaf()) {
        return at;
    }
    let (_, id) = doc.split_leaf(at.block, at.offset);
    Caret {
        block: id,
        offset: 0,
    }
}

pub(super) fn indent(doc: &mut Document, sel: Sel) -> Caret {
    let at = sel.head;
    if doc.kind(at.block) == Some(BlockKind::TableCell) {
        return at;
    }
    if sel.anchor.block != sel.head.block {
        if let Some((list, items)) = path::item_span(doc, sel) {
            return list::sink_items(doc, list, &items, at);
        }
        return at;
    }
    if is_fence_leaf(doc.kind(at.block)) {
        return indent_in_leaf(doc, sel);
    }
    if let Some((list, items)) = path::item_span(doc, sel) {
        return list::sink_items(doc, list, &items, at);
    }
    if doc.kind(at.block).is_some_and(|k| k.is_text_leaf()) {
        return indent_in_leaf(doc, sel);
    }
    at
}

pub(super) fn outdent(doc: &mut Document, sel: Sel) -> Caret {
    let at = sel.head;
    if doc.kind(at.block) == Some(BlockKind::TableCell) {
        return at;
    }
    if sel.anchor.block != sel.head.block {
        if let Some((list, items)) = path::item_span(doc, sel) {
            return list::lift_items(doc, list, &items, at);
        }
        return at;
    }
    if is_fence_leaf(doc.kind(at.block)) {
        return outdent_in_leaf(doc, sel);
    }
    if let Some((list, items)) = path::item_span(doc, sel) {
        return list::lift_items(doc, list, &items, at);
    }
    if doc.kind(at.block).is_some_and(|k| k.is_text_leaf()) {
        return outdent_in_leaf(doc, sel);
    }
    at
}

fn indent_in_leaf(doc: &mut Document, sel: Sel) -> Caret {
    match same_block_span(sel) {
        Some((block, lo, hi)) if lo < hi => indent_lines(doc, block, lo, hi, sel.head.offset),
        _ => insert(doc, sel, "\t"),
    }
}

fn outdent_in_leaf(doc: &mut Document, sel: Sel) -> Caret {
    let (block, lo, hi) = match same_block_span(sel) {
        Some(span) => span,
        None => (sel.head.block, sel.head.offset, sel.head.offset),
    };
    outdent_lines(doc, block, lo, hi, sel.head.offset)
}

fn line_starts_overlapping(text: &str, lo: usize, hi: usize) -> Vec<usize> {
    let lo = lo.min(text.len());
    let hi = hi.min(text.len());
    let last = if hi > lo { hi - 1 } else { lo };
    let mut out = Vec::new();
    let mut i = line_range(text, lo).0;
    loop {
        out.push(i);
        let end = line_range(text, i).1;
        if end >= last || end >= text.len() {
            break;
        }
        i = end + 1;
        if i > last {
            break;
        }
    }
    out
}

enum LineDomain {
    Same,
    Phrasing,
}

fn line_domain(doc: &Document, block: u32) -> LineDomain {
    let phrasing = doc
        .live_id(block)
        .and_then(|id| doc.arena.get(id))
        .map(|n| n.kind)
        .is_some_and(|kind| kind.text_edit_strategy() == crate::block::TextEditStrategy::Phrasing);
    if phrasing {
        LineDomain::Phrasing
    } else {
        LineDomain::Same
    }
}

fn source_line_start(doc: &Document, id: crate::document::NodeId, display_start: usize) -> usize {
    use crate::document::bind;
    let source = doc.leaf_source(id);
    let s2d = doc.visual_s2d(id);
    let at = bind::display_to_source_first(&s2d, display_start);
    let at = crate::document::floor_char_boundary(source, at.min(source.len()));
    line_range(source, at).0
}

fn head_to_source(doc: &Document, id: crate::document::NodeId, block: u32, head: usize) -> usize {
    match line_domain(doc, block) {
        LineDomain::Same => head,
        LineDomain::Phrasing => {
            let s2d = doc.visual_s2d(id);
            let at = crate::document::bind::display_to_source_first(&s2d, head);
            let source = doc.leaf_source(id);
            crate::document::floor_char_boundary(source, at.min(source.len()))
        }
    }
}

fn indent_lines(doc: &mut Document, block: u32, lo: usize, hi: usize, head: usize) -> Caret {
    let Some(id) = doc.live_id(block) else {
        return Caret {
            block,
            offset: head,
        };
    };
    let text = doc.caret_text(id).to_string();
    let starts = line_starts_overlapping(&text, lo, hi);
    let mut src_starts = Vec::with_capacity(starts.len());
    match line_domain(doc, block) {
        LineDomain::Same => src_starts.extend(starts.iter().copied()),
        LineDomain::Phrasing => {
            for &start in &starts {
                src_starts.push(source_line_start(doc, id, start));
            }
        }
    }
    let head_src = head_to_source(doc, id, block, head);
    src_starts.sort_unstable();
    src_starts.dedup();
    for &start in src_starts.iter().rev() {
        let _ = doc.replace_text_at_source(block, start..start, "\t");
    }
    let added = src_starts.iter().filter(|&&s| s < head_src).count();
    Caret {
        block,
        offset: head + added,
    }
}

fn leading_indent(line: &str) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    line.bytes().take_while(|&b| b == b' ').count().min(4)
}

fn outdent_lines(doc: &mut Document, block: u32, lo: usize, hi: usize, head: usize) -> Caret {
    let Some(id) = doc.live_id(block) else {
        return Caret {
            block,
            offset: head,
        };
    };
    let text = doc.caret_text(id).to_string();
    let starts = line_starts_overlapping(&text, lo, hi);
    let mut removed_before = 0usize;
    let mut src_starts = Vec::with_capacity(starts.len());
    match line_domain(doc, block) {
        LineDomain::Same => src_starts.extend(starts.iter().copied()),
        LineDomain::Phrasing => {
            for &start in &starts {
                src_starts.push(source_line_start(doc, id, start));
            }
        }
    }
    src_starts.sort_unstable();
    src_starts.dedup();
    let head_src = head_to_source(doc, id, block, head);
    let mut applied = 0usize;
    for &orig in &src_starts {
        let start = orig.saturating_sub(applied);
        let source = doc.leaf_source(id);
        let line_start = line_range(source, start).0;
        let n = leading_indent(source.get(line_start..).unwrap_or(""));
        if n == 0 {
            continue;
        }
        if orig < head_src {
            removed_before += n.min(head_src - orig);
        }
        let _ = doc.replace_text_at_source(block, line_start..line_start + n, "");
        applied += n;
    }
    Caret {
        block,
        offset: head.saturating_sub(removed_before),
    }
}

pub(super) fn toggle_task(doc: &mut Document, sel: Sel) -> Caret {
    let at = sel.head;
    if let Some(path) = path::Path::at(doc, at)
        && path.item.is_some()
    {
        return list::toggle_task(doc, &path);
    }
    at
}

pub(super) fn wrap_list(doc: &mut Document, sel: Sel, ordered: bool, task: Option<bool>) -> Caret {
    let at = sel.head;
    if doc.kind(at.block) != Some(BlockKind::Paragraph) {
        return at;
    }
    if let Some(path) = path::Path::at(doc, at)
        && path.item.is_some()
    {
        return at;
    }
    let consume = doc
        .live_id(at.block)
        .and_then(|id| list::marker_prefix_spec(doc.display(id)))
        .map(|(len, _, _)| len)
        .unwrap_or(0);
    list::wrap_paragraph(doc, at, ordered, task, consume)
}

pub(super) fn paste(doc: &mut Document, sel: Sel, text: &str, intent: PasteIntent) -> Caret {
    let at = clear_same_block_span(doc, sel);
    let host_list = path::Path::at(doc, at).and_then(|p| p.list);
    let (_, block, offset) = doc.paste(at.block, at.offset..at.offset, text, intent);
    let caret = Caret { block, offset };
    if intent != PasteIntent::IndependentFragment {
        return caret;
    }
    let landed = path::Path::at(doc, caret).and_then(|p| p.list);
    if landed.is_none() || landed == host_list {
        return caret;
    }
    list::join_nested_into_host(doc, caret)
}
