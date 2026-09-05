use crate::block::{BlockId, BlockKind, TextEditStrategy};
use crate::document::{DocChange, Document, NodeId};

impl Document {
    pub fn block_edit(&self) -> Option<BlockId> {
        self.block_edit.map(|id| id.index)
    }

    pub(super) fn set_block_edit(&mut self, id: Option<NodeId>) {
        let prev = self.block_edit;
        self.block_edit = id;
        if let Some(prev) = prev.filter(|p| Some(*p) != id) {
            self.settle_block_source(prev);
        }
    }

    fn settle_block_source(&mut self, id: NodeId) {
        let kind = self.arena.get(id).map(|n| n.kind);
        if !kind.is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::BlockSource) {
            return;
        }
        let frag = super::load_markdown(self.leaf_source(id), super::editor_options());
        if super::bind::matching_leaf(&frag, crate::block::BlockKind::Image).is_some() {
            return;
        }
        let old_kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Image);
        let old_extra = self.extra(id);
        let change = self.reproject_as_paragraph(id);
        self.push_changes(vec![self.attrs_change(id, old_kind, old_extra), change]);
    }

    pub(super) fn emit_focus_text(&mut self, id: NodeId) {
        let change = self.focus_text_change(id);
        self.push_change(change);
    }

    pub(in crate::document) fn clear_inline_focus(&mut self) {
        if let Some(focus) = self.focus.take() {
            self.emit_focus_text(focus.node);
        }
    }

    pub(in crate::document) fn focus_text_change(&mut self, id: NodeId) -> DocChange {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.revision = leaf.revision.saturating_add(1).max(1);
        }
        let new_revision = self.bump_content(id);
        DocChange::text(
            id,
            old_revision,
            new_revision,
            0..0,
            String::new(),
            String::new(),
        )
    }
}
