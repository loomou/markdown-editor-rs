use super::{Document, NodeId};
use crate::block::BlockId;
use std::cmp::Ordering;

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

    pub fn cmp_reading_order(&self, a: BlockId, b: BlockId) -> Ordering {
        let (Some(a_id), Some(b_id)) = (self.live_id(a), self.live_id(b)) else {
            return a.cmp(&b);
        };
        if a_id == b_id {
            return Ordering::Equal;
        }
        let chain = |from: NodeId| {
            let mut out = Vec::new();
            let mut cur = Some(from);
            while let Some(id) = cur {
                out.push(id);
                cur = self.arena.get(id).and_then(|n| n.parent);
            }
            out
        };
        let a_chain = chain(a_id);
        let b_chain = chain(b_id);
        let (mut i, mut j) = (a_chain.len(), b_chain.len());
        while i > 0 && j > 0 && a_chain[i - 1] == b_chain[j - 1] {
            i -= 1;
            j -= 1;
        }
        if i == 0 {
            return Ordering::Less;
        }
        if j == 0 {
            return Ordering::Greater;
        }
        let (fa, fb) = (a_chain[i - 1], b_chain[j - 1]);
        let mut cur = fa;
        while let Some(next) = self.arena.get(cur).and_then(|n| n.next_sibling) {
            if next == fb {
                return Ordering::Less;
            }
            cur = next;
        }
        Ordering::Greater
    }
}
