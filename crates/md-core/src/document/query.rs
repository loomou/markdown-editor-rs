use super::Document;
use super::arena::NodeId;
use super::change::{ChangeSet, DocChange};
use super::focus::{RevealedImage, RevealedMath};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use crate::inline::InlineRun;

impl Document {
    pub fn intern_len(&self) -> usize {
        self.intern.len()
    }

    pub fn is_full_replace(&self) -> bool {
        matches!(
            self.changes.changes.first(),
            Some(DocChange::DocumentReplaced)
        )
    }

    pub fn runs(&self, id: NodeId) -> &[InlineRun] {
        if let Some(f) = &self.focus
            && f.node == id
        {
            return f.runs.as_slice();
        }
        self.collapsed_runs(id)
    }

    pub fn leaf_snapshot(&self, id: NodeId) -> Option<std::sync::Arc<super::text::LeafSnapshot>> {
        self.arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .map(super::text::LeafText::snapshot)
    }

    pub fn layout_leaf_snapshot(
        &self,
        id: NodeId,
    ) -> Option<std::sync::Arc<super::text::LeafSnapshot>> {
        if self.focus.as_ref().is_some_and(|f| f.node == id) {
            let display = self.display(id).to_string();
            let runs = self.runs(id).to_vec();
            if display.is_empty() && runs.is_empty() {
                None
            } else {
                Some(std::sync::Arc::new(super::text::LeafSnapshot {
                    display,
                    runs,
                }))
            }
        } else {
            self.leaf_snapshot(id)
        }
    }

    pub fn extra(&self, id: NodeId) -> NodeExtra {
        self.arena
            .get(id)
            .map(|n| n.extra)
            .unwrap_or(NodeExtra::None)
    }

    pub fn link_dest(&self, id: u32) -> Option<&str> {
        self.links.get(id as usize).map(|l| l.dest.as_str())
    }

    pub fn link_at(&self, id: NodeId, offset: usize) -> Option<&str> {
        let off = offset as u32;
        let runs = self.runs(id);
        let run = runs
            .iter()
            .find(|r| r.display_range.start <= off && off < r.display_range.end)
            .or_else(|| {
                runs.iter()
                    .rev()
                    .find(|r| r.display_range.end == off && r.display_range.start < off)
            })?;
        if run.marks.is_image() {
            return None;
        }
        self.link_dest(run.link?)
    }

    pub fn lang(&self, id: u32) -> Option<&str> {
        self.langs.get(id as usize).map(|s| s.as_str())
    }

    pub(crate) fn footnote_label(&self, id: u32) -> Option<&str> {
        self.footnotes.get(id as usize).map(|s| s.as_str())
    }

    pub fn revealed_image(&self) -> Option<RevealedImage<'_>> {
        let f = self.focus.as_ref()?;
        let img = f.image.as_ref()?;
        Some(RevealedImage {
            block: f.node.index,
            display: img.display.clone(),
            link: img.link,
            dest: self.link_dest(img.link)?,
        })
    }

    pub fn revealed_math(&self) -> Option<RevealedMath<'_>> {
        let f = self.focus.as_ref()?;
        let m = f.math.as_ref()?;
        Some(RevealedMath {
            block: f.node.index,
            display: m.display.clone(),
            latex: m.latex.as_str(),
            display_math: m.display_math,
        })
    }

    pub fn display(&self, id: NodeId) -> &str {
        if let Some(f) = &self.focus
            && f.node == id
        {
            return f.display.as_str();
        }
        self.collapsed_display(id)
    }

    pub(crate) fn leaf_source(&self, id: NodeId) -> &str {
        self.arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .map(|l| l.source_str(&self.source))
            .unwrap_or("")
    }

    pub fn block_source(&self, id: NodeId) -> &str {
        self.leaf_source(id)
    }

    pub(crate) fn caret_text(&self, id: NodeId) -> &str {
        let kind = self.arena.get(id).map(|n| n.kind);
        if kind.is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::BlockSource) {
            self.leaf_source(id)
        } else {
            self.display(id)
        }
    }

    pub fn preorder(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            out.push(id);
            let mut child = self.arena.get(id).and_then(|node| node.last_child);
            while let Some(id) = child {
                stack.push(id);
                child = self.arena.get(id).and_then(|node| node.prev_sibling);
            }
        }
        out
    }

    pub fn take_changes(&mut self) -> ChangeSet {
        std::mem::replace(&mut self.changes, ChangeSet::empty(self.revision))
    }

    pub fn pending_changes(&self) -> &ChangeSet {
        &self.changes
    }

    pub(crate) fn changes_since(&self, start: usize, before_revision: u64) -> ChangeSet {
        debug_assert!(start <= self.changes.changes.len());
        ChangeSet {
            before_revision,
            after_revision: self.revision,
            changes: self
                .changes
                .changes
                .get(start..)
                .unwrap_or_default()
                .to_vec(),
        }
    }

    pub fn live_id(&self, index: BlockId) -> Option<NodeId> {
        self.arena.live_at(index)
    }

    pub fn kind(&self, index: BlockId) -> Option<BlockKind> {
        let id = self.live_id(index)?;
        self.arena.get(id).map(|n| n.kind)
    }

    pub fn text_of(&self, index: BlockId) -> Option<&str> {
        let id = self.live_id(index)?;
        Some(self.display(id))
    }

    pub(crate) fn collapsed_text_of(&self, index: BlockId) -> Option<&str> {
        let id = self.live_id(index)?;
        Some(self.collapsed_display(id))
    }

    pub fn for_each_text_leaf(&self, visit: impl FnMut(BlockId, &str) -> bool) {
        self.for_each_leaf_text(false, visit);
    }

    pub fn for_each_collapsed_text_leaf(&self, visit: impl FnMut(BlockId, &str) -> bool) {
        self.for_each_leaf_text(true, visit);
    }

    fn for_each_leaf_text(&self, collapsed: bool, mut visit: impl FnMut(BlockId, &str) -> bool) {
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            let Some(node) = self.arena.get(id) else {
                continue;
            };
            let kind_leaf = node.kind.is_text_leaf();
            let child = node.last_child;
            if kind_leaf {
                let text = if collapsed {
                    self.collapsed_display(id)
                } else {
                    self.display(id)
                };
                if !visit(id.index, text) {
                    return;
                }
            }
            let mut child = child;
            while let Some(c) = child {
                stack.push(c);
                child = self.arena.get(c).and_then(|n| n.prev_sibling);
            }
        }
    }

    pub fn text_leaves(&self) -> Vec<BlockId> {
        let mut out = Vec::new();
        self.for_each_text_leaf(|id, _| {
            out.push(id);
            true
        });
        out
    }

    pub fn for_each_extra(&self, mut visit: impl FnMut(BlockId, NodeExtra)) {
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            let Some(node) = self.arena.get(id) else {
                continue;
            };
            visit(id.index, node.extra);
            let mut child = node.last_child;
            while let Some(c) = child {
                stack.push(c);
                child = self.arena.get(c).and_then(|n| n.prev_sibling);
            }
        }
    }
}
