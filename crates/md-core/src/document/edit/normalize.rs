use super::super::Document;
use super::super::arena::NodeId;
use super::super::change::DocChange;
use super::Caret;
use crate::block::{BlockId, BlockKind, NodeExtra};

pub(crate) fn tombstone(doc: &mut Document, id: NodeId) {
    doc.arena.detach(id);
    doc.arena.tombstone(id);
}

pub(crate) fn prune_empty_up(doc: &mut Document, start: NodeId, changes: &mut Vec<DocChange>) {
    let mut cur = Some(start);
    while let Some(id) = cur {
        let Some(node) = doc.arena.get(id) else {
            return;
        };
        let parent = node.parent;
        let prev = node.prev_sibling;
        let empty = matches!(node.kind, BlockKind::List | BlockKind::ListItem)
            && node.first_child.is_none();
        if !empty {
            return;
        }
        tombstone(doc, id);
        if let Some(p) = parent {
            changes.push(DocChange::TreeSpliced {
                parent: p,
                before: prev,
                removed: vec![id],
                inserted: Vec::new(),
            });
            doc.bump_structure(p);
        }
        cur = parent;
    }
}

pub(crate) fn sync_loose_up(doc: &mut Document, start: NodeId, changes: &mut Vec<DocChange>) {
    sync_loose_up_inner(doc, start, changes, None);
}

pub(crate) fn drop_covered_empty_quotes(
    doc: &mut Document,
    span: &[BlockId],
    from_start: bool,
    caret: Caret,
    changes: &mut Vec<DocChange>,
) -> Caret {
    let span_set: std::collections::HashSet<BlockId> = span.iter().copied().collect();
    let mut candidates: Vec<NodeId> = Vec::new();
    for &block in span {
        let Some(mut id) = doc.live_id(block) else {
            continue;
        };
        while let Some(node) = doc.arena.get(id) {
            if node.kind == BlockKind::BlockQuote && !candidates.contains(&id) {
                candidates.push(id);
            }
            let Some(parent) = node.parent else {
                break;
            };
            id = parent;
        }
    }
    candidates.sort_by_key(|&id| std::cmp::Reverse(quote_depth(doc, id)));
    if let (Some(&first), Some(&last)) = (span.first(), span.last()) {
        let mut seen = 0usize;
        let mut lo = None;
        let mut hi = None;
        for id in doc.preorder() {
            let Some(node) = doc.arena.get(id) else {
                continue;
            };
            if node.kind.is_text_leaf() {
                if id.index == first {
                    lo = Some(seen);
                }
                if id.index == last {
                    hi = Some(seen);
                }
                seen += 1;
            }
        }
        if let (Some(lo), Some(hi)) = (lo, hi) {
            let lo_bound = if from_start { 0 } else { lo + 1 };
            let mut seen = 0usize;
            for id in doc.preorder() {
                let Some(node) = doc.arena.get(id) else {
                    continue;
                };
                if node.kind.is_text_leaf() {
                    seen += 1;
                } else if node.kind == BlockKind::BlockQuote
                    && node.first_child.is_none()
                    && node.parent.is_some()
                    && seen >= lo_bound
                    && seen <= hi
                    && !candidates.contains(&id)
                {
                    candidates.push(id);
                }
            }
        }
    }
    let mut caret = caret;
    for quote in candidates {
        if doc.arena.get(quote).is_none() {
            continue;
        }
        let covered = doc
            .text_leaves()
            .into_iter()
            .all(|leaf| !is_under(doc, leaf, quote) || span_set.contains(&leaf));
        if !covered {
            continue;
        }
        caret = drop_one_empty_quote(doc, quote, caret, changes);
    }
    caret
}

fn quote_depth(doc: &Document, id: NodeId) -> usize {
    let mut depth = 0;
    let mut walk = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = walk {
        if doc
            .arena
            .get(p)
            .is_some_and(|n| n.kind == BlockKind::BlockQuote)
        {
            depth += 1;
        }
        walk = doc.arena.get(p).and_then(|n| n.parent);
    }
    depth
}

fn is_under(doc: &Document, block: BlockId, root: NodeId) -> bool {
    doc.live_id(block)
        .is_some_and(|id| under_node(doc, id, root))
}

fn under_node(doc: &Document, mut id: NodeId, root: NodeId) -> bool {
    loop {
        if id == root {
            return true;
        }
        match doc.arena.get(id).and_then(|n| n.parent) {
            Some(p) => id = p,
            None => return false,
        }
    }
}

fn drop_one_empty_quote(
    doc: &mut Document,
    quote: NodeId,
    caret: Caret,
    changes: &mut Vec<DocChange>,
) -> Caret {
    let Some(host) = doc.arena.get(quote).and_then(|n| n.parent) else {
        return caret;
    };
    let quote_prev = doc.arena.get(quote).and_then(|n| n.prev_sibling);
    if let Some(id) = doc.live_id(caret.block)
        && under_node(doc, id, quote)
        && doc.arena.get(id).is_some_and(|n| n.kind.is_text_leaf())
    {
        let leaf_parent = doc
            .arena
            .get(id)
            .and_then(|n| n.parent)
            .expect("a node under a quote always has a parent");
        let leaf_prev = doc.arena.get(id).and_then(|n| n.prev_sibling);
        doc.arena.detach(id);
        doc.arena.insert_after(host, Some(quote), id);
        doc.bump_structure(host);
        changes.push(DocChange::TreeSpliced {
            parent: leaf_parent,
            before: leaf_prev,
            removed: vec![id],
            inserted: Vec::new(),
        });
        changes.push(DocChange::TreeSpliced {
            parent: host,
            before: Some(quote),
            removed: Vec::new(),
            inserted: vec![id],
        });
    }
    let mut stack = vec![quote];
    while let Some(id) = stack.pop() {
        doc.arena.snapshot(id);
        stack.extend(doc.arena.children(id));
    }
    doc.arena.detach(quote);
    let mut stack = vec![(quote, false)];
    while let Some((id, visited)) = stack.pop() {
        if visited {
            doc.arena.tombstone(id);
            continue;
        }
        let children: Vec<NodeId> = doc.arena.children(id).collect();
        stack.push((id, true));
        stack.extend(children.into_iter().rev().map(|c| (c, false)));
    }
    doc.bump_structure(host);
    changes.push(DocChange::TreeSpliced {
        parent: host,
        before: quote_prev,
        removed: vec![quote],
        inserted: Vec::new(),
    });
    if doc.arena.get(host).is_some() {
        prune_empty_up(doc, host, changes);
        if doc.arena.get(host).is_some() {
            sync_loose_up(doc, host, changes);
        }
    }
    caret
}

