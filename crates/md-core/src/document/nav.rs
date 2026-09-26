use super::{Caret, Document, NodeId, Sel};
use crate::block::{BlockId, BlockKind};
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

    pub fn whole_document_sel(&self) -> Option<Sel> {
        let mut leaves = self.text_leaves();
        if leaves.len() > 1
            && self.kind(leaves[leaves.len() - 1]) == Some(BlockKind::Paragraph)
            && self
                .live_id(leaves[leaves.len() - 1])
                .is_some_and(|id| self.caret_text(id).is_empty())
        {
            leaves.pop();
        }
        let first = *leaves.first()?;
        let last = *leaves.last()?;
        let anchor = self
            .textless_block_before(self.live_id(first)?)
            .unwrap_or(first);
        let head = self
            .textless_block_after(self.live_id(last)?)
            .unwrap_or(last);
        let head_offset = self
            .live_id(head)
            .map(|id| self.caret_text(id).len())
            .unwrap_or(0);
        Some(Sel {
            anchor: Caret {
                block: anchor,
                offset: 0,
            },
            head: Caret {
                block: head,
                offset: head_offset,
            },
        })
    }

    fn textless_block_before(&self, stop: NodeId) -> Option<BlockId> {
        let mut cur = self.root;
        while let Some(next) = self.preorder_next(cur) {
            if next == stop {
                return None;
            }
            if !self.subtree_has_text_leaf(next) {
                return Some(next.index);
            }
            cur = next;
        }
        None
    }

    fn textless_block_after(&self, start: NodeId) -> Option<BlockId> {
        let mut cur = self.preorder_next(start);
        while let Some(next) = cur {
            if !self.subtree_has_text_leaf(next) {
                return Some(next.index);
            }
            cur = self.preorder_next(next);
        }
        None
    }

    pub(crate) fn subtree_has_text_leaf(&self, id: NodeId) -> bool {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let Some(node) = self.arena.get(cur) else {
                continue;
            };
            if node.kind.is_text_leaf() {
                return true;
            }
            let mut child = node.first_child;
            while let Some(c) = child {
                stack.push(c);
                child = self.arena.get(c).and_then(|n| n.next_sibling);
            }
        }
        false
    }

    pub fn nth_text_leaf_from(&self, from: BlockId, delta: i32) -> Option<BlockId> {
        let mut cur = self.live_id(from)?;
        for _ in 0..delta.unsigned_abs() {
            cur = if delta < 0 {
                self.prev_text_leaf(cur)?
            } else {
                self.next_text_leaf(cur)?
            };
        }
        if !self.arena.get(cur)?.kind.is_text_leaf() {
            return None;
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
