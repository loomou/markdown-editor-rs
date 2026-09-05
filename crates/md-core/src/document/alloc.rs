use super::Document;
use super::arena::NodeId;
use crate::block::BlockKind;

impl Document {
    pub(super) fn alloc_leaf(&mut self, kind: BlockKind) -> NodeId {
        let id = self.arena.alloc(kind);
        self.texts.push_slot();
        self.texts.init_leaf(id.text_id());
        if let Some(n) = self.arena.get_mut(id) {
            n.text = Some(id.text_id());
        }
        id
    }

    pub(crate) fn alloc_container(&mut self, kind: BlockKind) -> NodeId {
        let id = self.arena.alloc(kind);
        self.texts.push_slot();
        id
    }

    pub(super) fn ensure_leaf_text(&mut self, id: NodeId) {
        let tid = id.text_id();
        if self.texts.get(tid).is_none() {
            self.texts.init_leaf(tid);
            if let Some(n) = self.arena.get_mut(id) {
                n.text = Some(tid);
            }
        }
    }
}
