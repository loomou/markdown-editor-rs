use super::arena::NodeId;
use super::chars::floor_char_boundary;
use super::edit::Sel;
use super::{Document, bind, write};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use crate::inline::covering_runs;
use std::ops::Range;

impl Document {
    pub fn copy_markdown(&self, sel: Sel) -> String {
        let mut leaves = Vec::new();
        let mut i0 = None;
        let mut i1 = None;
        self.for_each_text_leaf(|id, _| {
            let index = leaves.len();
            if id == sel.anchor.block {
                i0 = Some(index);
            }
            if id == sel.head.block {
                i1 = Some(index);
            }
            leaves.push(id);
            true
        });
        let Some(i0) = i0 else {
            return String::new();
        };
        let Some(i1) = i1 else {
            return String::new();
        };
        let (lo_i, hi_i, from, to) = match i0.cmp(&i1) {
            std::cmp::Ordering::Less => (i0, i1, sel.anchor.offset, sel.head.offset),
            std::cmp::Ordering::Greater => (i1, i0, sel.head.offset, sel.anchor.offset),
            std::cmp::Ordering::Equal => (
                i0,
                i0,
                sel.anchor.offset.min(sel.head.offset),
                sel.anchor.offset.max(sel.head.offset),
            ),
        };
        self.copy_leaf_span(&leaves, lo_i, hi_i, from, to)
    }

    fn copy_leaf_span(
        &self,
        leaves: &[BlockId],
        lo_i: usize,
        hi_i: usize,
        from: usize,
        to: usize,
    ) -> String {
        let mut out = String::new();

        let mut group: Option<(NodeId, u64)> = None;
        let mut i = lo_i;
        while i <= hi_i {
            let Some(id) = self.live_id(leaves[i]) else {
                i += 1;
                continue;
            };
            let len = self.caret_text(id).len();
            let start = if i == lo_i { from.min(len) } else { 0 };
            let end = if i == hi_i { to.min(len) } else { len };
            let whole = start == 0 && end == len;
            if whole && let Some((item, last)) = self.covered_item(leaves, i, hi_i, to) {
                let list = self.arena.get(item).and_then(|n| n.parent);
                let extra = list.map(|l| self.extra(l)).unwrap_or(NodeExtra::None);

                let continuing = matches!(group, Some((g, _)) if Some(g) == list);
                let num = match group {
                    Some((g, n)) if Some(g) == list => n + 1,
                    _ => extra.ordered_start().unwrap_or(1),
                };
                let mut piece = String::new();
                write::write_list_item(self, item, &mut piece, extra.list_marker(), num);
                push_piece(&mut out, &piece, !continuing || extra.list_loose());
                group = list.map(|l| (l, num));
                i = last + 1;
                continue;
            }
            group = None;
            push_piece(&mut out, &self.copy_leaf(id, start..end), true);
            i += 1;
        }
        out
    }

    fn covered_item(
        &self,
        leaves: &[BlockId],
        at: usize,
        hi_i: usize,
        to: usize,
    ) -> Option<(NodeId, usize)> {
        let leaf = self.live_id(leaves[at])?;
        let mut best = None;
        let mut up = self.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = up {
            let Some(node) = self.arena.get(id) else {
                break;
            };
            up = node.parent;
            if node.kind != BlockKind::ListItem {
                continue;
            }
            let mut kids = Vec::new();
            self.subtree_text_leaves(id, &mut kids);
            let (Some(&first), Some(&last)) = (kids.first(), kids.last()) else {
                continue;
            };
            if first != leaf {
                continue;
            }
            let end = at + kids.len() - 1;
            if end > hi_i || (end == hi_i && to < self.caret_text(last).len()) {
                continue;
            }
            best = Some((id, end));
        }
        best
    }

    fn subtree_text_leaves(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if self.arena.get(id).is_some_and(|n| n.kind.is_text_leaf()) {
                out.push(id);
            }
            let mut child = self.arena.get(id).and_then(|node| node.last_child);
            while let Some(id) = child {
                stack.push(id);
                child = self.arena.get(id).and_then(|node| node.prev_sibling);
            }
        }
    }

    fn copy_leaf(&self, id: NodeId, range: Range<usize>) -> String {
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let text = self.caret_text(id);
        let lo = floor_char_boundary(text, range.start.min(text.len()));
        let hi = floor_char_boundary(text, range.end.min(text.len())).max(lo);
        if lo >= hi {
            return String::new();
        }
        if lo == 0 && hi == text.len() && kind != BlockKind::TableCell {
            let mut out = String::new();
            write::write_node(self, id, &mut out);
            return out;
        }
        if kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            return self.phrasing_slice(id, lo..hi);
        }
        text.get(lo..hi).unwrap_or("").to_string()
    }

    fn phrasing_slice(&self, id: NodeId, range: Range<usize>) -> String {
        let display = self.display(id);
        let source = self.leaf_source(id);
        let s2d = self.visual_s2d(id);
        let mapped = s2d.last().copied() == Some(display.len());
        let mut out = String::new();
        for run in covering_runs(display.len() as u32, self.runs(id)) {
            let rs = run.display_range.start as usize;
            let re = run.display_range.end as usize;
            let lo = range.start.max(rs);
            let hi = range.end.min(re);
            if lo >= hi {
                continue;
            }
            if mapped
                && (lo, hi) == (rs, re)
                && (!run.marks.is_empty() || run.link.is_some())
                && let Some(text) = source_span(source, &s2d, rs, re)
            {
                out.push_str(text);
                continue;
            }
            out.push_str(display.get(lo..hi).unwrap_or(""));
        }
        out
    }
}

fn source_span<'a>(source: &'a str, s2d: &[usize], rs: usize, re: usize) -> Option<&'a str> {
    let a = bind::display_to_source_first(s2d, rs);
    let b = bind::display_to_source_outer(s2d, re);
    let a = floor_char_boundary(source, a.min(source.len()));
    let b = floor_char_boundary(source, b.min(source.len()));
    if a >= b {
        return None;
    }
    source.get(a..b)
}

fn push_piece(out: &mut String, piece: &str, blank: bool) {
    if piece.is_empty() {
        return;
    }
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if blank && !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    out.push_str(piece);
}
