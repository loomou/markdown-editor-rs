use super::arena::NodeId;
use super::change::{ChangeSet, DocChange};
use super::chars::floor_char_boundary;
use super::edit::Caret;
use super::word::prev_grapheme_boundary;
use super::{Document, bind, editor_options, load_markdown, syntax, text};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use crate::inline::InlineRun;
use std::ops::Range;

impl Document {
    pub(super) fn remap_runs(&mut self, frag: &Document, leaf: NodeId) -> Vec<InlineRun> {
        frag.runs(leaf)
            .iter()
            .map(|r| InlineRun {
                display_range: r.display_range.clone(),
                source_range: r.source_range.clone(),
                marks: r.marks,
                link: r.link.map(|i| self.remap_link(frag, i)),
            })
            .collect()
    }

    pub(super) fn project_phrasing(
        &mut self,
        id: NodeId,
        source: String,
        caret_src: usize,
        range: Range<u32>,
        deleted: String,
        inserted: String,
    ) -> (DocChange, usize) {
        self.reproject(id, source, caret_src, (range, deleted, inserted), true)
    }

    fn reproject(
        &mut self,
        id: NodeId,
        source: String,
        caret_src: usize,
        edit: (Range<u32>, String, String),
        focus: bool,
    ) -> (DocChange, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let current = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let frag = load_markdown(&source, editor_options());
        let parsed_kind = match current {
            BlockKind::TableCell => BlockKind::Paragraph,
            kind => kind,
        };
        let (display, runs, s2d, constructs) =
            if let Some(leaf) = bind::matching_leaf(&frag, parsed_kind) {
                let display = frag.display(leaf).to_string();
                let runs = self.remap_runs(&frag, leaf);
                let s2d = if current == BlockKind::TableCell {
                    bind::source_to_display_map_for_kind(&source, current)
                } else {
                    frag.collapsed_s2d(leaf)
                };

                let constructs = frag.recorded_constructs(leaf).map(|c| c.to_vec());
                let (display, s2d) = bind::restore_visible_ws(current, &source, display, s2d);
                (display, runs, s2d, constructs)
            } else {
                let n = source.len();

                (
                    source.clone(),
                    bind::identity_runs(n),
                    bind::identity_map(n),
                    Some(Vec::new()),
                )
            };
        let caret_src = caret_src.min(source.len());
        let caret = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::source_to_display(&s2d, caret_src)
        } else {
            caret_src.min(display.len())
        };
        let caret = floor_char_boundary(&display, caret);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.set_projected(display, source, runs, constructs);
            leaf.s2d = s2d;
        }
        let caret = if focus {
            self.apply_inline_focus(id, caret, Some(caret_src), super::FocusBias::Neutral, true)
        } else {
            caret
        };
        let new_revision = self.bump_content(id);
        let (range, deleted, inserted) = edit;
        (
            DocChange::text(id, old_revision, new_revision, range, deleted, inserted),
            caret,
        )
    }

    pub(super) fn reproject_as_paragraph(&mut self, id: NodeId) -> DocChange {
        let source = self.leaf_source(id).to_string();
        let n = source.len() as u32;
        if let Some(n) = self.arena.get_mut(id) {
            n.kind = BlockKind::Paragraph;
            n.extra = NodeExtra::None;
        }
        self.reproject(id, source.clone(), 0, (0..n, source.clone(), source), false)
            .0
    }

    fn apply_source_edit(
        &mut self,
        id: NodeId,
        s0: usize,
        s1: usize,
        s: &str,
    ) -> (DocChange, usize) {
        let mut source = self.leaf_source(id).to_string();
        let s0 = floor_char_boundary(&source, s0.min(source.len()));
        let s1 = floor_char_boundary(&source, s1.min(source.len())).max(s0);
        let deleted = source.get(s0..s1).unwrap_or("").to_string();
        let inserted = s.to_string();
        let range = s0 as u32..s1 as u32;
        source.replace_range(s0..s1, s);
        let caret_src = (s0 + s.len()).min(source.len());
        self.project_phrasing(id, source, caret_src, range, deleted, inserted)
    }

    fn display_range_to_source(
        display_len: usize,
        source_len: usize,
        s2d: &[usize],
        start: usize,
        end: usize,
    ) -> (usize, usize) {
        let mapped = s2d.last().copied().unwrap_or(0) == display_len;
        if start == end && start == display_len && !mapped {
            (source_len, source_len)
        } else if start == end {
            let a = bind::display_to_source_inner(s2d, start);
            (a, a)
        } else {
            let mut a = bind::display_to_source_inner(s2d, start);
            let mut b = if end == display_len {
                bind::display_to_source_outer(s2d, end)
            } else {
                bind::display_to_source_inner(s2d, end)
            };
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b)
        }
    }

    fn ensure_heading_delimiter(source: &mut String, kind: BlockKind, display: &str) -> bool {
        matches!(kind, BlockKind::Heading(_))
            && display != source.as_str()
            && !source.is_empty()
            && source.bytes().all(|b| b == b'#')
            && {
                source.push(' ');
                true
            }
    }

    fn rewrite_phrasing(&mut self, id: NodeId, range: Range<usize>, s: &str) -> (DocChange, usize) {
        let display = self.display(id).to_string();
        let start = floor_char_boundary(&display, range.start.min(display.len()));
        let end = floor_char_boundary(&display, range.end.min(display.len())).max(start);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let mut source = self.leaf_source(id).to_string();
        let mut s2d = self.visual_s2d(id);

        let padded = Self::ensure_heading_delimiter(&mut source, kind, &display);
        if padded {
            s2d.push(s2d.last().copied().unwrap_or(0));

            if let Some(leaf) = self.texts.get_mut(id.text_id()) {
                leaf.set_owned_source(source.clone());
            }
        }
        let (s0, s1) = if start == 0 && end == display.len() && s.is_empty() {
            let keep = if matches!(kind, BlockKind::Heading(_)) {
                bind::display_to_source_inner(&s2d, 0)
            } else {
                0
            };
            (keep, source.len())
        } else {
            Self::display_range_to_source(display.len(), source.len(), &s2d, start, end)
        };
        let (mut change, caret) = self.apply_source_edit(id, s0, s1, s);

        if padded
            && let DocChange::TextChanged {
                range, inserted, ..
            } = &mut change
            && range.start == range.end
        {
            range.start -= 1;
            range.end -= 1;
            inserted.insert(0, ' ');
        }
        (change, caret)
    }

    fn rewrite_literal(&mut self, id: NodeId, range: Range<usize>, s: &str) -> (DocChange, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let display = self.display(id).to_string();
        let start = floor_char_boundary(&display, range.start.min(display.len()));
        let end = floor_char_boundary(&display, range.end.min(display.len())).max(start);
        let deleted = display.get(start..end).unwrap_or("").to_string();
        let inserted = s.to_string();
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.replace_display(start..end, s);
            leaf.source = text::LeafSource::SameAsDisplay;
        }
        let new_revision = self.bump_content(id);
        (
            DocChange::text(
                id,
                old_revision,
                new_revision,
                start as u32..end as u32,
                deleted,
                inserted,
            ),
            start + s.len(),
        )
    }

    fn rewrite_block_source(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
    ) -> (DocChange, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let mut source = self.leaf_source(id).to_string();
        let start = floor_char_boundary(&source, range.start.min(source.len()));
        let end = floor_char_boundary(&source, range.end.min(source.len())).max(start);
        let deleted = source.get(start..end).unwrap_or("").to_string();
        let inserted = s.to_string();
        source.replace_range(start..end, s);

        let frag = load_markdown(&source, editor_options());
        let (display, runs, extra, constructs) = match bind::matching_leaf(&frag, BlockKind::Image)
        {
            Some(leaf) => {
                let display = frag.display(leaf).to_string();
                let runs = self.remap_runs(&frag, leaf);
                let extra = match frag.extra(leaf).image_dest() {
                    Some(i) => NodeExtra::Image {
                        dest: self.remap_link(&frag, i),
                        source: None,
                    },
                    None => NodeExtra::None,
                };
                let constructs = frag.recorded_constructs(leaf).map(|c| c.to_vec());
                (display, runs, extra, constructs)
            }
            None => (String::new(), Vec::new(), NodeExtra::None, Some(Vec::new())),
        };
        if let Some(n) = self.arena.get_mut(id) {
            n.extra = extra;
        }

        let s2d = bind::identity_map(source.len());
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.set_projected(display, source, runs, constructs);
            leaf.s2d = s2d;
        }
        let new_revision = self.bump_content(id);
        (
            DocChange::text(
                id,
                old_revision,
                new_revision,
                start as u32..end as u32,
                deleted,
                inserted,
            ),
            start + s.len(),
        )
    }

    fn delete_phrasing_back(&mut self, id: NodeId, display_off: usize) -> (DocChange, usize) {
        let display = self.display(id).to_string();
        let source = self.leaf_source(id).to_string();
        let s2d = self.visual_s2d(id);
        let s_caret = bind::display_to_source_outer(&s2d, display_off);
        let s_caret = floor_char_boundary(&source, s_caret.min(source.len()));
        if self.arena.get(id).map(|node| node.kind) == Some(BlockKind::TableCell)
            && let Some(range) = bind::html_line_break_before(&source, s_caret)
        {
            return self.apply_source_edit(id, range.start, range.end, "");
        }
        if s_caret == 0 {
            let prev = prev_grapheme_boundary(&display, display_off);
            return self.rewrite_phrasing(id, prev..display_off, "");
        }
        let s_prev = prev_grapheme_boundary(&source, s_caret);
        self.apply_source_edit(id, s_prev, s_caret, "")
    }

    pub(crate) fn rewrite_text(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
    ) -> (DocChange, usize) {
        self.ensure_leaf_text(id);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => self.rewrite_phrasing(id, range, s),
            TextEditStrategy::BlockSource => self.rewrite_block_source(id, range, s),
            TextEditStrategy::Literal => self.rewrite_literal(id, range, s),
        }
    }

    pub(crate) fn append_leaf_source(
        &mut self,
        target: NodeId,
        tail: NodeId,
    ) -> (DocChange, usize) {
        if self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.node == target || focus.node == tail)
        {
            self.focus = None;
        }
        let join_at = self.collapsed_display(target).len();
        let inserted = self.leaf_source(tail).to_string();
        let strategy = self
            .arena
            .get(target)
            .map(|node| node.kind.text_edit_strategy())
            .unwrap_or(TextEditStrategy::Literal);
        let change = match strategy {
            TextEditStrategy::Phrasing => {
                let mut source = self.leaf_source(target).to_string();
                let at = source.len();
                source.push_str(&inserted);
                self.reproject(
                    target,
                    source,
                    at + inserted.len(),
                    (at as u32..at as u32, String::new(), inserted),
                    false,
                )
                .0
            }
            TextEditStrategy::BlockSource => {
                let at = self.leaf_source(target).len();
                self.rewrite_block_source(target, at..at, &inserted).0
            }
            TextEditStrategy::Literal => {
                self.rewrite_literal(target, join_at..join_at, &inserted).0
            }
        };
        (change, join_at)
    }

    pub(crate) fn break_literal_leaf(&mut self, id: NodeId, offset: usize) -> Caret {
        let text = self.caret_text(id).to_string();
        let off = floor_char_boundary(&text, offset.min(text.len()));
        let can_exit_fence = self
            .arena
            .get(id)
            .is_some_and(|node| matches!(node.kind, BlockKind::CodeBlock | BlockKind::Mermaid));
        if can_exit_fence && let Some(range) = syntax::close_fence_delete_range(&text, off) {
            return self.exit_fence(id, range);
        }
        let before = self.revision;
        let (change, caret) = self.rewrite_text(id, off..off, "\n");
        let _ = self.commit(before, vec![change]);
        Caret {
            block: id.index,
            offset: caret,
        }
    }

    fn exit_fence(&mut self, id: NodeId, range: Range<usize>) -> Caret {
        let parent = self.arena.get(id).and_then(|n| n.parent).expect("parent");
        let before = self.revision;
        let (text_changed, _) = self.rewrite_text(id, range, "");
        let para = self.alloc_leaf(BlockKind::Paragraph);
        self.arena.insert_after(parent, Some(id), para);
        self.bump_structure(parent);
        let _ = self.commit(
            before,
            vec![
                text_changed,
                DocChange::TreeSpliced {
                    parent,
                    before: Some(id),
                    removed: Vec::new(),
                    inserted: vec![para],
                },
            ],
        );
        Caret {
            block: para.index,
            offset: 0,
        }
    }

    pub fn replace_text(&mut self, index: BlockId, range: Range<usize>, s: &str) -> ChangeSet {
        let Some(id) = self.live_id(index) else {
            return ChangeSet::empty(self.revision);
        };
        if self.math_edit_would_close(id, range.clone(), s) {
            return ChangeSet::empty(self.revision);
        }
        let before = self.revision;
        let (change, _) = self.rewrite_text(id, range, s);
        self.commit(before, vec![change])
    }

    pub(super) fn math_edit_would_close(
        &self,
        id: NodeId,
        _range: Range<usize>,
        inserted: &str,
    ) -> bool {
        if self.arena.get(id).map(|node| node.kind) != Some(BlockKind::Math) {
            return false;
        }

        inserted.contains('$')
    }

    pub(super) fn last_text_caret(&self, id: NodeId) -> Option<(BlockId, usize)> {
        let mut last = None;
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if matches!(
                self.arena.get(id).map(|n| n.kind),
                Some(k) if k.is_text_leaf()
            ) {
                last = Some((id.index, self.caret_text(id).len()));
            }
            let mut child = self.arena.get(id).and_then(|node| node.last_child);
            while let Some(id) = child {
                stack.push(id);
                child = self.arena.get(id).and_then(|node| node.prev_sibling);
            }
        }
        last
    }

    pub(crate) fn delete_back(&mut self, index: BlockId, offset: usize) -> (ChangeSet, usize) {
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), offset);
        };
        let kind = self.arena.get(id).map(|n| n.kind);
        let text = self.caret_text(id);
        let off = floor_char_boundary(text, offset.min(text.len()));
        if off == 0 {
            return (ChangeSet::empty(self.revision), off);
        }
        let before = self.revision;
        let (change, caret) =
            if kind.is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::Phrasing) {
                self.delete_phrasing_back(id, off)
            } else {
                let prev = prev_grapheme_boundary(text, off);
                self.rewrite_text(id, prev..off, "")
            };
        (self.commit(before, vec![change]), caret)
    }

    pub(super) fn apply_recorded_text(&mut self, id: NodeId, range: Range<u32>, inserted: &str) {
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let start = range.start as usize;
        let end = range.end as usize;
        match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => {
                let _ = self.apply_source_edit(id, start, end, inserted);
            }
            TextEditStrategy::BlockSource => {
                let _ = self.rewrite_block_source(id, start..end, inserted);
            }
            TextEditStrategy::Literal => {
                let _ = self.rewrite_literal(id, start..end, inserted);
            }
        }
    }

    pub(super) fn reproject_current(&mut self, id: NodeId) {
        let source = self.leaf_source(id).to_string();
        let n = source.len() as u32;
        let caret = source.len();
        let _ = self.reproject(
            id,
            source.clone(),
            caret,
            (0..n, source.clone(), source),
            false,
        );
    }

    pub(crate) fn split_leaf_nodes(&mut self, id: NodeId, offset: usize) -> (DocChange, NodeId) {
        let node = self.arena.get(id).expect("live node");
        let kind = node.kind;
        let new_kind = match kind {
            BlockKind::Heading(_) => BlockKind::Paragraph,
            k => k,
        };
        let parent = node.parent.expect("leaf parent");
        let display = self.display(id).to_string();
        let source = self.leaf_source(id).to_string();
        let off = floor_char_boundary(&display, offset.min(display.len()));
        let s2d = self.visual_s2d(id);
        let src_off = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::display_to_source_inner(&s2d, off)
        } else {
            off.min(source.len())
        };
        let src_off = floor_char_boundary(&source, src_off.min(source.len()));
        let head_s = source[..src_off].to_string();
        let tail_s = source[src_off..].to_string();
        let tail_d = display[off..].to_string();
        let head_len = head_s.len();
        let src_end = source.len() as u32;
        let text_changed = if kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            self.project_phrasing(
                id,
                head_s,
                head_len,
                src_off as u32..src_end,
                tail_s.clone(),
                String::new(),
            )
            .0
        } else {
            let (change, _) = self.rewrite_literal(id, off..display.len(), "");
            change
        };
        let new_id = self.alloc_leaf(new_kind);
        if new_kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            let _ = self.project_phrasing(new_id, tail_s.clone(), 0, 0..0, String::new(), tail_s);
        } else if let Some(leaf) = self.texts.get_mut(new_id.text_id()) {
            leaf.replace_display(0..0, &tail_d);
            leaf.set_owned_source(tail_s);
        }
        self.arena.insert_after(parent, Some(id), new_id);
        (text_changed, new_id)
    }

    pub fn split_leaf(&mut self, index: BlockId, offset: usize) -> (ChangeSet, BlockId) {
        let before = self.revision;
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), index);
        };
        let Some(node) = self.arena.get(id) else {
            return (ChangeSet::empty(self.revision), index);
        };
        if !node.kind.is_text_leaf() || node.kind == BlockKind::Image {
            return (ChangeSet::empty(self.revision), index);
        }
        let parent = node.parent.expect("leaf parent");
        let (text_changed, new_id) = self.split_leaf_nodes(id, offset);
        self.bump_structure(parent);
        let set = self.commit(
            before,
            vec![
                text_changed,
                DocChange::TreeSpliced {
                    parent,
                    before: Some(id),
                    removed: Vec::new(),
                    inserted: vec![new_id],
                },
            ],
        );
        (set, new_id.index)
    }

    pub fn merge_into_prev(&mut self, index: BlockId) -> Option<(ChangeSet, BlockId, usize)> {
        let cur_id = self.live_id(index)?;
        if !self.arena.get(cur_id)?.kind.is_text_leaf() {
            return None;
        }
        let prev_id = self.prev_text_leaf(cur_id)?;
        let prev = prev_id.index;
        if matches!(self.kind(prev), Some(BlockKind::TableCell))
            || matches!(self.kind(index), Some(BlockKind::TableCell))
        {
            return None;
        }
        let before = self.revision;
        let parent = self.arena.get(cur_id)?.parent?;
        let before_sibling = self.arena.get(cur_id)?.prev_sibling;
        let focused = self.focus.as_ref().map(|focus| focus.node);
        let focus_change = if focused == Some(prev_id) {
            self.focus = None;
            Some(self.focus_text_change(prev_id))
        } else {
            if focused == Some(cur_id) {
                self.focus = None;
            }
            None
        };
        let old_revision = self.arena.get(prev_id)?.content_revision;
        let tail_src = self.leaf_source(cur_id).to_string();
        let strategy = self
            .arena
            .get(prev_id)
            .map(|node| node.kind.text_edit_strategy())
            .unwrap_or(TextEditStrategy::Literal);
        let tail = match strategy {
            TextEditStrategy::Phrasing => self.collapsed_display(cur_id).to_string(),
            TextEditStrategy::BlockSource | TextEditStrategy::Literal => tail_src.clone(),
        };
        let mut joined = self.leaf_source(prev_id).to_string();

        let prev_kind = self.arena.get(prev_id)?.kind;
        let padded =
            Self::ensure_heading_delimiter(&mut joined, prev_kind, self.collapsed_display(prev_id));
        let src_at = joined.len();
        let join_at = self.collapsed_display(prev_id).len();
        joined.push_str(&tail_src);
        if let Some(leaf) = self.texts.get_mut(prev_id.text_id()) {
            let end = leaf.display().len();
            leaf.replace_display(end..end, &tail);
            leaf.set_owned_source(joined);
        }
        let new_revision = self.bump_content(prev_id);
        self.arena.snapshot(cur_id);
        self.arena.detach(cur_id);
        self.arena.tombstone(cur_id);
        self.bump_structure(parent);
        let change_at = match strategy {
            TextEditStrategy::Literal => join_at,
            TextEditStrategy::Phrasing | TextEditStrategy::BlockSource => src_at,
        };
        let mut changes = Vec::with_capacity(3);
        if let Some(change) = focus_change {
            changes.push(change);
        }
        let (change_at, recorded) = if padded {
            (change_at - 1, format!(" {tail_src}"))
        } else {
            (change_at, tail_src)
        };
        changes.push(DocChange::text(
            prev_id,
            old_revision,
            new_revision,
            change_at as u32..change_at as u32,
            String::new(),
            recorded,
        ));
        changes.push(DocChange::TreeSpliced {
            parent,
            before: before_sibling,
            removed: vec![cur_id],
            inserted: Vec::new(),
        });
        let set = self.commit(before, changes);
        let caret = match strategy {
            TextEditStrategy::BlockSource => src_at,
            TextEditStrategy::Phrasing | TextEditStrategy::Literal => join_at,
        };
        Some((set, prev, caret))
    }

    pub fn merge_into_next(&mut self, index: BlockId) -> Option<(ChangeSet, BlockId, usize)> {
        let cur_id = self.live_id(index)?;
        let (cur_kind, parent, next_id, old_revision) = {
            let cur_node = self.arena.get(cur_id)?;
            (
                cur_node.kind,
                cur_node.parent?,
                cur_node.next_sibling?,
                cur_node.content_revision,
            )
        };
        let next_kind = self.arena.get(next_id)?.kind;
        if !cur_kind.is_text_leaf() {
            return None;
        }
        if !next_kind.is_text_leaf()
            || matches!(cur_kind, BlockKind::TableCell)
            || matches!(next_kind, BlockKind::TableCell)
            || self.arena.get(next_id).and_then(|node| node.parent) != Some(parent)
        {
            return None;
        }
        let before = self.revision;
        let focused = self.focus.as_ref().map(|focus| focus.node);
        let focus_change = if focused == Some(cur_id) {
            self.focus = None;
            Some(self.focus_text_change(cur_id))
        } else {
            if focused == Some(next_id) {
                self.focus = None;
            }
            None
        };
        let tail_src = self.leaf_source(next_id).to_string();
        let strategy = cur_kind.text_edit_strategy();
        let tail = match strategy {
            TextEditStrategy::Phrasing => self.collapsed_display(next_id).to_string(),
            TextEditStrategy::BlockSource | TextEditStrategy::Literal => tail_src.clone(),
        };
        let mut joined = self.leaf_source(cur_id).to_string();

        let padded =
            Self::ensure_heading_delimiter(&mut joined, cur_kind, self.collapsed_display(cur_id));
        let src_at = joined.len();
        let join_at = self.collapsed_display(cur_id).len();
        joined.push_str(&tail_src);
        if let Some(leaf) = self.texts.get_mut(cur_id.text_id()) {
            let end = leaf.display().len();
            leaf.replace_display(end..end, &tail);
            leaf.set_owned_source(joined);
        }
        let new_revision = self.bump_content(cur_id);
        self.arena.snapshot(next_id);
        self.arena.detach(next_id);
        self.arena.tombstone(next_id);
        self.bump_structure(parent);
        let change_at = match strategy {
            TextEditStrategy::Literal => join_at,
            TextEditStrategy::Phrasing | TextEditStrategy::BlockSource => src_at,
        };
        let mut changes = Vec::with_capacity(3);
        if let Some(change) = focus_change {
            changes.push(change);
        }
        let (change_at, recorded) = if padded {
            (change_at - 1, format!(" {tail_src}"))
        } else {
            (change_at, tail_src)
        };
        changes.push(DocChange::text(
            cur_id,
            old_revision,
            new_revision,
            change_at as u32..change_at as u32,
            String::new(),
            recorded,
        ));
        changes.push(DocChange::TreeSpliced {
            parent,
            before: Some(cur_id),
            removed: vec![next_id],
            inserted: Vec::new(),
        });
        let set = self.commit(before, changes);
        let caret = match strategy {
            TextEditStrategy::BlockSource => src_at,
            TextEditStrategy::Phrasing | TextEditStrategy::Literal => join_at,
        };
        Some((set, cur_id.index, caret))
    }
}
