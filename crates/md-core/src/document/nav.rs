use super::{Document, NodeId};
use crate::block::BlockId;

impl Document {
    fn preorder_next(&self, id: NodeId) -> Option<NodeId> {
        let node = self.arena.get(id)?;
        if let Some(first) = node.first_child {
            return Some(first);
        }

        let mut cur = id;
        loop {
            if cur == self.root {
                return None;
            }
            let n = self.arena.get(cur)?;
            if let Some(next) = n.next_sibling {
                return Some(next);
            }
            cur = n.parent?;
        }
    }

    pub(crate) fn preorder_prev(&self, id: NodeId) -> Option<NodeId> {
        if id == self.root {
            return None;
        }
        let node = self.arena.get(id)?;

        if let Some(prev) = node.prev_sibling {
            let mut cur = prev;
            while let Some(last) = self.arena.get(cur).and_then(|n| n.last_child) {
                cur = last;
            }
            return Some(cur);
        }
        node.parent
    }

    pub(crate) fn next_text_leaf(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = id;
        loop {
            cur = self.preorder_next(cur)?;
            if self.arena.get(cur)?.kind.is_text_leaf() {
                return Some(cur);
            }
        }
    }

    pub(crate) fn prev_text_leaf(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = id;
        loop {
            cur = self.preorder_prev(cur)?;
            if self.arena.get(cur)?.kind.is_text_leaf() {
                return Some(cur);
            }
        }
    }

    pub fn first_text_leaf(&self) -> Option<BlockId> {
        if self.arena.get(self.root)?.kind.is_text_leaf() {
            return Some(self.root.index);
        }
        self.next_text_leaf(self.root).map(|id| id.index)
    }

    pub fn nth_text_leaf_from(&self, from: BlockId, delta: i32) -> Option<BlockId> {
        let mut cur = self.live_id(from)?;
        if !self.arena.get(cur)?.kind.is_text_leaf() {
            return None;
        }
        for _ in 0..delta.unsigned_abs() {
            cur = if delta < 0 {
                self.prev_text_leaf(cur)?
            } else {
                self.next_text_leaf(cur)?
            };
        }
        Some(cur.index)
    }
}
