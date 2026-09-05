use super::RawConstruct;
use crate::document::bind::{display_to_source_first, display_to_source_inner, source_to_display};
use crate::document::{Caret, Document, NodeId};
use crate::inline::InlineRun;

impl Document {
    pub(crate) fn collapsed_display(&self, id: NodeId) -> &str {
        self.arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .map(|l| l.display())
            .unwrap_or("")
    }

    pub(crate) fn recorded_constructs(&self, id: NodeId) -> Option<&[RawConstruct]> {
        self.arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .and_then(|l| l.constructs.as_deref())
    }

    pub(crate) fn collapsed_runs(&self, id: NodeId) -> &[InlineRun] {
        self.arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .map(|l| l.runs())
            .unwrap_or(&[])
    }

    pub(crate) fn collapsed_s2d(&self, id: NodeId) -> Vec<usize> {
        let Some(leaf) = self
            .arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
        else {
            return Vec::new();
        };
        if leaf.s2d.len() == leaf.source_str(&self.source).len() + 1
            && leaf.s2d.last().copied() == Some(leaf.display().len())
        {
            return leaf.s2d.clone();
        }
        let kind = self
            .arena
            .get(id)
            .map(|node| node.kind)
            .unwrap_or(crate::block::BlockKind::Paragraph);
        super::bind::bind_map(
            leaf.source_str(&self.source),
            leaf.snapshot.display.as_str(),
            kind,
        )
        .1
    }

    pub(crate) fn visual_s2d(&self, id: NodeId) -> Vec<usize> {
        let source_len = self
            .arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .map(|l| l.source_str(&self.source).len());
        if let Some(f) = &self.focus
            && f.node == id
            && Some(f.s2d.len()) == source_len.map(|n| n + 1)
            && f.s2d.last().copied() == Some(f.display.len())
        {
            return f.s2d.clone();
        }
        self.collapsed_s2d(id)
    }

    pub(crate) fn collapsed_to_visual(&self, id: NodeId, collapsed: usize) -> usize {
        if self.focus.as_ref().is_none_or(|f| f.node != id) {
            return collapsed;
        }
        let src = display_to_source_inner(&self.collapsed_s2d(id), collapsed);
        source_to_display(&self.visual_s2d(id), src)
    }

    pub(crate) fn collapsed_range_to_visual(
        &self,
        id: NodeId,
        range: std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        if self.focus.as_ref().is_none_or(|f| f.node != id) {
            return range;
        }
        let collapsed_s2d = self.collapsed_s2d(id);
        let src_start = display_to_source_inner(&collapsed_s2d, range.start);
        let src_end = display_to_source_first(&collapsed_s2d, range.end);
        let visual_s2d = self.visual_s2d(id);
        source_to_display(&visual_s2d, src_start)..source_to_display(&visual_s2d, src_end)
    }

    pub(crate) fn visual_caret_to_collapsed(&self, caret: Caret) -> Caret {
        let Some(id) = self.live_id(caret.block) else {
            return caret;
        };
        if self.focus.as_ref().is_none_or(|focus| focus.node != id) {
            return caret;
        }
        let visual = self.display(id);
        let visual_at = super::floor_char_boundary(visual, caret.offset.min(visual.len()));
        let source_at = display_to_source_inner(&self.visual_s2d(id), visual_at);
        Caret {
            block: caret.block,
            offset: source_to_display(&self.collapsed_s2d(id), source_at),
        }
    }
}
