use super::super::Document;
use super::super::arena::NodeId;
use super::{Caret, Sel};
use crate::block::BlockKind;

pub(crate) struct Path {
    pub leaf: NodeId,
    pub offset: usize,
    pub item: Option<NodeId>,
    pub list: Option<NodeId>,
}

impl Path {
    pub(crate) fn at(doc: &Document, caret: Caret) -> Option<Self> {
        let leaf = doc.live_id(caret.block)?;
        let mut item = None;
        let mut list = None;
        let mut walk = doc.arena.get(leaf).and_then(|n| n.parent);
        while let Some(id) = walk {
            let node = doc.arena.get(id)?;
            if node.kind == BlockKind::ListItem && item.is_none() {
                item = Some(id);
            } else if node.kind == BlockKind::List && item.is_some() {
                list = Some(id);
                break;
            }
            walk = node.parent;
        }
        Some(Path {
            leaf,
            offset: caret.offset,
            item,
            list,
        })
    }

    pub(crate) fn simple_empty_item(&self, doc: &Document) -> bool {
        self.sole_item_leaf(doc) == Some(false)
    }

    pub(crate) fn item_direct_leaf(&self, doc: &Document) -> bool {
        let Some(item) = self.item else {
            return false;
        };
        self.list.is_some() && doc.arena.get(self.leaf).and_then(|n| n.parent) == Some(item)
    }

    fn sole_item_leaf(&self, doc: &Document) -> Option<bool> {
        let item = self.item?;
        self.list?;
        let mut n = 0;
        let mut only = None;
        for c in doc.arena.children(item) {
            n += 1;
            only = Some(c);
            if n > 1 {
                return None;
            }
        }
        if only != Some(self.leaf) {
            return None;
        }
        Some(!doc.display(self.leaf).is_empty())
    }

    pub(crate) fn is_item_lead(&self, doc: &Document) -> bool {
        let Some(item) = self.item else {
            return false;
        };
        doc.arena.get(item).and_then(|n| n.first_child) == Some(self.leaf)
    }

    pub(crate) fn prev_item(&self, doc: &Document) -> Option<NodeId> {
        let item = self.item?;
        let prev = doc.arena.get(item)?.prev_sibling?;
        doc.arena
            .get(prev)
            .filter(|n| n.kind == BlockKind::ListItem)
            .map(|_| prev)
    }
}

pub(crate) fn item_span(doc: &Document, sel: Sel) -> Option<(NodeId, Vec<NodeId>)> {
    let a = Path::at(doc, sel.anchor)?;
    let b = Path::at(doc, sel.head)?;
    let list = a.list?;
    let item_a = a.item?;
    let item_b = b.item?;
    if b.list != Some(list) {
        return None;
    }
    let kids: Vec<NodeId> = doc.arena.children(list).collect();
    let ia = kids.iter().position(|&id| id == item_a)?;
    let ib = kids.iter().position(|&id| id == item_b)?;
    let (lo, hi) = if ia <= ib { (ia, ib) } else { (ib, ia) };
    Some((list, kids[lo..=hi].to_vec()))
}
