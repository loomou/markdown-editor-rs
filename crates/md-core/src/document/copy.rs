use super::arena::NodeId;
use super::chars::floor_char_boundary;
use super::edit::{Caret, Sel};
use super::reference;
use super::{Document, bind, write};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use crate::inline::{InlineMarks, InlineRun, covering_runs};
use std::cmp::Ordering;
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
        if self.selection_reaches_textless_block(sel) {
            return self.whole_document_markdown();
        }
        let anchor = self.first_leaf_of(sel.anchor);
        let Some(anchor_id) = self.live_id(anchor.block) else {
            return String::new();
        };
        let Some(head_id) = self.live_id(sel.head.block) else {
            return String::new();
        };
        let is_leaf = |id: NodeId| {
            self.arena
                .get(id)
                .is_some_and(|node| node.kind.is_text_leaf())
        };
        if !is_leaf(anchor_id) || !is_leaf(head_id) {
            return String::new();
        }
        let (lo, hi, from, to) = match self.cmp_reading_order(anchor.block, sel.head.block) {
            Ordering::Less => (anchor, sel.head, anchor.offset, sel.head.offset),
            Ordering::Greater => (sel.head, anchor, sel.head.offset, anchor.offset),
            Ordering::Equal => {
                if anchor.offset == sel.head.offset {
                    return String::new();
                }
                let from = anchor.offset.min(sel.head.offset);
                let to = anchor.offset.max(sel.head.offset);
                (anchor, sel.head, from, to)
            }
        };
        let mut deps = CopiedDeps::default();
        let piece = self.copy_leaf_span(lo, hi, from, to, &mut deps);
        let piece = self.with_footnote_definitions(&mut deps, piece);
        self.with_reference_definitions(&deps.links, piece)
    }

    fn selection_reaches_textless_block(&self, sel: Sel) -> bool {
        [sel.anchor.block, sel.head.block].into_iter().any(|block| {
            self.live_id(block)
                .is_some_and(|id| !self.subtree_has_text_leaf(id))
        })
    }

    fn first_leaf_of(&self, caret: Caret) -> Caret {
        let Some(id) = self.live_id(caret.block) else {
            return caret;
        };
        if self
            .arena
            .get(id)
            .is_some_and(|node| node.kind.is_text_leaf())
        {
            return caret;
        }
        match self.first_text_leaf_in(id) {
            Some(first) => Caret {
                block: first.index,
                offset: 0,
            },
            None => caret,
        }
    }

    fn first_text_leaf_in(&self, id: NodeId) -> Option<NodeId> {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let node = self.arena.get(cur)?;
            if node.kind.is_text_leaf() {
                return Some(cur);
            }
            let mut child = node.last_child;
            while let Some(c) = child {
                stack.push(c);
                child = self.arena.get(c).and_then(|n| n.prev_sibling);
            }
        }
        None
    }

    fn whole_document_markdown(&self) -> String {
        self.to_markdown().trim_end_matches('\n').to_string()
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
        let mut skipped = 0usize;
        for def in carried {
            let mut piece = String::new();
            write::write_node(self, def, &mut piece);
            push_piece(&mut out, &piece, true, &mut skipped);
        }
        out
    }

    fn with_reference_definitions(&self, used_links: &[u32], mut out: String) -> String {
        if self.reference_definitions.is_empty() || used_links.is_empty() {
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
        lo: Caret,
        hi: Caret,
        from: usize,
        to: usize,
        deps: &mut CopiedDeps,
    ) -> String {
        let mut out = String::new();
        let Some(lo_id) = self.live_id(lo.block) else {
            return out;
        };
        let Some(hi_id) = self.live_id(hi.block) else {
            return out;
        };
        let multi = lo_id != hi_id;
        let rules = self.rules_between(lo_id, hi_id);
        let mut next_rule = 0;
        let mut group: Option<(NodeId, u64)> = None;
        let mut jumped_container: Option<NodeId> = None;
        let mut skipped = 0usize;
        let mut cur = lo_id;
        loop {
            if self.cmp_reading_order(cur.index, hi.block) == Ordering::Greater {
                break;
            }
            while next_rule < rules.len()
                && self.cmp_reading_order(rules[next_rule].0.index, cur.index) != Ordering::Greater
            {
                let (_, rule) = rules[next_rule];
                let skip = jumped_container.is_some_and(|item| self.descends_from(rule, item));
                if !skip {
                    let mut piece = String::new();
                    write::write_node(self, rule, &mut piece);
                    push_piece(&mut out, &piece, true, &mut skipped);
                }
                next_rule += 1;
            }
            jumped_container = None;
            let len = self.caret_text(cur).len();
            let start = if cur == lo_id { from.min(len) } else { 0 };
            let end = if cur == hi_id { to.min(len) } else { len };
            let whole = start == 0 && end == len && (len > 0 || multi);
            if whole && let Some(def) = self.covered_footnote(cur, hi.block, to) {
                let mut piece = String::new();
                write::write_node(self, def, &mut piece);
                self.collect_subtree_deps(def, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, true, &mut skipped);
                deps.promoted_footnotes.push(def);
                jumped_container = Some(def);
                let Some(next) = self.leaf_after_container(cur, def) else {
                    break;
                };
                cur = next;
                continue;
            }
            if whole && let Some(quote) = self.covered_quote(cur, hi.block, to) {
                let mut piece = String::new();
                write::write_node(self, quote, &mut piece);
                self.collect_subtree_deps(quote, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, true, &mut skipped);
                jumped_container = Some(quote);
                let Some(next) = self.leaf_after_container(cur, quote) else {
                    break;
                };
                cur = next;
                continue;
            }
            if whole && let Some(item) = self.covered_item(cur, hi.block, to) {
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
                push_piece(&mut out, &piece, !continuing, &mut skipped);
                group = list.map(|l| (l, num));
                jumped_container = Some(item);
                let Some(next) = self.leaf_after_container(cur, item) else {
                    break;
                };
                cur = next;
                continue;
            }
            if whole && let Some(table) = self.covered_table(cur, hi.block, to) {
                let mut piece = String::new();
                write::write_node(self, table, &mut piece);
                self.collect_subtree_deps(table, &mut deps.footnotes, &mut deps.links);
                let piece = piece.trim_end_matches(['\n', '\r']);
                push_piece(&mut out, piece, true, &mut skipped);
                jumped_container = Some(table);
                let Some(next) = self.leaf_after_container(cur, table) else {
                    break;
                };
                cur = next;
                continue;
            }
            group = None;
            if whole
                && self
                    .arena
                    .get(cur)
                    .is_some_and(|n| n.kind != BlockKind::TableCell)
            {
                let mut piece = String::new();
                write::write_node(self, cur, &mut piece);
                self.collect_leaf_deps(cur, &mut deps.footnotes, &mut deps.links);
                push_piece(&mut out, &piece, true, &mut skipped);
            } else {
                let text = self.copy_leaf(cur, start..end, deps);
                push_piece(&mut out, &text, true, &mut skipped);
            }
            if cur == hi_id {
                break;
            }
            let Some(next) = self.next_text_leaf(cur) else {
                break;
            };
            cur = next;
        }
        out
    }

    fn rules_between(&self, lo: NodeId, hi: NodeId) -> Vec<(NodeId, NodeId)> {
        if lo == hi {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut pending: Vec<NodeId> = Vec::new();
        let mut cur = self.preorder_next(lo);
        while let Some(id) = cur {
            if id == hi {
                for rule in pending.drain(..) {
                    out.push((hi, rule));
                }
                break;
            }
            let Some(node) = self.arena.get(id) else {
                break;
            };
            if node.kind.is_text_leaf() {
                for rule in pending.drain(..) {
                    out.push((id, rule));
                }
            } else if node.kind == BlockKind::ThematicBreak {
                pending.push(id);
            }
            cur = self.preorder_next(id);
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

    fn starts_at(&self, leaf: NodeId, container: NodeId) -> bool {
        match self.prev_text_leaf(leaf) {
            Some(prev) => !self.descends_from(prev, container),
            None => true,
        }
    }

    fn covers_leaf(&self, container: NodeId, hi: BlockId, to: usize) -> bool {
        let Some(hi_id) = self.live_id(hi) else {
            return false;
        };
        if !self.descends_from(hi_id, container) {
            return true;
        }
        if to < self.caret_text(hi_id).len() {
            return false;
        }
        match self.next_text_leaf(hi_id) {
            Some(next) => !self.descends_from(next, container),
            None => true,
        }
    }

    fn leaf_after_container(&self, from: NodeId, container: NodeId) -> Option<NodeId> {
        let mut cur = self.next_text_leaf(from)?;
        while self.descends_from(cur, container) {
            cur = self.next_text_leaf(cur)?;
        }
        Some(cur)
    }

    fn covered_quote(&self, leaf: NodeId, hi: BlockId, to: usize) -> Option<NodeId> {
        let mut best = None;
        let mut up = self.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = up {
            let Some(node) = self.arena.get(id) else {
                break;
            };
            up = node.parent;
            if node.kind != BlockKind::BlockQuote {
                continue;
            }
            if self.starts_at(leaf, id) && self.covers_leaf(id, hi, to) {
                best = Some(id);
            }
        }
        best
    }

    fn covered_item(&self, leaf: NodeId, hi: BlockId, to: usize) -> Option<NodeId> {
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
            if self.starts_at(leaf, id) && self.covers_leaf(id, hi, to) {
                best = Some(id);
            }
        }
        best
    }

    fn covered_table(&self, leaf: NodeId, hi: BlockId, to: usize) -> Option<NodeId> {
        let mut up = self.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = up {
            let Some(node) = self.arena.get(id) else {
                break;
            };
            up = node.parent;
            if node.kind != BlockKind::Table {
                continue;
            }
            if self.starts_at(leaf, id) && self.covers_leaf(id, hi, to) {
                return Some(id);
            }
        }
        None
    }

    fn covered_footnote(&self, leaf: NodeId, hi: BlockId, to: usize) -> Option<NodeId> {
        let mut up = self.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = up {
            let Some(node) = self.arena.get(id) else {
                break;
            };
            up = node.parent;
            if node.kind != BlockKind::FootnoteDefinition {
                continue;
            }
            if self.starts_at(leaf, id) && self.covers_leaf(id, hi, to) {
                return Some(id);
            }
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
                    out.push_str(display.get(rs..hi).unwrap_or(""));
                }
                i = j + 1;
                continue;
            }
            let lo = range.start.max(rs);
            let hi = range.end.min(re);
            if lo < hi {
                out.push_str(display.get(lo..hi).unwrap_or(""));
            }
            i += 1;
        }
        out
    }
}

fn push_piece(out: &mut String, piece: &str, blank: bool, skipped: &mut usize) {
    if piece.is_empty() {
        if !out.is_empty() {
            *skipped += 1;
        }
        return;
    }
    if !out.is_empty() {
        let want = usize::from(blank) + 1 + *skipped * 2;
        let have = out.len() - out.trim_end_matches('\n').len();
        for _ in have..want {
            out.push('\n');
        }
    }
    *skipped = 0;
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
