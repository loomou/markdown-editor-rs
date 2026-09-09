use super::arena::NodeId;
use super::chars::floor_char_boundary;
use super::edit::Sel;
use super::reference;
use super::{Document, bind, write};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use crate::inline::{InlineMarks, InlineRun, covering_runs};
use std::collections::HashSet;
use std::ops::Range;

#[derive(Default)]
struct CopiedDeps {
    links: Vec<u32>,
    footnotes: Vec<String>,
    promoted_footnotes: Vec<NodeId>,
}

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
        if i0 == i1 && sel.anchor.offset == sel.head.offset {
            return String::new();
        }
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
        let mut deps = CopiedDeps::default();
        let piece = self.copy_leaf_span(&leaves, lo_i, hi_i, from, to, &mut deps);
        let piece = self.with_footnote_definitions(&mut deps, piece);
        self.with_reference_definitions(&deps.links, piece)
    }

    fn with_footnote_definitions(&self, deps: &mut CopiedDeps, mut out: String) -> String {
        if deps.footnotes.is_empty() {
            return out;
        }
        let defs: Vec<(String, NodeId)> = self
            .preorder()
            .into_iter()
            .filter_map(|id| {
                if self.arena.get(id)?.kind != BlockKind::FootnoteDefinition {
                    return None;
                }
                let intern = self.extra(id).footnote_label()?;
                let label = self.footnote_label(intern)?;
                Some((label.to_string(), id))
            })
            .collect();
        let mut queue: Vec<String> = deps.footnotes.clone();
        deps.footnotes.clear();
        let mut visited: HashSet<String> = HashSet::new();
        let mut carried: Vec<NodeId> = Vec::new();
        while let Some(label) = queue.pop() {
            if !visited.insert(label.clone()) {
                continue;
            }
            let Some((_, def)) = defs.iter().find(|(l, _)| *l == label) else {
                continue;
            };
            if deps.promoted_footnotes.contains(def) {
                continue;
            }
            carried.push(*def);
            let mut kids = Vec::new();
            self.subtree_text_leaves(*def, &mut kids);
            for kid in kids {
                self.collect_leaf_deps(kid, &mut queue, &mut deps.links);
            }
        }
        if carried.is_empty() {
            return out;
        }
        for def in carried {
            let mut piece = String::new();
            write::write_node(self, def, &mut piece);
            push_piece(&mut out, &piece, true);
        }
        out
    }

    fn with_reference_definitions(&self, used_links: &[u32], mut out: String) -> String {
        if self.reference_definitions.is_empty() {
            return out;
        }
        let mut wanted: Vec<(String, String)> = Vec::new();
        for &index in used_links {
            if let Some(link) = self.links.get(index as usize) {
                wanted.push((link.dest.clone(), link.title.clone()));
            }
        }
        let used = reference::used_definitions(&self.reference_definitions, &wanted);
        if used.is_empty() {
            return out;
        }
        if !out.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.ends_with("\n\n") {
                out.push('\n');
            }
        }
        for (index, line) in used.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(line.trim_end_matches(['\n', '\r']));
        }
        out
    }

    fn collect_leaf_deps(&self, id: NodeId, footnotes: &mut Vec<String>, links: &mut Vec<u32>) {
        let display = self.display(id);
        for run in self.runs(id) {
            if let Some(link) = run.link {
                links.push(link);
            }
            if let Some(label) = footnote_label_of(display, run) {
                footnotes.push(label);
            }
        }
    }

    fn collect_subtree_deps(
        &self,
        item: NodeId,
        footnotes: &mut Vec<String>,
        links: &mut Vec<u32>,
    ) {
        let mut kids = Vec::new();
        self.subtree_text_leaves(item, &mut kids);
        for kid in kids {
            self.collect_leaf_deps(kid, footnotes, links);
        }
    }

    fn copy_leaf_span(
        &self,
        leaves: &[BlockId],
        lo_i: usize,
        hi_i: usize,
        from: usize,
        to: usize,
        deps: &mut CopiedDeps,
    ) -> String {
        let mut out = String::new();
        let mut group: Option<(NodeId, u64)> = None;
        let rules = self.rules_between(leaves, lo_i, hi_i);
        let mut next_rule = 0;
        let mut jumped_item: Option<NodeId> = None;
        let mut i = lo_i;
        while i <= hi_i {
            while next_rule < rules.len() && rules[next_rule].0 <= i {
                let (rule, id) = rules[next_rule];
                let _ = rule;
                let skip = jumped_item.is_some_and(|item| self.descends_from(id, item));
                if !skip {
                    let mut piece = String::new();
                    write::write_node(self, id, &mut piece);
                    push_piece(&mut out, &piece, true);
                }
                next_rule += 1;
            }
            jumped_item = None;
            let Some(id) = self.live_id(leaves[i]) else {
                i += 1;
                continue;
            };
            let len = self.caret_text(id).len();
            let start = if i == lo_i { from.min(len) } else { 0 };
            let end = if i == hi_i { to.min(len) } else { len };
            let whole = start == 0 && end == len && (len > 0 || lo_i != hi_i);
            if whole && let Some((def, last)) = self.covered_footnote(leaves, i, hi_i, to) {
                let mut piece = String::new();
                write::write_node(self, def, &mut piece);
                self.collect_subtree_deps(def, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, true);
                deps.promoted_footnotes.push(def);
                i = last + 1;
                continue;
            }
            if whole && let Some((item, last)) = self.covered_item(leaves, i, hi_i, to) {
                let list = self.arena.get(item).and_then(|n| n.parent);
                let extra = list.map(|l| self.extra(l)).unwrap_or(NodeExtra::None);
                let continuing = matches!(group, Some((g, _)) if Some(g) == list);
                let num = match group {
                    Some((g, n)) if Some(g) == list => n.saturating_add(1).min(999_999_999),
                    _ => extra.ordered_start().unwrap_or(1),
                };
                let mut piece = String::new();
                write::write_list_item(self, item, &mut piece, extra.list_marker(), num);
                self.collect_subtree_deps(item, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, !continuing || extra.list_loose());
                group = list.map(|l| (l, num));
                jumped_item = Some(item);
                i = last + 1;
                continue;
            }
            group = None;
            if whole
                && self
                    .arena
                    .get(id)
                    .is_some_and(|n| n.kind != BlockKind::TableCell)
            {
                let mut piece = String::new();
                write::write_node(self, id, &mut piece);
                self.collect_leaf_deps(id, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, true);
            } else {
                let text = self.copy_leaf(id, start..end, deps);
                push_piece(&mut out, &text, true);
            }
            i += 1;
        }
        out
    }

    fn rules_between(&self, leaves: &[BlockId], lo_i: usize, hi_i: usize) -> Vec<(usize, NodeId)> {
        let _ = leaves;
        let mut out = Vec::new();
        let mut seen = 0usize;
        for id in self.preorder() {
            let Some(node) = self.arena.get(id) else {
                continue;
            };
            if node.kind.is_text_leaf() {
                seen += 1;
            } else if node.kind == BlockKind::ThematicBreak && seen > lo_i && seen <= hi_i {
                out.push((seen, id));
            }
        }
        out
    }

    fn descends_from(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut cur = self.arena.get(id).and_then(|n| n.parent);
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.arena.get(p).and_then(|n| n.parent);
        }
        false
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

    fn covered_footnote(
        &self,
        leaves: &[BlockId],
        at: usize,
        hi_i: usize,
        to: usize,
    ) -> Option<(NodeId, usize)> {
        let leaf = self.live_id(leaves[at])?;
        let mut up = self.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = up {
            let Some(node) = self.arena.get(id) else {
                break;
            };
            up = node.parent;
            if node.kind != BlockKind::FootnoteDefinition {
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
            return Some((id, end));
        }
        None
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

    fn copy_leaf(&self, id: NodeId, range: Range<usize>, deps: &mut CopiedDeps) -> String {
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
            self.collect_leaf_deps(id, &mut deps.footnotes, &mut deps.links);
            return out;
        }
        if kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            return self.phrasing_slice(id, lo..hi, deps);
        }
        text.get(lo..hi).unwrap_or("").to_string()
    }

    fn phrasing_slice(&self, id: NodeId, range: Range<usize>, deps: &mut CopiedDeps) -> String {
        let display = self.display(id);
        let source = self.leaf_source(id);
        let s2d = self.visual_s2d(id);
        let mapped = s2d.last().copied() == Some(display.len());
        let covering = covering_runs(display.len() as u32, self.runs(id));
        let full: Vec<bool> = covering
            .iter()
            .map(|run| {
                let rs = run.display_range.start as usize;
                let re = run.display_range.end as usize;
                mapped
                    && range.start <= rs
                    && re <= range.end
                    && rs < re
                    && (!run.marks.is_empty() || run.link.is_some())
            })
            .collect();
        let mut out = String::new();
        let mut i = 0;
        while i < covering.len() {
            let run = &covering[i];
            let rs = run.display_range.start as usize;
            let re = run.display_range.end as usize;
            if full[i] {
                let mut j = i;
                let mut hi = re;
                while j + 1 < covering.len() && full[j + 1] {
                    j += 1;
                    hi = hi.max(covering[j].display_range.end as usize);
                }
                for run in &covering[i..=j] {
                    if let Some(link) = run.link {
                        deps.links.push(link);
                    }
                    if let Some(label) = footnote_label_of(display, run) {
                        deps.footnotes.push(label);
                    }
                }
                if let Some(text) = bind::source_span(source, &s2d, rs, hi) {
                    out.push_str(text);
                } else {
                    bind::escape_literal(&mut out, display.get(rs..hi).unwrap_or(""));
                }
                i = j + 1;
                continue;
            }
            let lo = range.start.max(rs);
            let hi = range.end.min(re);
            if lo < hi {
                bind::escape_literal(&mut out, display.get(lo..hi).unwrap_or(""));
            }
            i += 1;
        }
        out
    }
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

fn footnote_label_of(display: &str, run: &InlineRun) -> Option<String> {
    if !run.marks.contains(InlineMarks::FOOTNOTE) {
        return None;
    }
    let rs = run.display_range.start as usize;
    let re = run.display_range.end as usize;
    let text = display.get(rs..re)?;
    let label = text.strip_prefix("[^")?.strip_suffix(']')?;
    (!label.is_empty()).then(|| label.to_string())
}