pub(crate) fn sync_loose_after_join(
    doc: &mut Document,
    start: NodeId,
    joined_list: NodeId,
    changes: &mut Vec<DocChange>,
) {
    sync_loose_up_inner(doc, start, changes, Some(joined_list));
}

fn sync_loose_up_inner(
    doc: &mut Document,
    start: NodeId,
    changes: &mut Vec<DocChange>,
    clear_source_loose: Option<NodeId>,
) {
    let mut cur = Some(start);
    while let Some(id) = cur {
        let Some(node) = doc.arena.get(id) else {
            return;
        };
        let parent = node.parent;
        if node.kind == BlockKind::List {
            sync_loose(doc, id, changes, clear_source_loose == Some(id));
        }
        cur = parent;
    }
}

fn sync_loose(
    doc: &mut Document,
    list: NodeId,
    changes: &mut Vec<DocChange>,
    clear_source_loose: bool,
) {
    let extra = doc.extra(list);
    let structural_loose = doc
        .arena
        .children(list)
        .any(|item| doc.arena.children(item).nth(1).is_some());
    let source_loose = extra.list_source_loose() && !clear_source_loose;
    let want = source_loose || structural_loose;
    if extra.list_loose() == want && extra.list_source_loose() == source_loose {
        return;
    }
    let old_extra = extra;
    doc.set_extra(
        list,
        NodeExtra::List {
            start: extra.ordered_start(),
            marker: extra.list_marker(),
            loose: want,
            source_loose,
        },
    );
    changes.push(doc.attrs_change(list, BlockKind::List, old_extra));
}

impl Document {
    pub(crate) fn ensure_trailing_blank(&mut self) {
        let _ = ensure_trailing_blank_paragraph(self, None);
    }

    pub(crate) fn ensure_trailing_blank_at(&mut self, caret: Caret) -> Caret {
        ensure_trailing_blank_paragraph(self, Some(caret))
    }

    pub(crate) fn clamp_live_caret(&self, caret: Caret) -> Caret {
        if let Some(id) = self.live_id(caret.block) {
            let text = self.caret_text(id);
            let offset = crate::document::floor_char_boundary(text, caret.offset.min(text.len()));
            return Caret {
                block: caret.block,
                offset,
            };
        }
        match self.first_text_leaf() {
            Some(block) => Caret { block, offset: 0 },
            None => caret,
        }
    }
}

pub(crate) fn is_blank_paragraph(doc: &Document, id: NodeId) -> bool {
    doc.arena
        .get(id)
        .is_some_and(|n| n.kind == BlockKind::Paragraph)
        && matches!(doc.extra(id), NodeExtra::None)
        && doc.display(id).is_empty()
        && doc.leaf_source(id).trim().is_empty()
}

pub(crate) fn ensure_trailing_blank_paragraph(doc: &mut Document, caret: Option<Caret>) -> Caret {
    let before_rev = doc.revision();
    let mut changes = Vec::new();
    let root = doc.root;
    loop {
        let last = doc.arena.get(root).and_then(|n| n.last_child);
        let prev = last.and_then(|id| doc.arena.get(id).and_then(|n| n.prev_sibling));
        if last.is_some_and(|id| is_blank_paragraph(doc, id))
            && prev.is_some_and(|id| is_blank_paragraph(doc, id))
        {
            let id = last.expect("last");
            let drop_sentinel = match caret {
                None => true,
                Some(c) => prev.is_some_and(|p| c.block == p.index),
            };
            if !drop_sentinel {
                break;
            }
            tombstone(doc, id);
            doc.bump_structure(root);
            changes.push(DocChange::TreeSpliced {
                parent: root,
                before: prev,
                removed: vec![id],
                inserted: Vec::new(),
            });
            continue;
        }
        break;
    }
    let last = doc.arena.get(root).and_then(|n| n.last_child);
    if let Some(id) = last.filter(|&id| is_blank_paragraph(doc, id)) {
        if !changes.is_empty() {
            let _ = doc.commit(before_rev, changes);
        }
        return caret.unwrap_or(Caret {
            block: id.index,
            offset: 0,
        });
    }
    let para = doc.alloc_leaf(BlockKind::Paragraph);
    doc.arena.append_child(root, para);
    doc.bump_structure(root);
    changes.push(DocChange::TreeSpliced {
        parent: root,
        before: last,
        removed: Vec::new(),
        inserted: vec![para],
    });
    let _ = doc.commit(before_rev, changes);
    caret.unwrap_or(Caret {
        block: para.index,
        offset: 0,
    })
}
