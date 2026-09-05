use super::Document;
use super::arena::NodeId;
use super::change::{ChangeSet, DocChange};
use crate::block::{BlockKind, NodeExtra};

impl Document {
    pub(crate) fn set_extra(&mut self, id: NodeId, extra: NodeExtra) {
        if let Some(n) = self.arena.get_mut(id) {
            n.extra = extra;
        }
    }

    pub(crate) fn commit(&mut self, before: u64, changes: Vec<DocChange>) -> ChangeSet {
        self.revision += 1;
        let delta = ChangeSet {
            before_revision: before,
            after_revision: self.revision,
            changes: changes.clone(),
        };
        if self.changes.is_empty() {
            self.changes.before_revision = before;
        }
        self.changes.after_revision = self.revision;
        self.changes.changes.extend(changes);
        delta
    }

    pub(crate) fn push_change(&mut self, change: DocChange) {
        self.push_changes(vec![change]);
    }

    pub(crate) fn push_changes(&mut self, changes: Vec<DocChange>) {
        let before = self.revision;
        let _ = self.commit(before, changes);
    }

    pub(super) fn attrs_change(
        &self,
        id: NodeId,
        old_kind: BlockKind,
        old_extra: NodeExtra,
    ) -> DocChange {
        let kind = self.arena.get(id).map(|n| n.kind).unwrap_or(old_kind);
        let extra = self.arena.get(id).map(|n| n.extra).unwrap_or(old_extra);
        DocChange::attrs(id, old_kind, kind, old_extra, extra)
    }

    pub(crate) fn bump_structure(&mut self, id: NodeId) {
        if let Some(n) = self.arena.get_mut(id) {
            n.structure_revision = n.structure_revision.saturating_add(1);
        }
    }

    pub(super) fn bump_content(&mut self, id: NodeId) -> u64 {
        let Some(new_revision) = self.texts.get(id.text_id()).map(|l| l.revision) else {
            return self.arena.get(id).map_or(1, |n| n.content_revision);
        };
        if let Some(n) = self.arena.get_mut(id) {
            n.content_revision = new_revision;
        }
        self.max_content_revision = self.max_content_revision.max(new_revision);
        new_revision
    }
}
